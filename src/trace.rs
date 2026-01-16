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
#[cfg(feature = "tinyvec-256-bytes")]
type LocationVec = tinyvec::TinyVec<[LocationElem; 28]>;

/// Heap-allocated location storage (default, no tinyvec feature).
#[cfg(not(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
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

/// Default capacity hint for new traces.
///
/// Most error traces are 3-6 levels deep (e.g., handler → service → repo → db).
/// Pre-allocating 6 slots avoids reallocations for typical call stacks.
/// For deeper traces, the Vec will grow automatically.
///
/// Note: This is ignored when tinyvec features are enabled (TinyVec starts inline).
pub(crate) const DEFAULT_TRACE_CAPACITY: usize = 6;

/// Try to allocate a Box. Returns Some on success.
/// Note: Box::try_new is not yet stable, so this can panic on OOM.
/// The error E is stored inline, so even if tracing fails, the error propagates.
#[inline]
pub(crate) fn try_box<T>(value: T) -> Option<Box<T>> {
    // TODO: Use Box::try_new when stabilized
    Some(Box::new(value))
}

/// Try to push a location onto a LocationVec, returning false on allocation failure.
/// For Vec: uses try_reserve. For TinyVec: spills to heap if needed.
#[cfg(not(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
)))]
#[inline]
fn try_push_location(vec: &mut LocationVec, elem: LocationElem) -> bool {
    if vec.try_reserve(1).is_err() {
        return false;
    }
    vec.push(elem);
    true
}

/// Try to push a location onto a LocationVec (TinyVec version).
/// TinyVec spills to heap if inline capacity exceeded.
#[cfg(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
))]
#[inline]
fn try_push_location(vec: &mut LocationVec, elem: LocationElem) -> bool {
    vec.push(elem);
    true
}

/// Try to create a LocationVec with the given capacity hint, returning None on failure.
/// For Vec: allocates capacity.
#[cfg(not(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
)))]
#[inline]
fn try_location_vec_with_capacity(capacity: usize) -> Option<LocationVec> {
    let mut vec = LocationVec::new();
    if vec.try_reserve(capacity).is_err() {
        return None;
    }
    Some(vec)
}

/// Try to create a LocationVec. For TinyVec, always succeeds (starts on stack).
#[cfg(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
))]
#[inline]
fn try_location_vec_with_capacity(_capacity: usize) -> Option<LocationVec> {
    Some(LocationVec::new())
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
///     fn trace_mut(&mut self) -> &mut AtTrace {
///         &mut self.trace
///     }
/// }
///
/// impl MyError {
///     #[track_caller]
///     fn new(kind: &'static str) -> Self {
///         Self {
///             kind,
///             trace: AtTrace::capture(),
///         }
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

    /// Try to create a AtTrace with pre-allocated capacity.
    /// Returns None if allocation fails (Vec) or always succeeds (TinyVec).
    pub(crate) fn try_with_capacity(cap: usize) -> Option<Self> {
        Some(Self {
            locations: try_location_vec_with_capacity(cap)?,
            crate_info: None,
            contexts: context_vec_new(),
        })
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

    /// Try to push a location with context.
    ///
    /// If the last location has the same file:line as `loc`, just adds context
    /// to that location instead of pushing a new one. This allows chaining
    /// multiple context methods on the same line without duplicating frames.
    ///
    /// On allocation failure, the location/context may be lost but existing data is preserved.
    pub(crate) fn try_push_with_context(
        &mut self,
        loc: &'static Location<'static>,
        context: AtContext,
    ) {
        // Check if last location matches current file:line - if so, just add context
        let idx = if let Some(Some(last)) = self.locations.last() {
            if last.file() == loc.file() && last.line() == loc.line() {
                // Same location - reuse index
                (self.locations.len() - 1).min(u16::MAX as usize) as u16
            } else {
                // Different location - push new
                if !try_push_location(&mut self.locations, Some(loc)) {
                    return;
                }
                (self.locations.len() - 1).min(u16::MAX as usize) as u16
            }
        } else {
            // Empty or last was skipped - push new location
            if !try_push_location(&mut self.locations, Some(loc)) {
                return;
            }
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

    /// Iterate over all context entries, newest first.
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
}

impl Default for AtTrace {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// AtTraceable Trait - for embedding traces in custom error types
// ============================================================================

/// Trait for types that embed an [`AtTrace`] directly.
///
/// Implement this trait to get all the `.at_*()` methods on your custom error types.
/// Only one method is required: [`trace_mut()`](Self::trace_mut).
///
/// ## Example: Inline trace
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
///     fn trace_mut(&mut self) -> &mut AtTrace {
///         &mut self.trace
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
/// let err = MyError::new("not_found")
///     .at_str("looking up user");
/// ```
///
/// ## Example: Boxed trace (smaller error type)
///
/// ```rust
/// use errat::{AtTrace, AtTraceable};
///
/// struct MyError {
///     kind: &'static str,
///     trace: Box<AtTrace>,  // 8 bytes instead of sizeof(AtTrace)
/// }
///
/// impl AtTraceable for MyError {
///     fn trace_mut(&mut self) -> &mut AtTrace {
///         &mut self.trace
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

    /// Add the caller's location to the trace.
    #[track_caller]
    #[inline]
    fn at(mut self) -> Self {
        let _ = self.trace_mut().try_push(Location::caller());
        self
    }

    /// Add the caller's location and a static string context.
    #[track_caller]
    #[inline]
    fn at_str(mut self, msg: &'static str) -> Self {
        let context = AtContext::Text(Cow::Borrowed(msg));
        self.trace_mut()
            .try_push_with_context(Location::caller(), context);
        self
    }

    /// Add the caller's location and a lazily-computed string context.
    #[track_caller]
    #[inline]
    fn at_string(mut self, f: impl FnOnce() -> String) -> Self {
        let context = AtContext::Text(Cow::Owned(f()));
        self.trace_mut()
            .try_push_with_context(Location::caller(), context);
        self
    }

    /// Add the caller's location and lazily-computed typed context (Display formatted).
    #[track_caller]
    #[inline]
    fn at_data<T: fmt::Display + Send + Sync + 'static>(mut self, f: impl FnOnce() -> T) -> Self {
        let ctx = f();
        let Some(boxed_ctx) = try_box(ctx) else {
            return self;
        };
        let context = AtContext::Display(boxed_ctx);
        self.trace_mut()
            .try_push_with_context(Location::caller(), context);
        self
    }

    /// Add the caller's location and lazily-computed typed context (Debug formatted).
    #[track_caller]
    #[inline]
    fn at_debug<T: fmt::Debug + Send + Sync + 'static>(mut self, f: impl FnOnce() -> T) -> Self {
        let ctx = f();
        let Some(boxed_ctx) = try_box(ctx) else {
            return self;
        };
        let context = AtContext::Debug(boxed_ctx);
        self.trace_mut()
            .try_push_with_context(Location::caller(), context);
        self
    }

    /// Add a crate boundary marker.
    #[track_caller]
    #[inline]
    fn at_crate(mut self, info: &'static AtCrateInfo) -> Self {
        let context = AtContext::Crate(info);
        self.trace_mut()
            .try_push_with_context(Location::caller(), context);
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
}
