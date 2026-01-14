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
//! - **no_std compatible**: Works with just `alloc`, `std` is optional
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
//! assert_eq!(err.trace_len(), 2);
//! assert!(err.message().unwrap().contains("fetching user"));
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

/// Context data attached to a trace segment.
///
/// Can be either a simple string message or arbitrary typed data.
pub enum Context {
    /// A text message describing what operation was being performed.
    Text(String),
    /// Arbitrary typed context data (boxed to allow any type).
    Any(Box<dyn core::any::Any + Send + Sync>),
}

impl Context {
    /// Get as text, if this is a Text variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Context::Text(s) => Some(s),
            Context::Any(_) => None,
        }
    }

    /// Try to downcast to a specific type, if this is an Any variant.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        match self {
            Context::Text(_) => None,
            Context::Any(b) => b.downcast_ref(),
        }
    }
}

impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Context::Text(s) => write!(f, "Text({:?})", s),
            Context::Any(_) => write!(f, "Any(...)"),
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

    fn with_capacity(cap: usize) -> Self {
        Self {
            head: None,
            recent: Vec::with_capacity(cap),
        }
    }

    #[inline]
    fn push(&mut self, loc: &'static Location<'static>) {
        self.recent.push(loc);
    }

    /// Push a location with context, creating a new segment.
    fn push_with_context(&mut self, loc: &'static Location<'static>, context: Context) {
        // Take the recent locations and create a new segment
        let locations_after = core::mem::take(&mut self.recent);
        let prev = self.head.take();
        self.head = Some(Box::new(Segment {
            location: loc,
            context: Some(context),
            locations_after,
            prev,
        }));
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
            Some(trace) => trace.push(loc),
            None => {
                let mut trace = Trace::with_capacity(4);
                trace.push(loc);
                self.trace = Some(Box::new(trace));
            }
        }
        self
    }

    /// Add the caller's location and a context message to the trace.
    ///
    /// Each context message creates a new segment in the trace, preserving all
    /// previous context. Use this to add human-readable context about what
    /// operation was being performed.
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
    pub fn at_msg(mut self, msg: impl Into<String>) -> Self {
        let loc = Location::caller();
        match &mut self.trace {
            Some(trace) => {
                trace.push_with_context(loc, Context::Text(msg.into()));
            }
            None => {
                let mut trace = Trace::new();
                trace.push_with_context(loc, Context::Text(msg.into()));
                self.trace = Some(Box::new(trace));
            }
        }
        self
    }

    /// Add the caller's location and arbitrary typed context to the trace.
    ///
    /// This allows attaching any `Send + Sync + 'static` data to the error trace.
    /// Use `contexts()` to retrieve context entries and `downcast_ref` to access them.
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
    pub fn at_context<T: Send + Sync + 'static>(mut self, ctx: T) -> Self {
        let loc = Location::caller();
        match &mut self.trace {
            Some(trace) => {
                trace.push_with_context(loc, Context::Any(Box::new(ctx)));
            }
            None => {
                let mut trace = Trace::new();
                trace.push_with_context(loc, Context::Any(Box::new(ctx)));
                self.trace = Some(Box::new(trace));
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
    pub fn with_message(self, msg: impl Into<String>) -> Self {
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
                    Context::Any(_) => writeln!(f, "  context: <typed data>")?,
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
/// assert_eq!(err.trace_len(), 1);
/// ```
pub trait Traceable: Sized {
    /// Wrap this value in a `Traced` and add the caller's location.
    #[track_caller]
    fn traced(self) -> Traced<Self>;

    /// Wrap this value in a `Traced` and add the caller's location with a message.
    #[track_caller]
    fn traced_msg(self, msg: impl Into<String>) -> Traced<Self>;
}

impl<E> Traceable for E {
    #[track_caller]
    #[inline]
    fn traced(self) -> Traced<Self> {
        Traced::new(self).at()
    }

    #[track_caller]
    #[inline]
    fn traced_msg(self, msg: impl Into<String>) -> Traced<Self> {
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
    /// This is a no-op on the `Ok` path.
    #[track_caller]
    fn at(self) -> Result<T, Traced<E>>;

    /// Add the caller's location and a context message if this is `Err`.
    ///
    /// This is a no-op on the `Ok` path.
    #[track_caller]
    fn at_msg(self, msg: impl Into<String>) -> Result<T, Traced<E>>;
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
    fn at_msg(self, msg: impl Into<String>) -> Result<T, Traced<E>> {
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
    #[track_caller]
    fn trace(self) -> Result<T, Traced<E>>;

    /// Convert the error to a `Traced<E>` and add the caller's location with a message.
    #[track_caller]
    fn trace_msg(self, msg: impl Into<String>) -> Result<T, Traced<E>>;
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
    fn trace_msg(self, msg: impl Into<String>) -> Result<T, Traced<E>> {
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

        let any_ctx = Context::Any(Box::new(42u32));
        assert_eq!(any_ctx.as_text(), None);
        assert_eq!(any_ctx.downcast_ref::<u32>(), Some(&42));
    }
}
