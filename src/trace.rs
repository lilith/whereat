//! Trace storage and trait for location tracking.
//!
//! This module provides [`AtTrace`] for storing location traces and
//! [`AtTraceable`] for types that embed their own trace.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::panic::Location;

use crate::AtCrateInfo;
use crate::context::{AtContext, AtContextRef};

/// Context entry: (location_index, context).
type ContextEntry = (u16, AtContext);

// ============================================================================
// LocationVec - configurable storage for trace locations
// ============================================================================
//
// Locations are stored as Option<&'static Location> where:
// - Some(loc) = a real captured location
// - None = skipped frame marker (displayed as [...])
//
// This eliminates the need for AtContext::Skipped and saves context allocations.
// Option<&T> has the same size as &T due to null pointer optimization.
//
// When tinyvec features are enabled, we use TinyVec which starts with inline
// storage and spills to heap when capacity is exceeded.

/// Location element type. None = skipped frame marker.
type LocationElem = Option<&'static Location<'static>>;

/// Stack-first location storage with 4 inline slots (tinyvec-64-bytes: sizeof(AtTrace) ≤ 64).
#[cfg(all(
    feature = "tinyvec-64-bytes",
    not(any(feature = "tinyvec-128-bytes", feature = "tinyvec-256-bytes"))
))]
type LocationVec = tinyvec::TinyVec<[LocationElem; 4]>;

/// Stack-first location storage with 12 inline slots (tinyvec-128-bytes: sizeof(AtTrace) ≤ 128).
#[cfg(all(feature = "tinyvec-128-bytes", not(feature = "tinyvec-256-bytes")))]
type LocationVec = tinyvec::TinyVec<[LocationElem; 12]>;

/// Stack-first location storage with 28 inline slots (tinyvec-256-bytes: sizeof(AtTrace) ≤ 256).
#[cfg(all(feature = "tinyvec-256-bytes", not(feature = "tinyvec-512-bytes")))]
type LocationVec = tinyvec::TinyVec<[LocationElem; 28]>;

/// Stack-first location storage with 60 inline slots (tinyvec-512-bytes: sizeof(AtTrace) ≤ 512).
#[cfg(all(feature = "tinyvec-512-bytes", not(feature = "smallvec-128-bytes")))]
type LocationVec = tinyvec::TinyVec<[LocationElem; 60]>;

/// Stack-first location storage with 12 inline slots using smallvec.
#[cfg(all(feature = "smallvec-128-bytes", not(feature = "smallvec-256-bytes")))]
type LocationVec = smallvec::SmallVec<[LocationElem; 12]>;

/// Stack-first location storage with 28 inline slots using smallvec.
#[cfg(feature = "smallvec-256-bytes")]
type LocationVec = smallvec::SmallVec<[LocationElem; 28]>;

/// Heap-allocated location storage (default, no tinyvec/smallvec feature).
#[cfg(not(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes",
    feature = "tinyvec-512-bytes",
    feature = "smallvec-128-bytes",
    feature = "smallvec-256-bytes"
)))]
type LocationVec = Vec<LocationElem>;

// ============================================================================
// ContextVec - lazily-allocated context storage
// ============================================================================
//
// Context storage is typically empty (most traces have no context).
// Using Option<Box<Vec>> saves 16 bytes vs Vec in the common case (8 vs 24).

/// Lazily-allocated context storage. Most traces have no context.
type ContextVec = Option<Box<Vec<ContextEntry>>>;

// ============================================================================
// Fallible Allocation Helpers
// ============================================================================
//
// Uses stable try_reserve APIs where available. Box::try_new is not yet stable,
// so Box allocations use regular Box::new which can panic on OOM.
// In practice, OOM panics are rare and the error itself still propagates
// (since E is stored inline in At<E>).

/// Try to allocate a Box. Returns Some on success.
/// Note: Box::try_new is not yet stable, so this can panic on OOM.
/// The error E is stored inline, so even if tracing fails, the error propagates.
#[inline]
pub(crate) fn try_box<T>(value: T) -> Option<Box<T>> {
    // TODO: Use Box::try_new when stabilized
    Some(Box::new(value))
}

/// Try to push a location onto a LocationVec, returning false on allocation failure.
/// For Vec: uses try_reserve. For TinyVec/SmallVec: spills to heap if needed.
#[cfg(not(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes",
    feature = "tinyvec-512-bytes",
    feature = "smallvec-128-bytes",
    feature = "smallvec-256-bytes"
)))]
#[inline]
fn try_push_location(vec: &mut LocationVec, elem: LocationElem) -> bool {
    if vec.try_reserve(1).is_err() {
        return false;
    }
    vec.push(elem);
    true
}

/// Try to push a location onto a LocationVec (TinyVec/SmallVec version).
/// Spills to heap if inline capacity exceeded.
#[cfg(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes",
    feature = "tinyvec-512-bytes",
    feature = "smallvec-128-bytes",
    feature = "smallvec-256-bytes"
))]
#[inline]
fn try_push_location(vec: &mut LocationVec, elem: LocationElem) -> bool {
    vec.push(elem);
    true
}

// ============================================================================
// ContextVec Helpers
// ============================================================================

/// Create a new empty ContextVec.
#[inline]
fn context_vec_new() -> ContextVec {
    None
}

/// Try to push a context entry (lazily allocates on first push).
#[inline]
fn try_push_context(vec: &mut ContextVec, entry: ContextEntry) -> bool {
    let inner = vec.get_or_insert_with(|| Box::new(Vec::new()));
    if inner.try_reserve(1).is_err() {
        return false;
    }
    inner.push(entry);
    true
}

/// Iterate over contexts.
#[inline]
fn context_iter(vec: &ContextVec) -> impl DoubleEndedIterator<Item = &ContextEntry> {
    vec.iter().flat_map(|v| v.iter())
}

// ============================================================================
// AtTrace - Trace storage for location and context tracking
// ============================================================================

/// Trace storage for location and context tracking.
///
/// Use this type directly when embedding traces in custom error types.
/// For the common case, use `At<E>` which wraps your error with a boxed trace.
///
/// ## Example: Embedding in custom error
///
/// ```rust
/// use errat::{AtTrace, AtTraceable};
///
/// struct MyError {
///     kind: &'static str,
///     trace: AtTrace,
/// }
///
/// impl AtTraceable for MyError {
///     fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
///     fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
///     fn fmt_message(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "{}", self.kind)
///     }
/// }
///
/// impl MyError {
///     #[track_caller]
///     fn new(kind: &'static str) -> Self {
///         Self { kind, trace: AtTrace::capture() }
///     }
/// }
///
/// // Now MyError has all the .at_*() methods from AtTraceable
/// let err = MyError::new("not_found").at_str("looking up user");
/// ```
#[derive(Debug)]
pub struct AtTrace {
    /// All locations in order (oldest first).
    locations: LocationVec,
    /// Crate info for generating repository links (stored once, not per-location).
    /// Set by `at!()` macro or `set_crate_info()` method.
    crate_info: Option<&'static AtCrateInfo>,
    /// AtContext associations: (location_index, context).
    /// Index saturates at u16::MAX; out-of-bounds associations are silently ignored.
    contexts: ContextVec,
}

impl AtTrace {
    /// Create an empty trace.
    ///
    /// Use [`capture()`](Self::capture) to create a trace with the caller's location.
    #[inline]
    pub fn new() -> Self {
        Self {
            locations: LocationVec::new(),
            crate_info: None,
            contexts: context_vec_new(),
        }
    }

    /// Create a trace with the caller's location captured.
    ///
    /// This is the recommended way to start a trace in error constructors.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::AtTrace;
    ///
    /// struct MyError {
    ///     trace: AtTrace,
    /// }
    ///
    /// impl MyError {
    ///     #[track_caller]
    ///     fn new() -> Self {
    ///         Self { trace: AtTrace::capture() }
    ///     }
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn capture() -> Self {
        let mut trace = Self::new();
        let _ = trace.try_push(Location::caller());
        trace
    }

    /// Set the crate info for this trace.
    ///
    /// This is used by `at!()` to provide repository metadata for GitHub links.
    /// Only one crate info can be set per trace - subsequent calls overwrite.
    #[inline]
    pub fn set_crate_info(&mut self, info: &'static AtCrateInfo) {
        self.crate_info = Some(info);
    }

    /// Get the crate info for this trace, if set.
    #[inline]
    pub fn crate_info(&self) -> Option<&'static AtCrateInfo> {
        self.crate_info
    }

    /// Try to push a location. Returns false if allocation fails.
    #[inline]
    pub(crate) fn try_push(&mut self, loc: &'static Location<'static>) -> bool {
        try_push_location(&mut self.locations, Some(loc))
    }

    /// Try to push a skipped frame marker. Returns false if allocation fails.
    #[inline]
    pub(crate) fn try_push_skipped(&mut self) -> bool {
        try_push_location(&mut self.locations, None)
    }

    /// Add context to the last location, or push a new location if trace is empty.
    ///
    /// This allows `at_str()` etc. to add context without creating duplicate frames.
    /// Use `try_push()` first if you need a new location, then call this for context.
    ///
    /// On allocation failure, the context may be lost but existing data is preserved.
    pub(crate) fn try_add_context(&mut self, loc: &'static Location<'static>, context: AtContext) {
        // If empty, push a location first
        let idx = if self.locations.is_empty() {
            if !try_push_location(&mut self.locations, Some(loc)) {
                return;
            }
            0u16
        } else {
            (self.locations.len() - 1).min(u16::MAX as usize) as u16
        };
        // Try to push context; silently fail on OOM
        let _ = try_push_context(&mut self.contexts, (idx, context));
    }

    /// Get the number of entries in the trace (locations + skipped markers).
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.locations.len()
    }

    /// Iterate over all location entries, oldest first.
    /// Returns Option where None = skipped frame marker.
    pub(crate) fn iter(&self) -> impl Iterator<Item = Option<&'static Location<'static>>> + '_ {
        self.locations.iter().copied()
    }

    /// Iterate over all context entries, newest first (loses location association).
    /// Prefer `frames()` for unified iteration.
    pub(crate) fn contexts(&self) -> impl Iterator<Item = AtContextRef<'_>> {
        context_iter(&self.contexts)
            .rev()
            .map(|(_, ctx)| AtContextRef { inner: ctx })
    }

    /// Get all contexts at a specific location index.
    pub(crate) fn contexts_at(&self, idx: usize) -> impl Iterator<Item = &AtContext> {
        context_iter(&self.contexts)
            .filter(move |(i, _)| *i as usize == idx)
            .map(|(_, ctx)| ctx)
    }

    /// Iterate over frames (location + contexts pairs), oldest first.
    ///
    /// This is the recommended way to traverse a trace. Each frame contains
    /// a location (or None for skipped-frames marker) and its associated contexts.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::at;
    ///
    /// #[derive(Debug)]
    /// struct MyError;
    ///
    /// let err = at(MyError)
    ///     .at_str("loading config")
    ///     .at();
    ///
    /// for frame in err.frames() {
    ///     if let Some(loc) = frame.location() {
    ///         println!("at {}:{}", loc.file(), loc.line());
    ///     } else {
    ///         println!("[...]");
    ///     }
    ///     for ctx in frame.contexts() {
    ///         println!("  - {}", ctx);
    ///     }
    /// }
    /// ```
    pub fn frames(&self) -> impl Iterator<Item = AtFrame<'_>> {
        self.locations.iter().enumerate().map(|(idx, loc)| AtFrame {
            location: *loc,
            trace: self,
            index: idx,
        })
    }

    /// Get the number of frames in the trace.
    #[inline]
    pub fn frame_count(&self) -> usize {
        self.locations.len()
    }

    /// Check if the trace is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.locations.is_empty()
    }

    /// Take the entire trace, leaving self empty.
    ///
    /// Preserves crate_info in self (not transferred).
    pub fn take(&mut self) -> AtTrace {
        AtTrace {
            locations: core::mem::take(&mut self.locations),
            crate_info: self.crate_info, // Copy, don't move
            contexts: core::mem::take(&mut self.contexts),
        }
    }

    /// Pop the most recent location and its contexts from the end.
    ///
    /// Returns `None` if the trace is empty.
    pub fn pop(&mut self) -> Option<AtTraceSegment> {
        if self.locations.is_empty() {
            return None;
        }

        let last_idx = (self.locations.len() - 1) as u16;
        let location = self.locations.pop()?;

        // Collect contexts for this location (they're stored newest-first in usage,
        // but we need to extract those with matching index)
        let mut contexts = Vec::new();
        if let Some(ref mut ctx_vec) = self.contexts {
            // Remove contexts with matching index from the end
            while let Some(&(idx, _)) = ctx_vec.last() {
                if idx == last_idx {
                    contexts.push(ctx_vec.pop().unwrap().1);
                } else {
                    break;
                }
            }
        }
        contexts.reverse(); // Restore original order

        Some(AtTraceSegment { location, contexts })
    }

    /// Push a segment (location + contexts) to the end of the trace.
    pub fn push(&mut self, segment: AtTraceSegment) {
        let idx = self.locations.len() as u16;

        // Try to push location
        if !try_push_location(&mut self.locations, segment.location) {
            return;
        }

        // Push contexts
        for ctx in segment.contexts {
            let _ = try_push_context(&mut self.contexts, (idx, ctx));
        }
    }

    /// Pop the oldest location and its contexts from the beginning.
    ///
    /// Returns `None` if the trace is empty.
    ///
    /// Note: This is O(n) as it shifts all remaining elements.
    pub fn pop_first(&mut self) -> Option<AtTraceSegment> {
        if self.locations.is_empty() {
            return None;
        }

        let location = self.locations.remove(0);

        // Collect and remove contexts for index 0, decrement remaining indices
        let mut contexts = Vec::new();
        if let Some(ref mut ctx_vec) = self.contexts {
            let mut i = 0;
            while i < ctx_vec.len() {
                if ctx_vec[i].0 == 0 {
                    contexts.push(ctx_vec.remove(i).1);
                } else {
                    // Decrement index for remaining contexts
                    ctx_vec[i].0 -= 1;
                    i += 1;
                }
            }
        }

        Some(AtTraceSegment { location, contexts })
    }

    /// Insert a segment (location + contexts) at the beginning of the trace.
    ///
    /// Note: This is O(n) as it shifts all existing elements.
    pub fn push_first(&mut self, segment: AtTraceSegment) {
        // Shift all existing indices up by 1
        if let Some(ref mut ctx_vec) = self.contexts {
            for (idx, _) in ctx_vec.iter_mut() {
                *idx = idx.saturating_add(1);
            }
        }

        // Insert location at beginning
        self.locations.insert(0, segment.location);

        // Insert contexts at beginning with index 0
        if !segment.contexts.is_empty() {
            let ctx_vec = self.contexts.get_or_insert_with(|| Box::new(Vec::new()));
            for (i, ctx) in segment.contexts.into_iter().enumerate() {
                ctx_vec.insert(i, (0, ctx));
            }
        }
    }

    /// Append all segments from another trace to the end of this trace.
    ///
    /// The source trace is consumed.
    pub fn append(&mut self, mut other: AtTrace) {
        while let Some(seg) = other.pop_first() {
            self.push(seg);
        }
    }

    /// Prepend all segments from another trace to the beginning of this trace.
    ///
    /// The source trace is consumed.
    pub fn prepend(&mut self, mut other: AtTrace) {
        // Pop from other's end and insert at our beginning (reverse order)
        let mut segments = Vec::new();
        while let Some(seg) = other.pop() {
            segments.push(seg);
        }
        // Insert in reverse order to maintain original order
        for seg in segments {
            self.push_first(seg);
        }
    }
}

impl Default for AtTrace {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// AtTraceSegment - A single location with its contexts
// ============================================================================

/// A segment of a trace: one location with its associated contexts.
///
/// Used for transferring trace segments between `At<E>` and `AtTraceable` types.
///
/// ## Example: Transferring trace segments
///
/// ```rust
/// use errat::{at, At, AtTrace};
///
/// #[derive(Debug)]
/// struct Error1;
/// #[derive(Debug)]
/// struct Error2;
///
/// let mut err1: At<Error1> = at(Error1).at_str("context");
/// let mut err2: At<Error2> = at(Error2);
///
/// // Transfer most recent segment from err1 to err2
/// if let Some(seg) = err1.at_pop() {
///     err2.at_push(seg);
/// }
/// ```
#[derive(Debug)]
pub struct AtTraceSegment {
    location: Option<&'static Location<'static>>,
    contexts: Vec<AtContext>,
}

impl AtTraceSegment {
    /// Create a new segment with a location and no contexts.
    pub fn new(location: Option<&'static Location<'static>>) -> Self {
        Self {
            location,
            contexts: Vec::new(),
        }
    }

    /// Create a new segment capturing the caller's location.
    #[track_caller]
    pub fn capture() -> Self {
        Self::new(Some(Location::caller()))
    }

    /// Get the location (None means skipped frames marker).
    pub fn location(&self) -> Option<&'static Location<'static>> {
        self.location
    }

    /// Check if this is a skipped frames marker.
    pub fn is_skipped(&self) -> bool {
        self.location.is_none()
    }

    /// Iterate over contexts in this segment.
    pub fn contexts(&self) -> impl Iterator<Item = AtContextRef<'_>> {
        self.contexts.iter().map(|c| AtContextRef { inner: c })
    }

    /// Number of contexts in this segment.
    pub fn context_count(&self) -> usize {
        self.contexts.len()
    }

    /// Add a static string context.
    pub fn with_str(mut self, msg: &'static str) -> Self {
        self.contexts.push(AtContext::Text(Cow::Borrowed(msg)));
        self
    }

    /// Add a dynamic string context.
    pub fn with_string(mut self, msg: String) -> Self {
        self.contexts.push(AtContext::Text(Cow::Owned(msg)));
        self
    }

    /// Add typed context (Display).
    pub fn with_data<T: fmt::Display + Send + Sync + 'static>(mut self, data: T) -> Self {
        if let Some(boxed) = try_box(data) {
            self.contexts.push(AtContext::Display(boxed));
        }
        self
    }

    /// Add typed context (Debug).
    pub fn with_debug<T: fmt::Debug + Send + Sync + 'static>(mut self, data: T) -> Self {
        if let Some(boxed) = try_box(data) {
            self.contexts.push(AtContext::Debug(boxed));
        }
        self
    }

    /// Add crate boundary marker.
    pub fn with_crate(mut self, info: &'static AtCrateInfo) -> Self {
        self.contexts.push(AtContext::Crate(info));
        self
    }

    /// Add an error as context.
    pub fn with_error<E: core::error::Error + Send + Sync + 'static>(mut self, err: E) -> Self {
        if let Some(boxed) = try_box(err) {
            self.contexts.push(AtContext::Error(boxed));
        }
        self
    }

    /// Consume and return the raw contexts (internal use).
    #[allow(dead_code)]
    pub(crate) fn into_contexts(self) -> Vec<AtContext> {
        self.contexts
    }
}

// ============================================================================
// AtFrame - A single frame in a trace (for iteration)
// ============================================================================

/// A single frame in a trace: location with its associated contexts.
///
/// Returned by [`AtTrace::frames()`] and [`At::frames()`](crate::At::frames).
/// Unlike [`AtTraceSegment`] which owns its data, this is a view into the trace.
///
/// ## Example
///
/// ```rust
/// use errat::at;
///
/// #[derive(Debug)]
/// struct MyError;
///
/// let err = at(MyError).at_str("loading");
///
/// for frame in err.frames() {
///     if let Some(loc) = frame.location() {
///         println!("{}:{}", loc.file(), loc.line());
///     }
///     for ctx in frame.contexts() {
///         if let Some(text) = ctx.as_text() {
///             println!("  context: {}", text);
///         }
///     }
/// }
/// ```
#[derive(Clone, Copy)]
pub struct AtFrame<'a> {
    location: Option<&'static Location<'static>>,
    trace: &'a AtTrace,
    index: usize,
}

impl<'a> AtFrame<'a> {
    /// Get the source location, or None if this is a skipped-frames marker.
    #[inline]
    pub fn location(&self) -> Option<&'static Location<'static>> {
        self.location
    }

    /// Check if this frame is a skipped-frames marker (`[...]`).
    #[inline]
    pub fn is_skipped(&self) -> bool {
        self.location.is_none()
    }

    /// Iterate over contexts attached to this frame.
    pub fn contexts(&self) -> impl Iterator<Item = AtContextRef<'a>> {
        let idx = self.index;
        context_iter(&self.trace.contexts)
            .filter(move |(i, _)| *i as usize == idx)
            .map(|(_, ctx)| AtContextRef { inner: ctx })
    }

    /// Check if this frame has any contexts.
    pub fn has_contexts(&self) -> bool {
        let idx = self.index;
        context_iter(&self.trace.contexts).any(|(i, _)| *i as usize == idx)
    }
}

impl fmt::Debug for AtFrame<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.location {
            Some(loc) => {
                write!(f, "at {}:{}", loc.file(), loc.line())?;
                for ctx in self.contexts() {
                    write!(f, " ({:?})", ctx)?;
                }
                Ok(())
            }
            None => write!(f, "[...]"),
        }
    }
}

// ============================================================================
// AtTraceBoxed - Boxed optional trace for small error footprint
// ============================================================================

/// A boxed optional trace for keeping error types small.
///
/// This type is always 8 bytes (one pointer) regardless of trace size.
/// The trace is allocated lazily on first mutation.
///
/// ## Example
///
/// ```rust
/// use errat::{AtTraceBoxed, AtTrace, AtTraceable};
///
/// struct MyError {
///     kind: &'static str,
///     trace: AtTraceBoxed,  // 8 bytes, not 24-256
/// }
///
/// impl AtTraceable for MyError {
///     fn trace_mut(&mut self) -> &mut AtTrace { self.trace.get_or_insert_mut() }
///     fn trace(&self) -> Option<&AtTrace> { self.trace.as_ref() }
///     fn fmt_message(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "{}", self.kind)
///     }
/// }
///
/// impl MyError {
///     fn new(kind: &'static str) -> Self {
///         Self { kind, trace: AtTraceBoxed::new() }
///     }
///
///     #[track_caller]
///     fn with_trace(kind: &'static str) -> Self {
///         Self { kind, trace: AtTraceBoxed::capture() }
///     }
/// }
///
/// // No allocation until .at_*() is called
/// let err = MyError::new("not_found");
/// assert!(err.trace.is_empty());
///
/// // With trace captured immediately
/// let err = MyError::with_trace("not_found");
/// assert!(!err.trace.is_empty());
/// ```
#[derive(Default)]
pub struct AtTraceBoxed(Option<Box<AtTrace>>);

impl AtTraceBoxed {
    /// Create an empty boxed trace (no allocation).
    #[inline]
    pub const fn new() -> Self {
        Self(None)
    }

    /// Create a boxed trace with the caller's location captured.
    #[track_caller]
    #[inline]
    pub fn capture() -> Self {
        Self(Some(Box::new(AtTrace::capture())))
    }

    /// Check if the trace is empty (None or inner is empty).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.as_ref().is_none_or(|t| t.is_empty())
    }

    /// Get immutable reference to the trace, if allocated.
    #[inline]
    pub fn as_ref(&self) -> Option<&AtTrace> {
        self.0.as_deref()
    }

    /// Get mutable reference to the trace, if allocated.
    #[inline]
    pub fn as_mut(&mut self) -> Option<&mut AtTrace> {
        self.0.as_deref_mut()
    }

    /// Get mutable reference, allocating if needed.
    ///
    /// Use this in `AtTraceable::trace_mut()` implementations.
    #[inline]
    pub fn get_or_insert_mut(&mut self) -> &mut AtTrace {
        self.0.get_or_insert_with(|| Box::new(AtTrace::new()))
    }

    /// Take the trace, leaving self empty.
    #[inline]
    pub fn take(&mut self) -> Option<AtTrace> {
        self.0.take().map(|b| *b)
    }

    /// Set the trace from an existing AtTrace.
    #[inline]
    pub fn set(&mut self, trace: AtTrace) {
        if trace.is_empty() {
            self.0 = None;
        } else {
            self.0 = Some(Box::new(trace));
        }
    }

    /// Iterate over frames (location + contexts pairs), oldest first.
    ///
    /// Returns an empty iterator if the trace hasn't been allocated.
    pub fn frames(&self) -> impl Iterator<Item = AtFrame<'_>> {
        self.0.iter().flat_map(|t| t.frames())
    }

    /// Get the number of frames in the trace.
    #[inline]
    pub fn frame_count(&self) -> usize {
        self.0.as_ref().map_or(0, |t| t.frame_count())
    }

    /// Get crate info, if set.
    #[inline]
    pub fn crate_info(&self) -> Option<&'static AtCrateInfo> {
        self.0.as_ref().and_then(|t| t.crate_info())
    }
}

impl fmt::Debug for AtTraceBoxed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(trace) => fmt::Debug::fmt(trace, f),
            None => write!(f, "AtTraceBoxed(empty)"),
        }
    }
}

impl From<AtTrace> for AtTraceBoxed {
    fn from(trace: AtTrace) -> Self {
        if trace.is_empty() {
            Self(None)
        } else {
            Self(Some(Box::new(trace)))
        }
    }
}

impl From<AtTraceBoxed> for Option<AtTrace> {
    fn from(boxed: AtTraceBoxed) -> Self {
        boxed.0.map(|b| *b)
    }
}

// ============================================================================
// AtTraceable Trait - for embedding traces in custom error types
// ============================================================================

/// Trait for types that embed an [`AtTrace`] directly.
///
/// Implement this trait to get all the `.at_*()` methods on your custom error types.
/// Three methods are required:
/// - [`trace_mut()`](Self::trace_mut) - mutable access to trace
/// - [`trace()`](Self::trace) - immutable access to trace
/// - [`fmt_message()`](Self::fmt_message) - format the error message
///
/// ## Example: Inline trace
///
/// ```rust
/// use errat::{AtTrace, AtTraceable};
/// use std::fmt;
///
/// struct MyError {
///     kind: &'static str,
///     trace: AtTrace,
/// }
///
/// impl AtTraceable for MyError {
///     fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
///     fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
///     fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         write!(f, "{}", self.kind)
///     }
/// }
///
/// impl MyError {
///     #[track_caller]
///     fn new(kind: &'static str) -> Self {
///         Self { kind, trace: AtTrace::capture() }
///     }
/// }
///
/// // Now you can chain .at_*() methods
/// let err = MyError::new("not_found").at_str("looking up user");
/// ```
///
/// ## Example: Boxed trace (smaller error type)
///
/// ```rust
/// use errat::{AtTrace, AtTraceable};
/// use std::fmt;
///
/// struct MyError {
///     kind: &'static str,
///     trace: Box<AtTrace>,  // 8 bytes instead of sizeof(AtTrace)
/// }
///
/// impl AtTraceable for MyError {
///     fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
///     fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
///     fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         write!(f, "{}", self.kind)
///     }
/// }
///
/// impl MyError {
///     #[track_caller]
///     fn new(kind: &'static str) -> Self {
///         Self { kind, trace: Box::new(AtTrace::capture()) }
///     }
/// }
/// ```
///
/// ## Example: Optional boxed trace (lazy allocation)
///
/// ```rust
/// use errat::{AtTrace, AtTraceable};
/// use std::fmt;
///
/// struct MyError {
///     kind: &'static str,
///     trace: Option<Box<AtTrace>>,  // None until first .at_*() call
/// }
///
/// impl AtTraceable for MyError {
///     fn trace_mut(&mut self) -> &mut AtTrace {
///         self.trace.get_or_insert_with(|| Box::new(AtTrace::new()))
///     }
///     fn trace(&self) -> Option<&AtTrace> { self.trace.as_deref() }
///     fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         write!(f, "{}", self.kind)
///     }
/// }
///
/// impl MyError {
///     fn new(kind: &'static str) -> Self {  // No #[track_caller] needed
///         Self { kind, trace: None }
///     }
/// }
///
/// // Trace allocated lazily on first .at_*() call
/// let err = MyError::new("not_found").at_str("context");
/// ```
///
/// ## Why use this over `At<E>`?
///
/// Use `AtTraceable` when you want:
/// - Full control over your error type's layout
/// - Custom storage strategy (inline, boxed, or optional)
/// - To define your own error constructors
///
/// Use `At<E>` when you want:
/// - Minimal changes to existing code
/// - To wrap errors from external crates
/// - The simplest possible setup
pub trait AtTraceable: Sized {
    /// Get a mutable reference to the embedded trace.
    fn trace_mut(&mut self) -> &mut AtTrace;

    /// Get an immutable reference to the trace, if allocated.
    ///
    /// Returns `None` if no trace has been allocated yet (for lazy storage patterns).
    /// For inline storage, this always returns `Some`.
    fn trace(&self) -> Option<&AtTrace>;

    /// Format just the error message (without trace).
    ///
    /// This is used by the trace formatters to show the error message
    /// separately from the trace. Typically delegates to your error kind's Display.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{AtTrace, AtTraceable};
    /// use std::fmt;
    ///
    /// #[derive(Debug)]
    /// enum ErrorKind {
    ///     NotFound,
    ///     InvalidInput(String),
    /// }
    ///
    /// struct MyError {
    ///     kind: ErrorKind,
    ///     trace: AtTrace,
    /// }
    ///
    /// impl AtTraceable for MyError {
    ///     fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
    ///     fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
    ///
    ///     fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    ///         match &self.kind {
    ///             ErrorKind::NotFound => write!(f, "not found"),
    ///             ErrorKind::InvalidInput(s) => write!(f, "invalid input: {}", s),
    ///         }
    ///     }
    /// }
    /// ```
    fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;

    /// Add the caller's location to the trace.
    #[track_caller]
    #[inline]
    fn at(mut self) -> Self {
        let _ = self.trace_mut().try_push(Location::caller());
        self
    }

    /// Add a static string context to the last location (or create one if empty).
    #[track_caller]
    #[inline]
    fn at_str(mut self, msg: &'static str) -> Self {
        let context = AtContext::Text(Cow::Borrowed(msg));
        self.trace_mut()
            .try_add_context(Location::caller(), context);
        self
    }

    /// Add a lazily-computed string context to the last location (or create one if empty).
    #[track_caller]
    #[inline]
    fn at_string(mut self, f: impl FnOnce() -> String) -> Self {
        let context = AtContext::Text(Cow::Owned(f()));
        self.trace_mut()
            .try_add_context(Location::caller(), context);
        self
    }

    /// Add lazily-computed typed context (Display) to the last location (or create one if empty).
    #[track_caller]
    #[inline]
    fn at_data<T: fmt::Display + Send + Sync + 'static>(mut self, f: impl FnOnce() -> T) -> Self {
        let ctx = f();
        let Some(boxed_ctx) = try_box(ctx) else {
            return self;
        };
        let context = AtContext::Display(boxed_ctx);
        self.trace_mut()
            .try_add_context(Location::caller(), context);
        self
    }

    /// Add lazily-computed typed context (Debug) to the last location (or create one if empty).
    #[track_caller]
    #[inline]
    fn at_debug<T: fmt::Debug + Send + Sync + 'static>(mut self, f: impl FnOnce() -> T) -> Self {
        let ctx = f();
        let Some(boxed_ctx) = try_box(ctx) else {
            return self;
        };
        let context = AtContext::Debug(boxed_ctx);
        self.trace_mut()
            .try_add_context(Location::caller(), context);
        self
    }

    /// Add an error as context to the last location (or create one if empty).
    ///
    /// Use this to attach a source error that implements `core::error::Error`.
    #[track_caller]
    #[inline]
    fn at_error<E: core::error::Error + Send + Sync + 'static>(mut self, err: E) -> Self {
        let Some(boxed_err) = try_box(err) else {
            return self;
        };
        let context = AtContext::Error(boxed_err);
        self.trace_mut()
            .try_add_context(Location::caller(), context);
        self
    }

    /// Add a crate boundary marker to the last location (or create one if empty).
    #[track_caller]
    #[inline]
    fn at_crate(mut self, info: &'static AtCrateInfo) -> Self {
        let context = AtContext::Crate(info);
        self.trace_mut()
            .try_add_context(Location::caller(), context);
        self
    }

    /// Add a skip marker to indicate skipped frames.
    /// Displayed as `[...]` in trace output.
    #[inline]
    fn at_skipped_frames(mut self) -> Self {
        // None in locations vec = skipped frame marker
        let _ = self.trace_mut().try_push_skipped();
        self
    }

    /// Add a location frame with the caller's function name as context.
    ///
    /// Captures both file:line:col AND the function name at zero runtime cost.
    /// Pass an empty closure `|| {}` - its type includes the parent function name.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{AtTrace, AtTraceable};
    ///
    /// struct MyError {
    ///     trace: AtTrace,
    /// }
    ///
    /// impl AtTraceable for MyError {
    ///     fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
    ///     fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
    ///     fn fmt_message(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    ///         write!(f, "my error")
    ///     }
    /// }
    ///
    /// impl MyError {
    ///     #[track_caller]
    ///     fn new() -> Self {
    ///         Self { trace: AtTrace::capture() }
    ///     }
    /// }
    ///
    /// fn do_something() -> Result<(), MyError> {
    ///     Err(MyError::new().at_fn(|| {}))  // Captures file:line + "do_something"
    /// }
    /// ```
    #[track_caller]
    #[inline]
    fn at_fn<F: Fn()>(mut self, _marker: F) -> Self {
        let full_name = core::any::type_name::<F>();
        // Type looks like: "crate::module::function::{{closure}}"
        // Strip "::{{closure}}" suffix if present
        let name = full_name.strip_suffix("::{{closure}}").unwrap_or(full_name);
        let loc = Location::caller();
        let trace = self.trace_mut();
        // First push a new location frame
        let _ = trace.try_push(loc);
        // Then add function name context to that frame
        let context = AtContext::FunctionName(name);
        trace.try_add_context(loc, context);
        self
    }

    /// Add a location frame with an explicit name as context.
    ///
    /// Like [`at_fn`](Self::at_fn) but with an explicit label instead of
    /// auto-detecting the function name.
    #[track_caller]
    #[inline]
    fn at_named(mut self, name: &'static str) -> Self {
        let loc = Location::caller();
        let trace = self.trace_mut();
        let _ = trace.try_push(loc);
        let context = AtContext::FunctionName(name);
        trace.try_add_context(loc, context);
        self
    }

    // ========================================================================
    // Trace manipulation methods
    // ========================================================================

    /// Pop the most recent location and its contexts from the trace.
    #[inline]
    fn at_pop(&mut self) -> Option<AtTraceSegment> {
        self.trace_mut().pop()
    }

    /// Push a segment (location + contexts) to the end of the trace.
    #[inline]
    fn at_push(&mut self, segment: AtTraceSegment) {
        self.trace_mut().push(segment);
    }

    /// Pop the oldest location and its contexts from the trace.
    #[inline]
    fn at_first_pop(&mut self) -> Option<AtTraceSegment> {
        self.trace_mut().pop_first()
    }

    /// Insert a segment (location + contexts) at the beginning of the trace.
    #[inline]
    fn at_first_insert(&mut self, segment: AtTraceSegment) {
        self.trace_mut().push_first(segment);
    }

    // ========================================================================
    // Error conversion methods
    // ========================================================================

    /// Convert to another `AtTraceable` type, transferring the trace.
    ///
    /// The trace is moved from self to the new error.
    fn map_traceable<E2, F>(mut self, f: F) -> E2
    where
        F: FnOnce(Self) -> E2,
        E2: AtTraceable,
    {
        let trace = self.trace_mut().take();
        let mut new_err = f(self);
        *new_err.trace_mut() = trace;
        new_err
    }

    /// Convert to `At<E2>`, transferring the trace.
    fn into_at<E2, F>(mut self, f: F) -> crate::At<E2>
    where
        F: FnOnce(Self) -> E2,
    {
        let trace = self.trace_mut().take();
        let error = f(self);
        crate::At::from_parts(error, trace)
    }

    // ========================================================================
    // Formatting methods
    // ========================================================================

    /// Format with full trace (message + all frames with contexts).
    ///
    /// Returns a formatter that displays:
    /// - The error message (via `fmt_message`)
    /// - All trace frames with locations
    /// - All context strings attached to each frame
    /// - Nested error chains for error contexts
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{AtTrace, AtTraceable};
    /// use std::fmt;
    ///
    /// struct MyError {
    ///     msg: &'static str,
    ///     trace: AtTrace,
    /// }
    ///
    /// impl AtTraceable for MyError {
    ///     fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
    ///     fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
    ///     fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    ///         write!(f, "{}", self.msg)
    ///     }
    /// }
    ///
    /// impl MyError {
    ///     #[track_caller]
    ///     fn new(msg: &'static str) -> Self {
    ///         Self { msg, trace: AtTrace::capture() }
    ///     }
    /// }
    ///
    /// let err = MyError::new("something failed").at_str("while loading");
    /// println!("{}", err.full_trace());
    /// // Output:
    /// // something failed
    /// //     at src/main.rs:10:15
    /// //         while loading
    /// ```
    fn full_trace(&self) -> impl fmt::Display + '_ {
        FullTraceDisplay { error: self }
    }

    /// Format with trace locations only (message + locations, no context strings).
    ///
    /// Returns a formatter that displays:
    /// - The error message (via `fmt_message`)
    /// - All trace frame locations
    /// - NO context strings (for compact output)
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{AtTrace, AtTraceable};
    /// use std::fmt;
    ///
    /// struct MyError {
    ///     msg: &'static str,
    ///     trace: AtTrace,
    /// }
    ///
    /// impl AtTraceable for MyError {
    ///     fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
    ///     fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
    ///     fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    ///         write!(f, "{}", self.msg)
    ///     }
    /// }
    ///
    /// impl MyError {
    ///     #[track_caller]
    ///     fn new(msg: &'static str) -> Self {
    ///         Self { msg, trace: AtTrace::capture() }
    ///     }
    /// }
    ///
    /// let err = MyError::new("something failed").at_str("while loading");
    /// println!("{}", err.last_error_trace());
    /// // Output:
    /// // something failed
    /// //     at src/main.rs:10:15
    /// // (note: "while loading" context is omitted)
    /// ```
    fn last_error_trace(&self) -> impl fmt::Display + '_ {
        LastErrorTraceDisplay { error: self }
    }

    /// Format just the error message (no trace).
    ///
    /// Returns a formatter that only displays the error message via `fmt_message`.
    /// Use this when you want to show the error without any trace information.
    fn last_error(&self) -> impl fmt::Display + '_ {
        LastErrorDisplay { error: self }
    }
}

// ============================================================================
// Trace formatters for AtTraceable
// ============================================================================

/// Formatter that shows error message + full trace with all contexts.
struct FullTraceDisplay<'a, E: AtTraceable> {
    error: &'a E,
}

impl<E: AtTraceable> fmt::Display for FullTraceDisplay<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show the error message
        self.error.fmt_message(f)?;

        // Show trace frames
        if let Some(trace) = self.error.trace() {
            for frame in trace.frames() {
                if let Some(loc) = frame.location() {
                    write!(f, "\n    at {}:{}:{}", loc.file(), loc.line(), loc.column())?;
                } else {
                    write!(f, "\n    [...]")?;
                }

                // Show contexts for this frame
                for ctx in frame.contexts() {
                    if let Some(text) = ctx.as_text() {
                        write!(f, "\n        {}", text)?;
                    } else if let Some(fn_name) = ctx.as_function_name() {
                        write!(f, "\n        in {}", fn_name)?;
                    } else if let Some(err) = ctx.as_error() {
                        write!(f, "\n        caused by: {}", err)?;
                        // Write nested error chain
                        let mut source = err.source();
                        let mut depth = 2;
                        while let Some(src) = source {
                            let indent = "    ".repeat(depth);
                            write!(f, "\n{}caused by: {}", indent, src)?;
                            source = src.source();
                            depth += 1;
                        }
                    } else {
                        write!(f, "\n        {}", ctx)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Formatter that shows error message + trace locations only (no contexts).
struct LastErrorTraceDisplay<'a, E: AtTraceable> {
    error: &'a E,
}

impl<E: AtTraceable> fmt::Display for LastErrorTraceDisplay<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show the error message
        self.error.fmt_message(f)?;

        // Show trace frames (locations only, no contexts)
        if let Some(trace) = self.error.trace() {
            for frame in trace.frames() {
                if let Some(loc) = frame.location() {
                    write!(f, "\n    at {}:{}:{}", loc.file(), loc.line(), loc.column())?;
                } else {
                    write!(f, "\n    [...]")?;
                }
            }
        }
        Ok(())
    }
}

/// Formatter that shows just the error message (no trace).
struct LastErrorDisplay<'a, E: AtTraceable> {
    error: &'a E,
}

impl<E: AtTraceable> fmt::Display for LastErrorDisplay<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt_message(f)
    }
}
