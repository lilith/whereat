//! # errat - Lightweight error location tracking
//!
//! A minimal error tracing library that adds location tracking to any error type
//! with small `sizeof` overhead and `no_std` support.
//!
//! ## Design Goals
//!
//! - **Small sizeof**: `At<E>` is only `sizeof(E) + 8` bytes (one pointer for boxed trace)
//! - **Zero allocation on Ok path**: No heap allocation until an error occurs
//! - **Simple API**: `.start_at()` on errors, `.at()` on Results
//! - **Zero-copy static strings**: `.at_str("literal")` is zero-cost
//! - **Lazy evaluation**: `.at_string(|| ...)` and `.at_data(|| ...)` defer computation to error path
//! - **no_std compatible**: Works with just `core` + `alloc`, `std` is optional
//!
//! ## Quick Start
//!
//! ```rust
//! use errat::{at, At, ResultAtExt};
//!
//! #[derive(Debug)]
//! enum MyError {
//!     NotFound,
//!     InvalidInput(String),
//! }
//!
//! fn inner() -> Result<(), At<MyError>> {
//!     Err(at(MyError::NotFound))  // at() wraps and captures location
//! }
//!
//! fn outer() -> Result<(), At<MyError>> {
//!     inner().at()?;  // .at() adds another location
//!     Ok(())
//! }
//!
//! let err = outer().unwrap_err();
//! assert_eq!(err.trace_len(), 2);
//! ```
//!
//! ## Adding Context
//!
//! Use `.at_str()` for static strings, `.at_string()` for lazy strings, `.at_data()` for Display, `.at_debug()` for Debug:
//!
//! ```rust
//! use errat::{at, At, ResultAtExt};
//!
//! #[derive(Debug)]
//! enum MyError { IoError }
//!
//! fn read_config() -> Result<(), At<MyError>> {
//!     Err(at(MyError::IoError))
//! }
//!
//! fn init() -> Result<(), At<MyError>> {
//!     read_config().at_str("loading configuration")?;  // static str, zero-cost
//!     Ok(())
//! }
//! ```
//!
//! String context with closure for lazy evaluation (only runs on error):
//!
//! ```rust
//! use errat::{at, At, ResultAtExt};
//!
//! #[derive(Debug)]
//! enum MyError { NotFound }
//!
//! fn load(path: &str) -> Result<(), At<MyError>> {
//!     Err(at(MyError::NotFound))
//! }
//!
//! fn init(path: &str) -> Result<(), At<MyError>> {
//!     load(path).at_string(|| format!("loading {}", path))?;  // only allocates on error
//!     Ok(())
//! }
//! ```
//!
//! ## Converting Non-Traced Errors
//!
//! Use `.trace()` on Results with non-traced errors:
//!
//! ```rust
//! use errat::{At, ResultTraceExt, ResultAtExt};
//!
//! fn external_api() -> Result<(), &'static str> {
//!     Err("external error")
//! }
//!
//! fn wrapper() -> Result<(), At<&'static str>> {
//!     external_api().trace()?;  // converts to At
//!     Ok(())
//! }
//! ```
//!
//! ## Allocation Failure Behavior
//!
//! Vec and String allocations use stable `try_reserve` APIs and silently fail on OOM.
//! Box allocations use `Box::new` (Box::try_new is not yet stable) which can panic on OOM.
//!
//! If memory allocation fails:
//! - Vec/String trace entries are silently skipped
//! - The error `E` itself always propagates (it's stored inline in `At<E>`)
//! - Box allocation failure will panic (rare in practice)

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::panic::Location;

// ============================================================================
// LocationVec - configurable storage for trace locations
// ============================================================================
//
// When tinyvec features are enabled, we use TinyVec which starts with inline
// storage and spills to heap when capacity is exceeded. We use Option<&Location>
// as the element type because tinyvec requires Default, and Option<&T> has the
// same size as &T due to null pointer optimization.

/// Stack-first location storage with 3 inline slots (tinyvec-64-bytes: sizeof(Trace) = 64).
#[cfg(all(
    feature = "tinyvec-64-bytes",
    not(any(feature = "tinyvec-128-bytes", feature = "tinyvec-256-bytes"))
))]
type LocationVec = tinyvec::TinyVec<[Option<&'static Location<'static>>; 3]>;

/// Stack-first location storage with 11 inline slots (tinyvec-128-bytes: sizeof(Trace) = 128).
#[cfg(all(feature = "tinyvec-128-bytes", not(feature = "tinyvec-256-bytes")))]
type LocationVec = tinyvec::TinyVec<[Option<&'static Location<'static>>; 11]>;

/// Stack-first location storage with 27 inline slots (tinyvec-256-bytes: sizeof(Trace) = 256).
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

/// Try to allocate a Box. Returns Some on success.
/// Note: Box::try_new is not yet stable, so this can panic on OOM.
/// The error E is stored inline, so even if tracing fails, the error propagates.
#[inline]
fn try_box<T>(value: T) -> Option<Box<T>> {
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
// Core Types
// ============================================================================

/// An error with location tracking - wraps any error type.
///
/// ## Size
///
/// `At<E>` is `sizeof(E) + 8` bytes on 64-bit platforms:
/// - The error `E` is stored inline
/// - The trace is boxed (8-byte pointer, null when empty)
///
/// ## Example
///
/// ```rust
/// use errat::{at, At};
///
/// #[derive(Debug)]
/// enum MyError { Oops }
///
/// // Create a traced error using at() function
/// let err: At<MyError> = at(MyError::Oops);
/// assert_eq!(err.trace_len(), 1);
/// ```
///
/// ## Note: Avoid `At<At<E>>`
///
/// Nesting `At<At<E>>` is supported but unnecessary and wasteful.
/// Each `At` has its own trace, so nesting allocates two `Box<Trace>`
/// instead of one. Use `.at()` on Results to extend the existing trace:
///
/// ```rust
/// use errat::{at, At};
///
/// #[derive(Debug)]
/// struct MyError;
///
/// // GOOD: Extend existing trace
/// fn good() -> Result<(), At<MyError>> {
///     let err: At<MyError> = at(MyError);
///     Err(err.at())  // Same trace, new location
/// }
///
/// // UNNECESSARY: Creates two separate traces
/// fn unnecessary() -> At<At<MyError>> {
///     at(at(MyError))  // Two allocations
/// }
/// ```
pub struct At<E> {
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
// CrateInfo - Static metadata about a crate for cross-crate tracing
// ============================================================================

/// Static metadata about a crate, used for generating repository links.
///
/// Create using the `crate_info!()` macro which captures compile-time values.
/// This is stored as a `&'static` reference in trace entries.
#[derive(Debug, Clone, Copy)]
pub struct CrateInfo {
    /// Crate name (from CARGO_PKG_NAME)
    pub name: &'static str,
    /// Repository URL (from CARGO_PKG_REPOSITORY or #[errat(repo = "...")])
    pub repo: Option<&'static str>,
    /// Git commit hash or tag for generating permalinks
    pub commit: Option<&'static str>,
    /// Module path where this info was captured
    pub module: &'static str,
}

impl CrateInfo {
    /// Create a new CrateInfo with all fields specified.
    pub const fn new(
        name: &'static str,
        repo: Option<&'static str>,
        commit: Option<&'static str>,
        module: &'static str,
    ) -> Self {
        Self {
            name,
            repo,
            commit,
            module,
        }
    }
}

/// Captures crate metadata at the call site.
///
/// Returns a `&'static CrateInfo` reference that can be used with
/// `.at_crate()` to mark crate boundaries in traces.
///
/// ## Commit Hash Capture
///
/// For GitHub permalink generation, set one of these env vars at build time:
/// - `GIT_COMMIT` - explicit commit hash
/// - `GITHUB_SHA` - set automatically by GitHub Actions
/// - `CI_COMMIT_SHA` - set automatically by GitLab CI
///
/// ### Option 1: build.rs (recommended)
///
/// ```ignore
/// // build.rs
/// fn main() {
///     if let Ok(output) = std::process::Command::new("git")
///         .args(["rev-parse", "HEAD"])
///         .output()
///     {
///         if let Ok(hash) = String::from_utf8(output.stdout) {
///             println!("cargo:rustc-env=GIT_COMMIT={}", hash.trim());
///         }
///     }
/// }
/// ```
///
/// ### Option 2: .cargo/config.toml
///
/// ```ignore
/// [env]
/// GIT_COMMIT = "abc123..."  # manual, or use script to update
/// ```
///
/// ### Option 3: Command line
///
/// ```ignore
/// GIT_COMMIT=$(git rev-parse HEAD) cargo build --release
/// ```
///
/// ## Example
///
/// ```rust
/// use errat::{crate_info, CrateInfo};
///
/// let info: &'static CrateInfo = crate_info!();
/// assert_eq!(info.name, "errat");
/// ```
#[macro_export]
macro_rules! crate_info {
    () => {{
        // Use match instead of .or() because Option::or isn't const on stable
        static INFO: $crate::CrateInfo = $crate::CrateInfo::new(
            env!("CARGO_PKG_NAME"),
            option_env!("CARGO_PKG_REPOSITORY"),
            match option_env!("GIT_COMMIT") {
                Some(c) => Some(c),
                None => match option_env!("GITHUB_SHA") {
                    Some(c) => Some(c),
                    None => option_env!("CI_COMMIT_SHA"),
                },
            },
            module_path!(),
        );
        &INFO
    }};
}

/// Start tracing an error with crate metadata for repository links.
///
/// This is equivalent to `err.start_at().at_crate(crate_info!())` but more concise.
/// Use this when creating new traced errors in your crate.
///
/// ## Example
///
/// ```rust
/// use errat::{at, At};
///
/// #[derive(Debug)]
/// enum MyError { NotFound }
///
/// fn find_user(id: u64) -> Result<String, At<MyError>> {
///     if id == 0 {
///         return Err(at!(MyError::NotFound));
///     }
///     Ok(format!("User {}", id))
/// }
/// ```
#[macro_export]
macro_rules! at {
    ($err:expr) => {{ $crate::At::new($err).at().at_crate($crate::crate_info!()) }};
}

/// Add crate boundary marker to a Result with an At<E> error.
///
/// This is syntactic sugar for `result.at_crate(crate_info!())`.
/// Use at crate boundaries when consuming errors from dependencies.
///
/// ## Example
///
/// ```rust
/// use errat::{at, at_crate, At, ResultAtExt};
///
/// #[derive(Debug)]
/// enum MyError { External(String) }
///
/// fn external_call() -> Result<(), At<MyError>> {
///     Err(at(MyError::External("dependency failed".into())))
/// }
///
/// fn my_function() -> Result<(), At<MyError>> {
///     at_crate!(external_call())?;  // Mark crate boundary
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! at_crate {
    ($result:expr) => {{ $crate::ResultAtExt::at_crate($result, $crate::crate_info!()) }};
}

/// Start tracing an error with a skip marker indicating late entry.
///
/// Use this when wrapping an error that originated elsewhere and you want
/// to indicate that the trace doesn't show the full call stack.
///
/// ## Example
///
/// ```rust
/// use errat::{start_at_late, At};
///
/// #[derive(Debug)]
/// enum MyError { Legacy(String) }
///
/// // Wrapping an error from code that doesn't use errat
/// fn wrap_legacy_error(msg: &str) -> At<MyError> {
///     start_at_late!(MyError::Legacy(msg.into()))
/// }
///
/// let err = wrap_legacy_error("old system failed");
/// // Debug output will show [...] to indicate skipped frames
/// let output = format!("{:?}", err);
/// assert!(output.contains("[...]"));
/// ```
#[macro_export]
macro_rules! start_at_late {
    ($err:expr) => {{ $crate::At::new($err).at_skipped() }};
}

/// Wrap any value in `At<E>` and capture the caller's location.
///
/// This function works with any type, not just `Error` types.
/// For types implementing `Error`, you can also use `.start_at()`.
/// For crate-aware tracing with GitHub links, use `at!()` instead.
///
/// ## Example
///
/// ```rust
/// use errat::{at, At};
///
/// #[derive(Debug)]
/// struct SimpleError { code: u32 }
///
/// fn fallible() -> Result<(), At<SimpleError>> {
///     Err(at(SimpleError { code: 42 }))
/// }
/// ```
#[track_caller]
#[inline]
pub fn at<E>(err: E) -> At<E> {
    At::new(err).at()
}

// ============================================================================
// Context Enum
// ============================================================================

/// Context data attached to a trace segment.
///
/// Can be a simple string message, typed data (Debug/Display), or
/// crate boundary information for cross-crate tracing.
pub enum Context {
    /// A text message describing what operation was being performed.
    /// Uses `Cow<'static, str>` for zero-copy static strings.
    Text(Cow<'static, str>),
    /// Typed context data formatted via Debug.
    Debug(Box<dyn DebugAny>),
    /// Typed context data formatted via Display.
    Display(Box<dyn DisplayAny>),
    /// Crate boundary marker - changes the assumed crate for subsequent locations.
    /// Used for generating correct repository links in cross-crate traces.
    Crate(&'static CrateInfo),
    /// Marker indicating that some frames were skipped.
    /// Used when starting tracing late or skipping intermediate frames.
    /// Displayed as `[...]` in trace output.
    Skipped,
}

impl Context {
    /// Get as text, if this is a Text variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Context::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Get as crate info, if this is a Crate variant.
    pub fn as_crate_info(&self) -> Option<&'static CrateInfo> {
        match self {
            Context::Crate(info) => Some(info),
            _ => None,
        }
    }

    /// Try to downcast to a specific type, if this is a typed variant.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        match self {
            Context::Text(_) | Context::Crate(_) | Context::Skipped => None,
            // Must use (**b) to call as_any on the trait object, not the Box
            // (Box<dyn DebugAny> itself implements DebugAny through the blanket impl)
            Context::Debug(b) => (**b).as_any().downcast_ref(),
            Context::Display(b) => (**b).as_any().downcast_ref(),
        }
    }

    /// Get the type name if this is a typed variant.
    pub fn type_name(&self) -> Option<&'static str> {
        match self {
            Context::Text(_) | Context::Crate(_) | Context::Skipped => None,
            Context::Debug(b) => Some((**b).type_name()),
            Context::Display(b) => Some((**b).type_name()),
        }
    }

    /// Check if this context uses Display formatting.
    pub fn is_display(&self) -> bool {
        matches!(self, Context::Text(_) | Context::Display(_))
    }

    /// Check if this is a crate boundary marker.
    pub fn is_crate_boundary(&self) -> bool {
        matches!(self, Context::Crate(_))
    }

    /// Check if this is a skip marker.
    pub fn is_skipped(&self) -> bool {
        matches!(self, Context::Skipped)
    }
}

impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Context::Text(s) => write!(f, "{:?}", s),
            Context::Debug(t) => write!(f, "{:?}", &**t),
            Context::Display(t) => write!(f, "{}", &**t), // Display types use Display even in Debug
            Context::Crate(info) => write!(f, "[crate: {}]", info.name),
            Context::Skipped => write!(f, "[...]"),
        }
    }
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Context::Text(s) => write!(f, "{}", s),
            Context::Debug(t) => write!(f, "{:?}", &**t), // Debug types use Debug in Display
            Context::Display(t) => write!(f, "{}", &**t),
            Context::Crate(info) => write!(f, "[crate: {}]", info.name),
            Context::Skipped => write!(f, "[...]"),
        }
    }
}

/// Internal trace storage - boxed to keep At<E> small.
///
/// Flat structure: contiguous location array + sparse context associations.
/// Contexts are associated with locations by index (u16, saturating).
struct Trace {
    /// All locations in order (oldest first).
    locations: LocationVec,
    /// Context associations: (location_index, context).
    /// Index saturates at u16::MAX; out-of-bounds associations are silently ignored.
    contexts: Vec<(u16, Context)>,
}

impl Trace {
    fn new() -> Self {
        Self {
            locations: LocationVec::new(),
            contexts: Vec::new(),
        }
    }

    /// Try to create a Trace with pre-allocated capacity.
    /// Returns None if allocation fails (Vec) or always succeeds (TinyVec).
    fn try_with_capacity(cap: usize) -> Option<Self> {
        Some(Self {
            locations: try_location_vec_with_capacity(cap)?,
            contexts: Vec::new(),
        })
    }

    /// Try to push a location. Returns false if allocation fails.
    #[inline]
    fn try_push(&mut self, loc: &'static Location<'static>) -> bool {
        try_push_location(&mut self.locations, loc)
    }

    /// Try to push a location with context.
    /// On allocation failure, the location/context may be lost but existing data is preserved.
    fn try_push_with_context(&mut self, loc: &'static Location<'static>, context: Context) {
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

    #[inline]
    fn len(&self) -> usize {
        self.locations.len()
    }

    /// Iterate over all locations, oldest first.
    fn iter(&self) -> impl Iterator<Item = &'static Location<'static>> + '_ {
        self.locations.iter().map(|elem| unwrap_location(elem))
    }

    /// Get the most recent context message (text only).
    fn message(&self) -> Option<&str> {
        // Contexts are in order of addition, so iterate backwards for most recent
        for (_, ctx) in self.contexts.iter().rev() {
            if let Context::Text(msg) = ctx {
                return Some(msg);
            }
        }
        None
    }

    /// Iterate over all context entries, newest first.
    fn contexts(&self) -> impl Iterator<Item = &Context> {
        self.contexts.iter().rev().map(|(_, ctx)| ctx)
    }

    /// Get context at a specific location index, if any.
    fn context_at(&self, idx: usize) -> Option<&Context> {
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

// ============================================================================
// At<E> Implementation
// ============================================================================

impl<E> At<E> {
    /// Create a new traced error without any location information.
    ///
    /// Use `.at()` to add the first location, or use the `ErrorAtExt::at()` method
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
    /// use errat::At;
    ///
    /// #[derive(Debug)]
    /// enum MyError { Oops }
    ///
    /// fn inner() -> Result<(), At<MyError>> {
    ///     Err(At::new(MyError::Oops).at())
    /// }
    ///
    /// fn outer() -> Result<(), At<MyError>> {
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
                let mut trace = Trace::try_with_capacity(6).unwrap_or_else(Trace::new);
                let _ = trace.try_push(loc);
                if let Some(boxed) = try_box(trace) {
                    self.trace = Some(boxed);
                }
            }
        }
        self
    }

    /// Add the caller's location and a static string context to the trace.
    ///
    /// This is zero-cost for static strings - just stores a pointer.
    /// For dynamically-computed strings, use `at_string()` instead.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{at, At, ResultAtExt};
    ///
    /// #[derive(Debug)]
    /// enum MyError { IoError }
    ///
    /// fn read_config() -> Result<(), At<MyError>> {
    ///     Err(at(MyError::IoError))
    /// }
    ///
    /// fn init() -> Result<(), At<MyError>> {
    ///     read_config().at_str("while loading configuration")?;
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn at_str(mut self, msg: &'static str) -> Self {
        let loc = Location::caller();
        let context = Context::Text(Cow::Borrowed(msg));

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

    /// Add the caller's location and a lazily-computed string context to the trace.
    ///
    /// The closure is only called on error path, avoiding allocation on success.
    /// For static strings, use `at_str()` instead for zero overhead.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{at, At, ResultAtExt};
    ///
    /// #[derive(Debug)]
    /// enum MyError { NotFound }
    ///
    /// fn load(path: &str) -> Result<(), At<MyError>> {
    ///     Err(at(MyError::NotFound))
    /// }
    ///
    /// fn init(path: &str) -> Result<(), At<MyError>> {
    ///     // Closure only runs on Err - no allocation on Ok path
    ///     load(path).at_string(|| format!("loading {}", path))?;
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn at_string(mut self, f: impl FnOnce() -> String) -> Self {
        let loc = Location::caller();
        let context = Context::Text(Cow::Owned(f()));

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

    /// Add the caller's location and lazily-computed typed context (Display formatted).
    ///
    /// The closure is only called on error path, avoiding allocation on success.
    /// Use for typed data that you want to format with `Display` and later retrieve
    /// via `downcast_ref::<T>()`.
    ///
    /// For plain string messages, prefer `at_str()` or `at_string()`.
    /// For Debug-formatted data, use `at_debug()`.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{at, At, Context};
    ///
    /// #[derive(Debug)]
    /// enum MyError { NotFound }
    ///
    /// // Custom Display type for rich context
    /// struct PathContext(String);
    /// impl std::fmt::Display for PathContext {
    ///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    ///         write!(f, "path: {}", self.0)
    ///     }
    /// }
    ///
    /// fn load(path: &str) -> Result<(), At<MyError>> {
    ///     Err(at(MyError::NotFound))
    /// }
    ///
    /// fn init(path: &str) -> Result<(), At<MyError>> {
    ///     load(path).map_err(|e| e.at_data(|| PathContext(path.into())))?;
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn at_data<T: fmt::Display + Send + Sync + 'static>(
        mut self,
        f: impl FnOnce() -> T,
    ) -> Self {
        let loc = Location::caller();
        let ctx = f();
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

    /// Add the caller's location and lazily-computed typed context (Debug formatted).
    ///
    /// The closure is only called on error path, avoiding allocation on success.
    /// Use `contexts()` to retrieve entries and `downcast_ref` to access typed data.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{at, At, Context};
    ///
    /// #[derive(Debug)]
    /// struct RequestInfo { user_id: u64, path: String }
    ///
    /// #[derive(Debug)]
    /// enum MyError { Forbidden }
    ///
    /// let err = at(MyError::Forbidden)
    ///     .at_debug(|| RequestInfo { user_id: 42, path: "/admin".into() });
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
    pub fn at_debug<T: fmt::Debug + Send + Sync + 'static>(
        mut self,
        f: impl FnOnce() -> T,
    ) -> Self {
        let loc = Location::caller();
        let ctx = f();
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

    /// Add a crate boundary marker to the trace.
    ///
    /// This marks that subsequent locations belong to a different crate,
    /// enabling correct GitHub links in cross-crate traces.
    ///
    /// Use `crate_info!()` to capture the current crate's metadata.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{at, At, crate_info};
    ///
    /// #[derive(Debug)]
    /// enum MyError { Wrapped(String) }
    ///
    /// // When receiving an error from a dependency:
    /// fn wrap_external_error(msg: &str) -> At<MyError> {
    ///     at(MyError::Wrapped(msg.into()))
    ///         .at_crate(crate_info!())  // Mark crate boundary
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn at_crate(mut self, info: &'static CrateInfo) -> Self {
        let loc = Location::caller();
        let context = Context::Crate(info);

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

    /// Add a skip marker (`[...]`) to the trace.
    ///
    /// Use this to indicate that some frames were skipped, either because
    /// tracing started late in the call stack or because intermediate frames
    /// are not meaningful.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{at, At};
    ///
    /// #[derive(Debug)]
    /// enum MyError { NotFound }
    ///
    /// // When you receive an error but want to indicate the origin is elsewhere
    /// fn handle_legacy_error() -> At<MyError> {
    ///     at(MyError::NotFound).at_skipped()
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn at_skipped(mut self) -> Self {
        let loc = Location::caller();
        let context = Context::Skipped;

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
}

impl<E: fmt::Debug> fmt::Debug for At<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Error header
        writeln!(f, "Error: {:?}", self.error)?;

        let Some(trace) = &self.trace else {
            return Ok(());
        };

        writeln!(f)?;

        // Simple iteration: walk locations, check for context at each index
        for (i, loc) in trace.iter().enumerate() {
            writeln!(f, "    at {}:{}", loc.file(), loc.line())?;
            if let Some(context) = trace.context_at(i) {
                match context {
                    Context::Text(msg) => writeln!(f, "       ╰─ {}", msg)?,
                    Context::Debug(t) => writeln!(f, "       ╰─ {:?}", &**t)?,
                    Context::Display(t) => writeln!(f, "       ╰─ {}", &**t)?,
                    Context::Crate(_) => {} // Crate boundaries don't display in basic Debug
                    Context::Skipped => writeln!(f, "       [...]")?,
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// Enhanced display with CrateInfo from trace
// ============================================================================

impl<E: fmt::Debug> At<E> {
    /// Format the error with GitHub links using CrateInfo from the trace.
    ///
    /// When you use `at!()` or `.at_crate(crate_info!())`, the crate metadata
    /// is stored in the trace. This method uses that metadata to generate
    /// clickable GitHub links for each location.
    ///
    /// For cross-crate traces, each `at_crate()` call updates the repository
    /// used for subsequent locations until another crate boundary is encountered.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{at, At};
    ///
    /// #[derive(Debug)]
    /// struct MyError;
    ///
    /// // at!() captures crate info automatically
    /// let err = at!(MyError);
    /// println!("{}", err.display_with_meta());
    /// ```
    pub fn display_with_meta(&self) -> impl fmt::Display + '_ {
        DisplayWithMeta { traced: self }
    }
}

/// Wrapper for displaying At<E> with CrateInfo enhancements.
struct DisplayWithMeta<'a, E> {
    traced: &'a At<E>,
}

impl<E: fmt::Debug> fmt::Display for DisplayWithMeta<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Error header
        writeln!(f, "Error: {:?}", self.traced.error)?;

        let Some(trace) = &self.traced.trace else {
            return Ok(());
        };

        // Find initial CrateInfo from first Context::Crate in trace
        let mut current_crate: Option<&'static CrateInfo> = None;
        for ctx in trace.contexts() {
            if let Context::Crate(info) = ctx {
                current_crate = Some(info);
                break;
            }
        }

        // Show crate info if available
        if let Some(info) = current_crate {
            writeln!(f, "  crate: {}", info.name)?;
        }

        writeln!(f)?;

        // Walk locations, updating crate context as we encounter Crate entries
        for (i, loc) in trace.iter().enumerate() {
            // Check for crate boundary at this location
            if let Some(Context::Crate(info)) = trace.context_at(i) {
                current_crate = Some(info);
            }

            // Build GitHub URL if crate info is available
            let github_base: Option<String> =
                current_crate.and_then(|info| match (info.repo, info.commit) {
                    (Some(repo), Some(commit)) => {
                        let repo = repo.trim_end_matches('/');
                        Some(alloc::format!("{}/blob/{}/", repo, commit))
                    }
                    _ => None,
                });

            write_location_meta(f, loc, github_base.as_deref())?;

            // Show non-crate context
            if let Some(context) = trace.context_at(i) {
                match context {
                    Context::Text(msg) => writeln!(f, "       ╰─ {}", msg)?,
                    Context::Debug(t) => writeln!(f, "       ╰─ {:?}", &**t)?,
                    Context::Display(t) => writeln!(f, "       ╰─ {}", &**t)?,
                    Context::Crate(_) => {} // Already handled above
                    Context::Skipped => writeln!(f, "       [...]")?,
                }
            }
        }

        Ok(())
    }
}

/// Helper to write a location with optional GitHub link.
fn write_location_meta(
    f: &mut fmt::Formatter<'_>,
    loc: &'static Location<'static>,
    github_base: Option<&str>,
) -> fmt::Result {
    writeln!(f, "    at {}:{}", loc.file(), loc.line())?;
    if let Some(base) = github_base {
        // Convert backslashes to forward slashes for Windows paths
        let file = loc.file().replace('\\', "/");
        writeln!(f, "       {}{}#L{}", base, file, loc.line())?;
    }
    Ok(())
}

impl<E: fmt::Display> fmt::Display for At<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl<E: core::error::Error> core::error::Error for At<E> {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.error.source()
    }
}

// ============================================================================
// ErrorAtExt Trait - for calling .at() directly on error values
// ============================================================================

/// Extension trait that allows calling `.start_at()` on error types.
///
/// This trait is implemented for all types that implement `core::error::Error`.
/// For types without `Error`, use the `at()` function or `at!()` macro instead.
///
/// ```rust
/// use errat::{ErrorAtExt, ResultAtExt};
/// use core::fmt;
///
/// #[derive(Debug)]
/// enum MyError { NotFound }
///
/// impl fmt::Display for MyError {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         match self { MyError::NotFound => write!(f, "not found") }
///     }
/// }
///
/// impl core::error::Error for MyError {}
///
/// fn inner() -> Result<(), errat::At<MyError>> {
///     Err(MyError::NotFound.start_at())
/// }
///
/// fn outer() -> Result<(), errat::At<MyError>> {
///     inner().at()?;  // .at() adds another location
///     Ok(())
/// }
/// ```
pub trait ErrorAtExt: Sized {
    /// Wrap this value in `At<E>` and add the caller's location.
    /// If allocation fails, the error is still wrapped but trace may be empty.
    ///
    /// For crate-aware tracing with repository links, use `at!(err)` or
    /// `err.start_at().at_crate(crate_info!())` instead.
    ///
    /// After calling `.start_at()`, you can chain context methods:
    /// - `.at_str("msg")` - static string context (zero-cost)
    /// - `.at_string(|| format!(...))` - lazy string context
    /// - `.at_data(|| value)` - lazy typed context (Display)
    /// - `.at_debug(|| value)` - lazy typed context (Debug)
    #[track_caller]
    fn start_at(self) -> At<Self>;
}

impl<E: core::error::Error> ErrorAtExt for E {
    #[track_caller]
    #[inline]
    fn start_at(self) -> At<Self> {
        At::new(self).at()
    }
}

// ============================================================================
// ResultAtExt Trait - for calling .at() on Results with At<E> errors
// ============================================================================

/// Extension trait for adding location tracking to `Result<T, At<E>>`.
///
/// ## Example
///
/// ```rust
/// use errat::{at, At, ResultAtExt};
///
/// #[derive(Debug)]
/// enum MyError { Oops }
///
/// fn inner() -> Result<(), At<MyError>> {
///     Err(at(MyError::Oops))
/// }
///
/// fn outer() -> Result<(), At<MyError>> {
///     inner().at()?;
///     Ok(())
/// }
/// ```
pub trait ResultAtExt<T, E> {
    /// Add the caller's location to the error trace if this is `Err`.
    #[track_caller]
    fn at(self) -> Result<T, At<E>>;

    /// Add location and static message context. Zero-cost for static strings.
    #[track_caller]
    fn at_str(self, msg: &'static str) -> Result<T, At<E>>;

    /// Add location and lazily-computed string context.
    #[track_caller]
    fn at_string(self, f: impl FnOnce() -> String) -> Result<T, At<E>>;

    /// Add location and lazily-computed typed context (Display formatted).
    #[track_caller]
    fn at_data<C: fmt::Display + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>>;

    /// Add location and lazily-computed typed context (Debug formatted).
    #[track_caller]
    fn at_debug<C: fmt::Debug + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>>;

    /// Add a crate boundary marker. Use with `crate_info!()` for cross-crate tracing.
    #[track_caller]
    fn at_crate(self, info: &'static CrateInfo) -> Result<T, At<E>>;

    /// Add a skip marker to indicate skipped frames.
    #[track_caller]
    fn at_skipped(self) -> Result<T, At<E>>;
}

impl<T, E> ResultAtExt<T, E> for Result<T, At<E>> {
    #[track_caller]
    #[inline]
    fn at(self) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at()),
        }
    }

    #[track_caller]
    #[inline]
    fn at_str(self, msg: &'static str) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_str(msg)),
        }
    }

    #[track_caller]
    #[inline]
    fn at_string(self, f: impl FnOnce() -> String) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_string(f)),
        }
    }

    #[track_caller]
    #[inline]
    fn at_data<C: fmt::Display + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_data(f)),
        }
    }

    #[track_caller]
    #[inline]
    fn at_debug<C: fmt::Debug + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_debug(f)),
        }
    }

    #[track_caller]
    #[inline]
    fn at_crate(self, info: &'static CrateInfo) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_crate(info)),
        }
    }

    #[track_caller]
    #[inline]
    fn at_skipped(self) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_skipped()),
        }
    }
}

/// Extension trait for converting non-traced errors to traced errors.
///
/// Use `.trace()` on `Result<T, E>` to wrap the error in `At<E>` and capture
/// the first location. For Results that already have `At<E>`, use `ResultAtExt::at()`.
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
/// fn wrapper() -> Result<(), errat::At<&'static str>> {
///     fallible().trace()?;  // converts to At and captures location
///     Ok(())
/// }
/// ```
pub trait ResultTraceExt<T, E> {
    /// Convert the error to `At<E>` and add the caller's location.
    #[track_caller]
    fn trace(self) -> Result<T, At<E>>;

    /// Convert to `At<E>` with static message context (zero-cost).
    #[track_caller]
    fn trace_str(self, msg: &'static str) -> Result<T, At<E>>;

    /// Convert to `At<E>` with lazily-computed string context.
    #[track_caller]
    fn trace_string(self, f: impl FnOnce() -> String) -> Result<T, At<E>>;

    /// Convert to `At<E>` with lazily-computed typed context (Display formatted).
    #[track_caller]
    fn trace_data<C: fmt::Display + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>>;

    /// Convert to `At<E>` with lazily-computed typed context (Debug formatted).
    #[track_caller]
    fn trace_debug<C: fmt::Debug + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>>;

    /// Convert to `At<E>` with a skip marker indicating late tracing.
    #[track_caller]
    fn trace_skipped(self) -> Result<T, At<E>>;
}

impl<T, E> ResultTraceExt<T, E> for Result<T, E> {
    #[track_caller]
    #[inline]
    fn trace(self) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(At::new(e).at()),
        }
    }

    #[track_caller]
    #[inline]
    fn trace_str(self, msg: &'static str) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(At::new(e).at_str(msg)),
        }
    }

    #[track_caller]
    #[inline]
    fn trace_string(self, f: impl FnOnce() -> String) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(At::new(e).at_string(f)),
        }
    }

    #[track_caller]
    #[inline]
    fn trace_data<C: fmt::Display + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(At::new(e).at_data(f)),
        }
    }

    #[track_caller]
    #[inline]
    fn trace_debug<C: fmt::Debug + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(At::new(e).at_debug(f)),
        }
    }

    #[track_caller]
    #[inline]
    fn trace_skipped(self) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(At::new(e).at_skipped()),
        }
    }
}

// ============================================================================
// From implementations
// ============================================================================

impl<E> From<E> for At<E> {
    #[inline]
    fn from(error: E) -> Self {
        At::new(error)
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

    impl core::error::Error for TestError {}

    #[test]
    fn test_sizeof() {
        use core::mem::size_of;

        // At<E> should be sizeof(E) + 8 (pointer to boxed trace)
        // With alignment, a 1-byte enum becomes 16 bytes total
        assert_eq!(size_of::<Option<Box<Trace>>>(), 8);

        let traced_size = size_of::<At<TestError>>();
        let error_size = size_of::<TestError>();
        let pointer_size = size_of::<Option<Box<Trace>>>();

        // Should be error + pointer, with possible padding
        assert!(traced_size <= error_size + pointer_size + 8); // Allow for alignment
        assert!(traced_size >= error_size + pointer_size);

        // For a 1-byte enum, should be 16 bytes (1 + 7 padding + 8 pointer)
        assert_eq!(traced_size, 16);
    }

    #[test]
    fn test_sizeof_trace() {
        use core::mem::size_of;

        let trace_size = size_of::<Trace>();
        let location_vec_size = size_of::<LocationVec>();
        // Print sizes for documentation (visible with cargo test -- --nocapture)
        // Trace = LocationVec + Vec<(u16, Context)>

        // Without tinyvec: LocationVec = Vec = 24, contexts = 24, Trace = 48
        #[cfg(not(any(
            feature = "tinyvec-64-bytes",
            feature = "tinyvec-128-bytes",
            feature = "tinyvec-256-bytes"
        )))]
        {
            let contexts_vec_size = size_of::<Vec<(u16, Context)>>();
            assert_eq!(location_vec_size, 24, "Vec<&Location> should be 24 bytes");
            assert_eq!(
                contexts_vec_size, 24,
                "Vec<(u16, Context)> should be 24 bytes"
            );
            assert_eq!(trace_size, 48, "Trace should be 48 bytes without tinyvec");
        }

        // With tinyvec-64-bytes (3 slots): sizeof(Trace) = 64 bytes exactly
        #[cfg(all(
            feature = "tinyvec-64-bytes",
            not(any(feature = "tinyvec-128-bytes", feature = "tinyvec-256-bytes"))
        ))]
        {
            assert_eq!(
                location_vec_size, 40,
                "TinyVec<[Option<&Location>; 3]> should be 40 bytes"
            );
            assert_eq!(
                trace_size, 64,
                "Trace with tinyvec-64-bytes should be exactly 64 bytes"
            );
        }

        // With tinyvec-128-bytes (11 slots): sizeof(Trace) = 128 bytes exactly
        #[cfg(all(feature = "tinyvec-128-bytes", not(feature = "tinyvec-256-bytes")))]
        {
            assert_eq!(
                location_vec_size, 104,
                "TinyVec<[Option<&Location>; 11]> should be 104 bytes"
            );
            assert_eq!(
                trace_size, 128,
                "Trace with tinyvec-128-bytes should be exactly 128 bytes"
            );
        }

        // With tinyvec-256-bytes (27 slots): sizeof(Trace) = 256 bytes exactly
        #[cfg(feature = "tinyvec-256-bytes")]
        {
            assert_eq!(
                location_vec_size, 232,
                "TinyVec<[Option<&Location>; 27]> should be 232 bytes"
            );
            assert_eq!(
                trace_size, 256,
                "Trace with tinyvec-256-bytes should be exactly 256 bytes"
            );
        }
    }

    #[test]
    fn test_basic_trace() {
        let err = TestError::NotFound.start_at();
        assert_eq!(*err.error(), TestError::NotFound);
        assert_eq!(err.trace_len(), 1);
        assert!(!err.trace_is_empty());
    }

    #[test]
    fn test_propagation() {
        fn inner() -> Result<(), At<TestError>> {
            Err(TestError::NotFound.start_at())
        }

        fn middle() -> Result<(), At<TestError>> {
            inner().at()
        }

        fn outer() -> Result<(), At<TestError>> {
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

        fn wrapper() -> Result<(), At<&'static str>> {
            fallible().trace()?;
            Ok(())
        }

        let err = wrapper().unwrap_err();
        assert_eq!(*err.error(), "oops");
        assert_eq!(err.trace_len(), 1);
    }

    #[test]
    fn test_into_inner() {
        let err = TestError::InvalidInput.start_at();
        let inner = err.into_inner();
        assert_eq!(inner, TestError::InvalidInput);
    }

    #[test]
    fn test_first_last_location() {
        fn level1() -> Result<(), At<TestError>> {
            Err(TestError::NotFound.start_at())
        }

        fn level2() -> Result<(), At<TestError>> {
            level1().at()
        }

        fn level3() -> Result<(), At<TestError>> {
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
        let err = TestError::NotFound.start_at();

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
        let err: At<TestError> = At::new(TestError::NotFound);
        assert_eq!(err.trace_len(), 0);
        assert!(err.trace_is_empty());
        assert!(err.first_location().is_none());
        assert!(err.last_location().is_none());
    }

    #[test]
    fn test_from_impl() {
        let err: At<TestError> = TestError::NotFound.into();
        assert_eq!(*err.error(), TestError::NotFound);
        assert!(err.trace_is_empty()); // From doesn't add trace
    }

    #[test]
    fn test_error_mut() {
        #[derive(Debug)]
        struct MutableError {
            count: u32,
        }

        let mut err = at(MutableError { count: 0 });
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

        let err = at(LargeError {
            message: String::from("test"),
            code: 42,
            data: [0; 32],
        });

        assert_eq!(err.trace_len(), 1);
        assert_eq!(err.error().code, 42);
    }

    #[test]
    fn test_at_str() {
        let err = TestError::NotFound.start_at().at_str("while fetching user");
        assert_eq!(err.trace_len(), 2); // start_at + at_str
        assert_eq!(err.message(), Some("while fetching user"));
    }

    #[test]
    fn test_str_propagation() {
        fn inner() -> Result<(), At<TestError>> {
            Err(TestError::NotFound.start_at())
        }

        fn outer() -> Result<(), At<TestError>> {
            inner().at_str("during initialization")?;
            Ok(())
        }

        let err = outer().unwrap_err();
        assert_eq!(err.trace_len(), 2);
        assert_eq!(err.message(), Some("during initialization"));
    }

    #[test]
    fn test_trace_str() {
        fn fallible() -> Result<(), &'static str> {
            Err("oops")
        }

        fn wrapper() -> Result<(), At<&'static str>> {
            fallible().trace_str("while doing something")?;
            Ok(())
        }

        let err = wrapper().unwrap_err();
        assert_eq!(*err.error(), "oops");
        assert_eq!(err.message(), Some("while doing something"));
    }

    #[test]
    fn test_debug_with_message() {
        let err = TestError::NotFound.start_at().at_str("context info");
        let debug = alloc::format!("{:?}", err);
        assert!(debug.contains("NotFound"));
        assert!(debug.contains("╰─ context info"));
        assert!(debug.contains("lib.rs"));
    }

    #[test]
    fn test_dbg_ctx_typed() {
        #[derive(Debug)]
        struct RequestInfo {
            user_id: u64,
        }

        let err = TestError::NotFound
            .start_at()
            .at_debug(|| RequestInfo { user_id: 42 });

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
        fn level1() -> Result<(), At<TestError>> {
            Err(TestError::NotFound.start_at())
        }

        fn level2() -> Result<(), At<TestError>> {
            level1().at_str("in level2")?;
            Ok(())
        }

        fn level3() -> Result<(), At<TestError>> {
            level2().at_str("in level3")?;
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

        let text_ctx = Context::Text(String::from("hello").into());
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

        let err = TestError::NotFound.start_at().at_debug(|| MyContext {
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
    fn test_ctx_data() {
        // Use a type that has both Display and Debug but we want Display formatting
        let err = TestError::NotFound
            .start_at()
            .at_data(|| "user-friendly message");

        assert_eq!(err.trace_len(), 2);

        // Check that Display formatting is used in output
        let debug = alloc::format!("{:?}", err);
        assert!(debug.contains("╰─ user-friendly message"));

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
            .start_at()
            .at_str("text message")
            .at_debug(|| DebugInfo { code: 42 })
            .at_data(|| "display message");

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

    #[test]
    fn test_trace_format_structure() {
        // Test that trace format shows locations oldest-first with contexts
        fn level1() -> Result<(), At<TestError>> {
            Err(TestError::NotFound.start_at())
        }

        fn level2() -> Result<(), At<TestError>> {
            level1().at_str("in level2")?;
            Ok(())
        }

        fn level3() -> Result<(), At<TestError>> {
            level2().at_str("in level3")?;
            Ok(())
        }

        let err = level3().unwrap_err();
        let debug = alloc::format!("{:?}", err);

        // Verify structure:
        // - Error header
        assert!(debug.contains("Error: NotFound"));

        // - Locations with contexts
        assert!(debug.contains("╰─ in level2"));
        assert!(debug.contains("╰─ in level3"));

        // - Location lines present
        assert!(debug.contains("at src/lib.rs:"));

        // Verify order: level2 context before level3 context (oldest first)
        let level2_pos = debug.find("in level2").unwrap();
        let level3_pos = debug.find("in level3").unwrap();
        assert!(
            level2_pos < level3_pos,
            "level2 should appear before level3 (oldest first)"
        );
    }

    #[test]
    fn test_trace_origin_comes_first() {
        fn origin() -> Result<(), At<TestError>> {
            Err(TestError::NotFound.start_at())
        }

        fn wrapper() -> Result<(), At<TestError>> {
            origin().at_str("wrapping")?;
            Ok(())
        }

        let err = wrapper().unwrap_err();
        let debug = alloc::format!("{:?}", err);

        // The first "at" line should be from origin (lower line number)
        // and the context "wrapping" should come after
        let lines: Vec<&str> = debug.lines().collect();

        // Find first "at" line
        let first_at = lines.iter().find(|l| l.contains("at src/lib.rs:")).unwrap();

        // It should be the origin location (before the wrapper's context)
        // The origin .start_at() call will be at a lower line than wrapper's .at_str()
        assert!(
            !first_at.contains("╰─"),
            "First location should be origin without context"
        );
    }
}
