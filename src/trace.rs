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

use crate::context::{AtContext, AtContextRef};
use crate::AtCrateInfo;

// ============================================================================
// LocationVec - configurable storage for trace locations
// ============================================================================
//
// When tinyvec features are enabled, we use TinyVec which starts with inline
// storage and spills to heap when capacity is exceeded. We use Option<&Location>
// as the element type because tinyvec requires Default, and Option<&T> has the
// same size as &T due to null pointer optimization.

/// Stack-first location storage with 3 inline slots (tinyvec-64-bytes: sizeof(AtTrace) = 64).
#[cfg(all(
    feature = "tinyvec-64-bytes",
    not(any(feature = "tinyvec-128-bytes", feature = "tinyvec-256-bytes"))
))]
type LocationVec = tinyvec::TinyVec<[Option<&'static Location<'static>>; 3]>;

/// Stack-first location storage with 11 inline slots (tinyvec-128-bytes: sizeof(AtTrace) = 128).
#[cfg(all(feature = "tinyvec-128-bytes", not(feature = "tinyvec-256-bytes")))]
type LocationVec = tinyvec::TinyVec<[Option<&'static Location<'static>>; 11]>;

/// Stack-first location storage with 27 inline slots (tinyvec-256-bytes: sizeof(AtTrace) = 256).
#[cfg(feature = "tinyvec-256-bytes")]
type LocationVec = tinyvec::TinyVec<[Option<&'static Location<'static>>; 27]>;

/// Heap-allocated location storage (default, no tinyvec feature).
#[cfg(not(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
)))]
type LocationVec = Vec<&'static Location<'static>>;

/// Element type stored in LocationVec (Option-wrapped for tinyvec).
#[cfg(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
))]
type LocationElem = Option<&'static Location<'static>>;

/// Element type stored in LocationVec (direct reference for Vec).
#[cfg(not(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
)))]
type LocationElem = &'static Location<'static>;

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

/// Try to push a location onto a LocationVec, returning false on failure.
/// For Vec: fails on allocation error.
#[cfg(not(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
)))]
#[inline]
fn try_push_location(vec: &mut LocationVec, value: &'static Location<'static>) -> bool {
    if vec.try_reserve(1).is_err() {
        return false;
    }
    vec.push(value);
    true
}

/// Try to push a location onto a LocationVec, returning false on allocation failure.
/// For TinyVec: wraps in Some(), spills to heap if inline capacity exceeded.
#[cfg(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
))]
#[inline]
fn try_push_location(vec: &mut LocationVec, value: &'static Location<'static>) -> bool {
    // TinyVec will spill to heap if needed, so this always succeeds
    // (unless we're truly out of memory, but then we'd panic anyway)
    vec.push(Some(value));
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

/// Get location from LocationVec element reference (identity for Vec, unwrap for TinyVec).
#[cfg(not(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
)))]
#[inline]
fn unwrap_location(loc: &LocationElem) -> &'static Location<'static> {
    loc
}

/// Get location from LocationVec element reference (identity for Vec, unwrap for TinyVec).
#[cfg(any(
    feature = "tinyvec-64-bytes",
    feature = "tinyvec-128-bytes",
    feature = "tinyvec-256-bytes"
))]
#[inline]
fn unwrap_location(loc: &LocationElem) -> &'static Location<'static> {
    // Safe because we only ever push Some values
    loc.expect("LocationVec should only contain Some values")
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
    /// AtContext associations: (location_index, context).
    /// Index saturates at u16::MAX; out-of-bounds associations are silently ignored.
    contexts: Vec<(u16, AtContext)>,
}

impl AtTrace {
    /// Create an empty trace.
    ///
    /// Use [`capture()`](Self::capture) to create a trace with the caller's location.
    #[inline]
    pub fn new() -> Self {
        Self {
            locations: LocationVec::new(),
            contexts: Vec::new(),
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
            contexts: Vec::new(),
        })
    }

    /// Try to push a location. Returns false if allocation fails.
    #[inline]
    pub(crate) fn try_push(&mut self, loc: &'static Location<'static>) -> bool {
        try_push_location(&mut self.locations, loc)
    }

    /// Try to push a location with context.
    /// On allocation failure, the location/context may be lost but existing data is preserved.
    pub(crate) fn try_push_with_context(
        &mut self,
        loc: &'static Location<'static>,
        context: AtContext,
    ) {
        if !try_push_location(&mut self.locations, loc) {
            return; // Location push failed, skip context too
        }
        // Saturate index at u16::MAX
        let idx = (self.locations.len() - 1).min(u16::MAX as usize) as u16;
        // Try to push context; silently fail on OOM
        if self.contexts.try_reserve(1).is_ok() {
            self.contexts.push((idx, context));
        }
    }

    /// Get the number of locations in the trace.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.locations.len()
    }

    /// Iterate over all locations, oldest first.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &'static Location<'static>> + '_ {
        self.locations.iter().map(|elem| unwrap_location(elem))
    }

    /// Iterate over all context entries, newest first.
    pub(crate) fn contexts(&self) -> impl Iterator<Item = AtContextRef<'_>> {
        self.contexts
            .iter()
            .rev()
            .map(|(_, ctx)| AtContextRef { inner: ctx })
    }

    /// Get context at a specific location index, if any.
    pub(crate) fn context_at(&self, idx: usize) -> Option<&AtContext> {
        if idx > u16::MAX as usize {
            return None;
        }
        let idx = idx as u16;
        // Linear search is fine - contexts vec is typically tiny (0-3 entries)
        self.contexts
            .iter()
            .find(|(i, _)| *i == idx)
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
    #[track_caller]
    #[inline]
    fn at_skipped_frames(mut self) -> Self {
        let context = AtContext::Skipped;
        self.trace_mut()
            .try_push_with_context(Location::caller(), context);
        self
    }
}
