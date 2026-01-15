//! # errat - Lightweight error location tracking
//!
//! A minimal error tracing library that adds location tracking to any error type
//! with small `sizeof` overhead and `no_std` support.
//!
//! ## Design Goals
//!
//! - **Small sizeof**: `Traced<E>` is only `sizeof(E) + 8` bytes (one pointer for boxed trace)
//! - **Zero allocation on Ok path**: No heap allocation until `.traced()` is called on an error
//! - **Ergonomic API**: `.traced()` on errors, `.at()` on `Result`s
//! - **Optional context messages**: Add context with `.at_msg("context")`
//! - **no_std compatible**: Works with just `core` + `alloc`, `std` is optional
//! - **Fallible allocations**: Trace operations silently fail on OOM; error still propagates
//!
//! ## Allocation Failure Behavior
//!
//! Vec and String allocations use stable `try_reserve` APIs and silently fail on OOM.
//! Box allocations use `Box::new` (Box::try_new is not yet stable) which can panic on OOM.
//!
//! If memory allocation fails:
//! - Vec/String trace entries are silently skipped
//! - The error `E` itself always propagates (it's stored inline in `Traced<E>`)
//! - Box allocation failure will panic (rare in practice)
//!
//! ## Example
//!
//! ```rust
//! use errat::{Traced, Traceable, ResultExt};
//!
//! #[derive(Debug)]
//! enum MyError {
//!     NotFound,
//!     InvalidInput(String),
//! }
//!
//! fn inner() -> Result<(), Traced<MyError>> {
//!     Err(MyError::NotFound.traced())
//! }
//!
//! fn outer() -> Result<(), Traced<MyError>> {
//!     inner().at_msg("while fetching user")?;
//!     Ok(())
//! }
//!
//! let err = outer().unwrap_err();
//! // Note: trace_len may be less than expected if allocations failed
//! assert!(err.trace_len() <= 2);
//! ```

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::panic::Location;

// ============================================================================
// Fallible Allocation Helpers
// ============================================================================
//
// Uses stable try_reserve APIs where available. Box::try_new is not yet stable,
// so Box allocations use regular Box::new which can panic on OOM.
// In practice, OOM panics are rare and the error itself still propagates
// (since E is stored inline in Traced<E>).

/// Try to allocate a Box. Returns Some on success.
/// Note: Box::try_new is not yet stable, so this can panic on OOM.
/// The error E is stored inline, so even if tracing fails, the error propagates.
#[inline]
fn try_box<T>(value: T) -> Option<Box<T>> {
    // TODO: Use Box::try_new when stabilized
    Some(Box::new(value))
}

/// Try to push a value onto a Vec, returning false on allocation failure.
#[inline]
fn try_push<T>(vec: &mut Vec<T>, value: T) -> bool {
    if vec.try_reserve(1).is_err() {
        return false;
    }
    vec.push(value);
    true
}

/// Try to create a Vec with the given capacity, returning None on failure.
#[inline]
fn try_vec_with_capacity<T>(capacity: usize) -> Option<Vec<T>> {
    let mut vec = Vec::new();
    if vec.try_reserve(capacity).is_err() {
        return None;
    }
    Some(vec)
}

/// Try to convert a &str to String, returning None on allocation failure.
#[inline]
fn try_string_from(s: &str) -> Option<String> {
    let mut string = String::new();
    if string.try_reserve(s.len()).is_err() {
        return None;
    }
    string.push_str(s);
    Some(string)
}

// ============================================================================
// Core Types
// ============================================================================

/// A traced error that wraps any error type with location tracking.
///
/// ## Size
///
/// `Traced<E>` is `sizeof(E) + 8` bytes on 64-bit platforms:
/// - The error `E` is stored inline
/// - The trace is boxed (8-byte pointer, null when empty)
///
/// ## Example
///
/// ```rust
/// use errat::{Traced, Traceable};
///
/// #[derive(Debug)]
/// enum MyError { Oops }
///
/// // Create a traced error
/// let err: Traced<MyError> = MyError::Oops.traced();
/// assert_eq!(err.trace_len(), 1);
/// ```
pub struct Traced<E> {
    error: E,
    trace: Option<Box<Trace>>,
}

// ============================================================================
// DebugAny Trait - combines Any + Debug in a single trait object
// ============================================================================

/// Trait combining `Any` and `Debug` for type-erased context data.
///
/// This allows storing arbitrary typed data while still being able to:
/// - Debug-print it
/// - Downcast it back to the original type
pub trait DebugAny: core::any::Any + fmt::Debug + Send + Sync {
    /// Get a reference to self as `&dyn Any` for downcasting.
    fn as_any(&self) -> &dyn core::any::Any;

    /// Get the type name for diagnostics.
    fn type_name(&self) -> &'static str;
}

impl<T: core::any::Any + fmt::Debug + Send + Sync> DebugAny for T {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn type_name(&self) -> &'static str {
        core::any::type_name::<T>()
    }
}

// ============================================================================
// DisplayAny Trait - combines Any + Display in a single trait object
// ============================================================================

/// Trait combining `Any` and `Display` for type-erased context data.
///
/// Similar to `DebugAny` but for types that implement `Display`.
/// Use this when you want human-readable output instead of debug format.
pub trait DisplayAny: core::any::Any + fmt::Display + Send + Sync {
    /// Get a reference to self as `&dyn Any` for downcasting.
    fn as_any(&self) -> &dyn core::any::Any;

    /// Get the type name for diagnostics.
    fn type_name(&self) -> &'static str;
}

impl<T: core::any::Any + fmt::Display + Send + Sync> DisplayAny for T {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn type_name(&self) -> &'static str {
        core::any::type_name::<T>()
    }
}

// ============================================================================
// Context Enum
// ============================================================================

/// Context data attached to a trace segment.
///
/// Can be a simple string message or arbitrary typed data with either
/// Debug or Display formatting.
pub enum Context {
    /// A text message describing what operation was being performed.
    Text(String),
    /// Typed context data formatted via Debug.
    Debug(Box<dyn DebugAny>),
    /// Typed context data formatted via Display.
    Display(Box<dyn DisplayAny>),
}

impl Context {
    /// Get as text, if this is a Text variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Context::Text(s) => Some(s),
            Context::Debug(_) | Context::Display(_) => None,
        }
    }

    /// Try to downcast to a specific type, if this is a typed variant.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        match self {
            Context::Text(_) => None,
            // Must use (**b) to call as_any on the trait object, not the Box
            // (Box<dyn DebugAny> itself implements DebugAny through the blanket impl)
            Context::Debug(b) => (**b).as_any().downcast_ref(),
            Context::Display(b) => (**b).as_any().downcast_ref(),
        }
    }

    /// Get the type name if this is a typed variant.
    pub fn type_name(&self) -> Option<&'static str> {
        match self {
            Context::Text(_) => None,
            Context::Debug(b) => Some((**b).type_name()),
            Context::Display(b) => Some((**b).type_name()),
        }
    }

    /// Check if this context uses Display formatting.
    pub fn is_display(&self) -> bool {
        matches!(self, Context::Text(_) | Context::Display(_))
    }
}

impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Context::Text(s) => write!(f, "{:?}", s),
            Context::Debug(t) => write!(f, "{:?}", &**t),
            Context::Display(t) => write!(f, "{}", &**t), // Display types use Display even in Debug
        }
    }
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Context::Text(s) => write!(f, "{}", s),
            Context::Debug(t) => write!(f, "{:?}", &**t), // Debug types use Debug in Display
            Context::Display(t) => write!(f, "{}", &**t),
        }
    }
}

/// A segment in the trace chain - contains a location, optional context, and link to previous.
struct Segment {
    location: &'static Location<'static>,
    context: Option<Context>,
    /// Locations accumulated between this segment and the next one.
    locations_after: Vec<&'static Location<'static>>,
    /// Link to previous segment (older context).
    prev: Option<Box<Segment>>,
}

/// Internal trace storage - boxed to keep Traced<E> small.
///
/// Structure: linked list of Segments (newest first), plus recent locations.
struct Trace {
    /// Most recent segment (head of linked list).
    head: Option<Box<Segment>>,
    /// Locations accumulated since the last context was added.
    recent: Vec<&'static Location<'static>>,
}

impl Trace {
    fn new() -> Self {
        Self {
            head: None,
            recent: Vec::new(),
        }
    }

    /// Try to create a Trace with pre-allocated capacity.
    /// Returns None if allocation fails.
    fn try_with_capacity(cap: usize) -> Option<Self> {
        Some(Self {
            head: None,
            recent: try_vec_with_capacity(cap)?,
        })
    }

    /// Try to push a location. Returns false if allocation fails.
    #[inline]
    fn try_push(&mut self, loc: &'static Location<'static>) -> bool {
        try_push(&mut self.recent, loc)
    }

    /// Try to push a location with context, creating a new segment.
    /// On allocation failure, the context is lost but existing trace data is preserved.
    fn try_push_with_context(&mut self, loc: &'static Location<'static>, context: Context) {
        // Take the recent locations and create a new segment
        let locations_after = core::mem::take(&mut self.recent);
        let prev = self.head.take();

        match try_box(Segment {
            location: loc,
            context: Some(context),
            locations_after,
            prev,
        }) {
            Some(segment) => {
                self.head = Some(segment);
            }
            None => {
                // Allocation failed - context is lost, but that's acceptable
                // We could try to preserve the prev chain but that adds complexity
                // In OOM scenarios, losing trace data is acceptable
            }
        }
    }

    fn len(&self) -> usize {
        let mut count = self.recent.len();
        let mut seg = self.head.as_ref();
        while let Some(s) = seg {
            count += 1 + s.locations_after.len();
            seg = s.prev.as_ref();
        }
        count
    }

    /// Iterate over all locations, oldest first.
    fn iter(&self) -> impl Iterator<Item = &'static Location<'static>> + '_ {
        TraceIter::new(self)
    }

    /// Get the most recent context message (text only).
    fn message(&self) -> Option<&str> {
        let mut seg = self.head.as_ref();
        while let Some(s) = seg {
            if let Some(Context::Text(ref msg)) = s.context {
                return Some(msg);
            }
            seg = s.prev.as_ref();
        }
        None
    }

    /// Iterate over all context entries, newest first.
    fn contexts(&self) -> impl Iterator<Item = &Context> {
        ContextIter {
            current: self.head.as_deref(),
        }
    }
}

/// Iterator over locations in a trace (oldest first).
struct TraceIter<'a> {
    /// Stack of segments to visit (we'll pop and process).
    segments: Vec<&'a Segment>,
    /// Current phase within a segment.
    phase: TraceIterPhase<'a>,
    /// Recent locations (processed last).
    recent: &'a [&'static Location<'static>],
    recent_idx: usize,
}

enum TraceIterPhase<'a> {
    /// About to yield the segment's main location.
    SegmentLocation(&'a Segment),
    /// Yielding locations_after.
    LocationsAfter(&'a [&'static Location<'static>], usize),
    /// Done with segments, now yielding recent.
    Recent,
}

impl<'a> TraceIter<'a> {
    fn new(trace: &'a Trace) -> Self {
        // Build stack of segments (oldest first by pushing in reverse order).
        let mut segments = Vec::new();
        let mut seg = trace.head.as_ref();
        while let Some(s) = seg {
            segments.push(s.as_ref());
            seg = s.prev.as_ref();
        }
        // segments is now newest-first, we want oldest-first
        segments.reverse();

        let phase = if let Some(first) = segments.pop() {
            TraceIterPhase::SegmentLocation(first)
        } else {
            TraceIterPhase::Recent
        };

        Self {
            segments,
            phase,
            recent: &trace.recent,
            recent_idx: 0,
        }
    }
}

impl<'a> Iterator for TraceIter<'a> {
    type Item = &'static Location<'static>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.phase {
                TraceIterPhase::SegmentLocation(seg) => {
                    let loc = seg.location;
                    self.phase = TraceIterPhase::LocationsAfter(&seg.locations_after, 0);
                    return Some(loc);
                }
                TraceIterPhase::LocationsAfter(locs, idx) => {
                    if *idx < locs.len() {
                        let loc = locs[*idx];
                        *idx += 1;
                        return Some(loc);
                    }
                    // Done with this segment, move to next
                    if let Some(next_seg) = self.segments.pop() {
                        self.phase = TraceIterPhase::SegmentLocation(next_seg);
                    } else {
                        self.phase = TraceIterPhase::Recent;
                    }
                }
                TraceIterPhase::Recent => {
                    if self.recent_idx < self.recent.len() {
                        let loc = self.recent[self.recent_idx];
                        self.recent_idx += 1;
                        return Some(loc);
                    }
                    return None;
                }
            }
        }
    }
}

/// Iterator over context entries (newest first).
struct ContextIter<'a> {
    current: Option<&'a Segment>,
}

impl<'a> Iterator for ContextIter<'a> {
    type Item = &'a Context;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(seg) = self.current.take() {
            self.current = seg.prev.as_deref();
            if let Some(ref ctx) = seg.context {
                return Some(ctx);
            }
        }
        None
    }
}

// ============================================================================
// Traced<E> Implementation
// ============================================================================

impl<E> Traced<E> {
    /// Create a new traced error without any location information.
    ///
    /// Use `.at()` to add the first location, or use the `Traceable::at()` method
    /// on the error directly.
    #[inline]
    pub const fn new(error: E) -> Self {
        Self { error, trace: None }
    }

    /// Add the caller's location to the trace.
    ///
    /// This is the primary API for building up a stack trace as errors propagate.
    /// If allocation fails, the location is silently skipped.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::Traced;
    ///
    /// #[derive(Debug)]
    /// enum MyError { Oops }
    ///
    /// fn inner() -> Result<(), Traced<MyError>> {
    ///     Err(Traced::new(MyError::Oops).at())
    /// }
    ///
    /// fn outer() -> Result<(), Traced<MyError>> {
    ///     inner().map_err(|e| e.at())
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn at(mut self) -> Self {
        let loc = Location::caller();
        match &mut self.trace {
            Some(trace) => {
                // Silently ignore if push fails
                let _ = trace.try_push(loc);
            }
            None => {
                // Try to create trace with capacity, fall back to no capacity
                if let Some(mut trace) = Trace::try_with_capacity(6) {
                    let _ = trace.try_push(loc);
                    if let Some(boxed) = try_box(trace) {
                        self.trace = Some(boxed);
                    }
                } else if let Some(mut trace) = Some(Trace::new()) {
                    let _ = trace.try_push(loc);
                    if let Some(boxed) = try_box(trace) {
                        self.trace = Some(boxed);
                    }
                }
            }
        }
        self
    }

    /// Add the caller's location and a context message to the trace.
    ///
    /// Each context message creates a new segment in the trace, preserving all
    /// previous context. Use this to add human-readable context about what
    /// operation was being performed.
    /// If allocation fails, the context is silently skipped.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{Traced, Traceable, ResultExt};
    ///
    /// #[derive(Debug)]
    /// enum MyError { IoError }
    ///
    /// fn read_config() -> Result<(), Traced<MyError>> {
    ///     Err(MyError::IoError.traced())
    /// }
    ///
    /// fn init() -> Result<(), Traced<MyError>> {
    ///     read_config().at_msg("while loading configuration")?;
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn at_msg(mut self, msg: &str) -> Self {
        let loc = Location::caller();
        // Try to allocate the string first
        let Some(text) = try_string_from(msg) else {
            return self;
        };
        let context = Context::Text(text);

        match &mut self.trace {
            Some(trace) => {
                trace.try_push_with_context(loc, context);
            }
            None => {
                let mut trace = Trace::new();
                trace.try_push_with_context(loc, context);
                if let Some(boxed) = try_box(trace) {
                    self.trace = Some(boxed);
                }
            }
        }
        self
    }

    /// Add the caller's location and arbitrary typed context to the trace (Debug formatted).
    ///
    /// This allows attaching any `Debug + Send + Sync + 'static` data to the error trace.
    /// The context will be formatted using `Debug` when displayed.
    /// Use `contexts()` to retrieve context entries and `downcast_ref` to access them.
    /// If allocation fails, the context is silently skipped.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{Traced, Traceable, Context};
    ///
    /// #[derive(Debug)]
    /// struct RequestInfo { user_id: u64, path: String }
    ///
    /// #[derive(Debug)]
    /// enum MyError { Forbidden }
    ///
    /// let err = MyError::Forbidden
    ///     .traced()
    ///     .at_context(RequestInfo { user_id: 42, path: "/admin".into() });
    ///
    /// // Later, retrieve the context
    /// for ctx in err.contexts() {
    ///     if let Some(req) = ctx.downcast_ref::<RequestInfo>() {
    ///         assert_eq!(req.user_id, 42);
    ///     }
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn at_context<T: fmt::Debug + Send + Sync + 'static>(mut self, ctx: T) -> Self {
        let loc = Location::caller();
        // Try to box the context first
        let Some(boxed_ctx) = try_box(ctx) else {
            return self;
        };
        let context = Context::Debug(boxed_ctx);

        match &mut self.trace {
            Some(trace) => {
                trace.try_push_with_context(loc, context);
            }
            None => {
                let mut trace = Trace::new();
                trace.try_push_with_context(loc, context);
                if let Some(boxed) = try_box(trace) {
                    self.trace = Some(boxed);
                }
            }
        }
        self
    }

    /// Add the caller's location and arbitrary typed context to the trace (Display formatted).
    ///
    /// Similar to `at_context`, but the context will be formatted using `Display` instead of `Debug`.
    /// Use this for more human-readable context output.
    /// If allocation fails, the context is silently skipped.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{Traced, Traceable};
    ///
    /// #[derive(Debug)]
    /// enum MyError { NotFound }
    ///
    /// let err = MyError::NotFound
    ///     .traced()
    ///     .at_context_display("loading config file /etc/config.toml");
    /// ```
    #[track_caller]
    #[inline]
    pub fn at_context_display<T: fmt::Display + Send + Sync + 'static>(mut self, ctx: T) -> Self {
        let loc = Location::caller();
        // Try to box the context first
        let Some(boxed_ctx) = try_box(ctx) else {
            return self;
        };
        let context = Context::Display(boxed_ctx);

        match &mut self.trace {
            Some(trace) => {
                trace.try_push_with_context(loc, context);
            }
            None => {
                let mut trace = Trace::new();
                trace.try_push_with_context(loc, context);
                if let Some(boxed) = try_box(trace) {
                    self.trace = Some(boxed);
                }
            }
        }
        self
    }

    /// Get a reference to the inner error.
    #[inline]
    pub fn error(&self) -> &E {
        &self.error
    }

    /// Get a mutable reference to the inner error.
    #[inline]
    pub fn error_mut(&mut self) -> &mut E {
        &mut self.error
    }

    /// Consume self and return the inner error, discarding the trace.
    #[inline]
    pub fn into_inner(self) -> E {
        self.error
    }

    /// Get the number of locations in the trace.
    #[inline]
    pub fn trace_len(&self) -> usize {
        self.trace.as_ref().map_or(0, |t| t.len())
    }

    /// Check if the trace is empty.
    #[inline]
    pub fn trace_is_empty(&self) -> bool {
        self.trace.is_none()
    }

    /// Iterate over all traced locations, oldest first.
    #[inline]
    pub fn trace_iter(&self) -> impl Iterator<Item = &'static Location<'static>> + '_ {
        self.trace.iter().flat_map(|t| t.iter())
    }

    /// Get the first (oldest) location in the trace, if any.
    #[inline]
    pub fn first_location(&self) -> Option<&'static Location<'static>> {
        self.trace_iter().next()
    }

    /// Get the last (most recent) location in the trace, if any.
    #[inline]
    pub fn last_location(&self) -> Option<&'static Location<'static>> {
        self.trace_iter().last()
    }

    /// Get the most recent context message (text only), if any was set via `at_msg()`.
    #[inline]
    pub fn message(&self) -> Option<&str> {
        self.trace.as_ref().and_then(|t| t.message())
    }

    /// Iterate over all context entries, newest first.
    ///
    /// Each call to `at_msg()` or `at_context()` creates a context entry.
    pub fn contexts(&self) -> impl Iterator<Item = &Context> {
        self.trace.iter().flat_map(|t| t.contexts())
    }

    /// Add a context message without a location.
    ///
    /// Prefer `at_msg()` which also captures the caller's location.
    #[inline]
    #[track_caller]
    pub fn with_message(self, msg: &str) -> Self {
        self.at_msg(msg)
    }
}

impl<E: fmt::Debug> fmt::Debug for Traced<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{:?}", self.error)?;
        if let Some(trace) = &self.trace {
            // Show all contexts (newest first)
            for ctx in trace.contexts() {
                match ctx {
                    Context::Text(msg) => writeln!(f, "  context: {}", msg)?,
                    Context::Debug(t) => writeln!(f, "  context: {:?}", &**t)?,
                    Context::Display(t) => writeln!(f, "  context: {}", &**t)?,
                }
            }
            // Show all locations (oldest first)
            for loc in trace.iter() {
                writeln!(f, "  at {}:{}:{}", loc.file(), loc.line(), loc.column())?;
            }
        }
        Ok(())
    }
}

impl<E: fmt::Display> fmt::Display for Traced<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl<E: fmt::Debug + fmt::Display> core::error::Error for Traced<E> {}

// ============================================================================
// Traceable Trait - for calling .at() directly on error values
// ============================================================================

/// Extension trait that allows calling `.traced()` directly on any error value.
///
/// This is implemented for all types, allowing ergonomic error creation:
///
/// ```rust
/// use errat::Traceable;
///
/// #[derive(Debug)]
/// enum MyError { NotFound }
///
/// let err = MyError::NotFound.traced();
/// // trace_len may be 0 if allocation failed, but error propagates
/// assert!(err.trace_len() <= 1);
/// ```
pub trait Traceable: Sized {
    /// Wrap this value in a `Traced` and add the caller's location.
    /// If allocation fails, the error is still wrapped but trace may be empty.
    #[track_caller]
    fn traced(self) -> Traced<Self>;

    /// Wrap this value in a `Traced` and add the caller's location with a message.
    /// If allocation fails, the error is still wrapped but trace may be empty.
    #[track_caller]
    fn traced_msg(self, msg: &str) -> Traced<Self>;
}

impl<E> Traceable for E {
    #[track_caller]
    #[inline]
    fn traced(self) -> Traced<Self> {
        Traced::new(self).at()
    }

    #[track_caller]
    #[inline]
    fn traced_msg(self, msg: &str) -> Traced<Self> {
        Traced::new(self).at_msg(msg)
    }
}

// ============================================================================
// ResultExt Trait - for calling .at() on Results
// ============================================================================

/// Extension trait for adding location tracking to `Result` types.
///
/// ## Example
///
/// ```rust
/// use errat::{Traced, Traceable, ResultExt};
///
/// #[derive(Debug)]
/// enum MyError { Oops }
///
/// fn inner() -> Result<(), Traced<MyError>> {
///     Err(MyError::Oops.traced())
/// }
///
/// fn outer() -> Result<(), Traced<MyError>> {
///     inner().at()?;
///     Ok(())
/// }
/// ```
pub trait ResultExt<T, E> {
    /// Add the caller's location to the error trace if this is `Err`.
    ///
    /// This is a no-op on the `Ok` path. If allocation fails, the error
    /// still propagates but the location is silently skipped.
    #[track_caller]
    fn at(self) -> Result<T, Traced<E>>;

    /// Add the caller's location and a context message if this is `Err`.
    ///
    /// This is a no-op on the `Ok` path. If allocation fails, the error
    /// still propagates but the context is silently skipped.
    #[track_caller]
    fn at_msg(self, msg: &str) -> Result<T, Traced<E>>;
}

impl<T, E> ResultExt<T, E> for Result<T, Traced<E>> {
    #[track_caller]
    #[inline]
    fn at(self) -> Result<T, Traced<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at()),
        }
    }

    #[track_caller]
    #[inline]
    fn at_msg(self, msg: &str) -> Result<T, Traced<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_msg(msg)),
        }
    }
}

/// Extension trait for converting non-traced errors to traced errors.
///
/// This allows `.trace()` to be called on `Result<T, E>` where `E` is not yet `Traced`.
///
/// ## Example
///
/// ```rust
/// use errat::ResultTraceExt;
///
/// fn fallible() -> Result<(), &'static str> {
///     Err("something went wrong")
/// }
///
/// fn wrapper() -> Result<(), errat::Traced<&'static str>> {
///     fallible().trace()?;
///     Ok(())
/// }
/// ```
pub trait ResultTraceExt<T, E> {
    /// Convert the error to a `Traced<E>` and add the caller's location.
    /// If allocation fails, the error is still wrapped but trace may be empty.
    #[track_caller]
    fn trace(self) -> Result<T, Traced<E>>;

    /// Convert the error to a `Traced<E>` and add the caller's location with a message.
    /// If allocation fails, the error is still wrapped but trace may be empty.
    #[track_caller]
    fn trace_msg(self, msg: &str) -> Result<T, Traced<E>>;
}

impl<T, E> ResultTraceExt<T, E> for Result<T, E> {
    #[track_caller]
    #[inline]
    fn trace(self) -> Result<T, Traced<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(Traced::new(e).at()),
        }
    }

    #[track_caller]
    #[inline]
    fn trace_msg(self, msg: &str) -> Result<T, Traced<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(Traced::new(e).at_msg(msg)),
        }
    }
}

// ============================================================================
// From implementations
// ============================================================================

impl<E> From<E> for Traced<E> {
    #[inline]
    fn from(error: E) -> Self {
        Traced::new(error)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[derive(Debug, PartialEq)]
    enum TestError {
        NotFound,
        InvalidInput,
    }

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                TestError::NotFound => write!(f, "not found"),
                TestError::InvalidInput => write!(f, "invalid input"),
            }
        }
    }

    #[test]
    fn test_sizeof() {
        use core::mem::size_of;

        // Traced<E> should be sizeof(E) + 8 (pointer to boxed trace)
        // With alignment, a 1-byte enum becomes 16 bytes total
        assert_eq!(size_of::<Option<Box<Trace>>>(), 8);

        let traced_size = size_of::<Traced<TestError>>();
        let error_size = size_of::<TestError>();
        let pointer_size = size_of::<Option<Box<Trace>>>();

        // Should be error + pointer, with possible padding
        assert!(traced_size <= error_size + pointer_size + 8); // Allow for alignment
        assert!(traced_size >= error_size + pointer_size);

        // For a 1-byte enum, should be 16 bytes (1 + 7 padding + 8 pointer)
        assert_eq!(traced_size, 16);
    }

    #[test]
    fn test_basic_trace() {
        let err = TestError::NotFound.traced();
        assert_eq!(*err.error(), TestError::NotFound);
        assert_eq!(err.trace_len(), 1);
        assert!(!err.trace_is_empty());
    }

    #[test]
    fn test_propagation() {
        fn inner() -> Result<(), Traced<TestError>> {
            Err(TestError::NotFound.traced())
        }

        fn middle() -> Result<(), Traced<TestError>> {
            inner().at()
        }

        fn outer() -> Result<(), Traced<TestError>> {
            middle().at()
        }

        let err = outer().unwrap_err();
        assert_eq!(err.trace_len(), 3);

        // Verify locations are captured
        let locations: Vec<_> = err.trace_iter().collect();
        assert_eq!(locations.len(), 3);

        // All locations should be in this file
        for loc in &locations {
            assert!(loc.file().contains("lib.rs"));
        }
    }

    #[test]
    fn test_result_trace_ext() {
        fn fallible() -> Result<(), &'static str> {
            Err("oops")
        }

        fn wrapper() -> Result<(), Traced<&'static str>> {
            fallible().trace()?;
            Ok(())
        }

        let err = wrapper().unwrap_err();
        assert_eq!(*err.error(), "oops");
        assert_eq!(err.trace_len(), 1);
    }

    #[test]
    fn test_into_inner() {
        let err = TestError::InvalidInput.traced();
        let inner = err.into_inner();
        assert_eq!(inner, TestError::InvalidInput);
    }

    #[test]
    fn test_first_last_location() {
        fn level1() -> Result<(), Traced<TestError>> {
            Err(TestError::NotFound.traced())
        }

        fn level2() -> Result<(), Traced<TestError>> {
            level1().at()
        }

        fn level3() -> Result<(), Traced<TestError>> {
            level2().at()
        }

        let err = level3().unwrap_err();

        let first = err.first_location().unwrap();
        let last = err.last_location().unwrap();

        // First should be from level1, last from level3
        assert!(first.line() < last.line());
    }

    #[test]
    fn test_display_debug() {
        let err = TestError::NotFound.traced();

        // Display should just show the error
        let display = alloc::format!("{}", err);
        assert_eq!(display, "not found");

        // Debug should include trace
        let debug = alloc::format!("{:?}", err);
        assert!(debug.contains("NotFound"));
        assert!(debug.contains("at"));
        assert!(debug.contains("lib.rs"));
    }

    #[test]
    fn test_no_trace() {
        let err: Traced<TestError> = Traced::new(TestError::NotFound);
        assert_eq!(err.trace_len(), 0);
        assert!(err.trace_is_empty());
        assert!(err.first_location().is_none());
        assert!(err.last_location().is_none());
    }

    #[test]
    fn test_from_impl() {
        let err: Traced<TestError> = TestError::NotFound.into();
        assert_eq!(*err.error(), TestError::NotFound);
        assert!(err.trace_is_empty()); // From doesn't add trace
    }

    #[test]
    fn test_error_mut() {
        #[derive(Debug)]
        struct MutableError {
            count: u32,
        }

        let mut err = MutableError { count: 0 }.traced();
        err.error_mut().count = 42;
        assert_eq!(err.error().count, 42);
    }

    #[test]
    fn test_larger_error_type() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct LargeError {
            message: String,
            code: u64,
            data: [u8; 32],
        }

        let err = LargeError {
            message: String::from("test"),
            code: 42,
            data: [0; 32],
        }
        .traced();

        assert_eq!(err.trace_len(), 1);
        assert_eq!(err.error().code, 42);
    }

    #[test]
    fn test_at_msg() {
        let err = TestError::NotFound.traced_msg("while fetching user");
        assert_eq!(err.trace_len(), 1);
        assert_eq!(err.message(), Some("while fetching user"));
    }

    #[test]
    fn test_at_msg_propagation() {
        fn inner() -> Result<(), Traced<TestError>> {
            Err(TestError::NotFound.traced())
        }

        fn outer() -> Result<(), Traced<TestError>> {
            inner().at_msg("during initialization")?;
            Ok(())
        }

        let err = outer().unwrap_err();
        assert_eq!(err.trace_len(), 2);
        assert_eq!(err.message(), Some("during initialization"));
    }

    #[test]
    fn test_with_message() {
        let err = TestError::NotFound.traced().with_message("custom context");
        assert_eq!(err.message(), Some("custom context"));
    }

    #[test]
    fn test_trace_msg() {
        fn fallible() -> Result<(), &'static str> {
            Err("oops")
        }

        fn wrapper() -> Result<(), Traced<&'static str>> {
            fallible().trace_msg("while doing something")?;
            Ok(())
        }

        let err = wrapper().unwrap_err();
        assert_eq!(*err.error(), "oops");
        assert_eq!(err.message(), Some("while doing something"));
    }

    #[test]
    fn test_debug_with_message() {
        let err = TestError::NotFound.traced_msg("context info");
        let debug = alloc::format!("{:?}", err);
        assert!(debug.contains("NotFound"));
        assert!(debug.contains("context: context info"));
        assert!(debug.contains("lib.rs"));
    }

    #[test]
    fn test_at_context_typed() {
        #[derive(Debug)]
        struct RequestInfo {
            user_id: u64,
        }

        let err = TestError::NotFound
            .traced()
            .at_context(RequestInfo { user_id: 42 });

        assert_eq!(err.trace_len(), 2);

        // Retrieve typed context
        let mut found = false;
        for ctx in err.contexts() {
            if let Some(req) = ctx.downcast_ref::<RequestInfo>() {
                assert_eq!(req.user_id, 42);
                found = true;
            }
        }
        assert!(found, "should find RequestInfo context");
    }

    #[test]
    fn test_multiple_contexts() {
        fn level1() -> Result<(), Traced<TestError>> {
            Err(TestError::NotFound.traced())
        }

        fn level2() -> Result<(), Traced<TestError>> {
            level1().at_msg("in level2")?;
            Ok(())
        }

        fn level3() -> Result<(), Traced<TestError>> {
            level2().at_msg("in level3")?;
            Ok(())
        }

        let err = level3().unwrap_err();

        // Should have 3 locations
        assert_eq!(err.trace_len(), 3);

        // Should have 2 context messages (level2 and level3)
        let contexts: Vec<_> = err.contexts().collect();
        assert_eq!(contexts.len(), 2);

        // Most recent first
        assert_eq!(contexts[0].as_text(), Some("in level3"));
        assert_eq!(contexts[1].as_text(), Some("in level2"));
    }

    #[test]
    fn test_context_enum() {
        use super::Context;

        let text_ctx = Context::Text(String::from("hello"));
        assert_eq!(text_ctx.as_text(), Some("hello"));
        assert!(text_ctx.downcast_ref::<u32>().is_none());

        // Debug context - requires Debug (u32 implements Debug)
        let debug_ctx = Context::Debug(Box::new(42u32));
        assert_eq!(debug_ctx.as_text(), None);
        assert_eq!(debug_ctx.downcast_ref::<u32>(), Some(&42));

        // Verify Debug output works
        let debug_str = alloc::format!("{:?}", debug_ctx);
        assert!(debug_str.contains("42"));

        // Display context - requires Display (u32 implements Display)
        let display_ctx = Context::Display(Box::new(99u32));
        assert_eq!(display_ctx.as_text(), None);
        assert_eq!(display_ctx.downcast_ref::<u32>(), Some(&99));

        // Verify Display output works
        let display_str = alloc::format!("{}", display_ctx);
        assert!(display_str.contains("99"));

        // is_display should be true for Text and Display
        assert!(text_ctx.is_display());
        assert!(!debug_ctx.is_display());
        assert!(display_ctx.is_display());
    }

    #[test]
    fn test_typed_context_debug_output() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct MyContext {
            id: u64,
            name: &'static str,
        }

        let err = TestError::NotFound.traced().at_context(MyContext {
            id: 123,
            name: "test",
        });

        let debug = alloc::format!("{:?}", err);
        // Should contain the Debug output of MyContext
        assert!(debug.contains("MyContext"));
        assert!(debug.contains("123"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn test_at_context_display() {
        // Use a type that has both Display and Debug but we want Display formatting
        let err = TestError::NotFound
            .traced()
            .at_context_display("user-friendly message");

        assert_eq!(err.trace_len(), 2);

        // Check that Display formatting is used in output
        let debug = alloc::format!("{:?}", err);
        assert!(debug.contains("context: user-friendly message"));

        // Downcast should still work
        let mut found = false;
        for ctx in err.contexts() {
            if ctx.downcast_ref::<&str>().is_some() {
                found = true;
                assert!(ctx.is_display());
            }
        }
        assert!(found, "should find string context");
    }

    #[test]
    fn test_mixed_context_types() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct DebugInfo {
            code: u32,
        }

        let err = TestError::NotFound
            .traced()
            .at_msg("text message")
            .at_context(DebugInfo { code: 42 })
            .at_context_display("display message");

        // Should have 4 locations (traced + 3 context methods)
        assert_eq!(err.trace_len(), 4);

        // Should have 3 contexts
        let contexts: Vec<_> = err.contexts().collect();
        assert_eq!(contexts.len(), 3);

        // Most recent first (display, debug, text)
        assert!(contexts[0].is_display()); // display message
        assert!(!contexts[1].is_display()); // DebugInfo (Debug)
        assert!(contexts[2].is_display()); // text message
    }
}
