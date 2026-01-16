//! The `At<E>` wrapper type for error location tracking.
//!
//! This module provides the core [`At<E>`] type that wraps any error with a trace
//! of source locations. It's the primary API surface for errat.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use core::fmt;
use core::panic::Location;

use crate::AtCrateInfo;
use crate::context::{AtContext, AtContextRef};
use crate::trace::{AtTrace, DEFAULT_TRACE_CAPACITY, try_box};

// ============================================================================
// At<E> - Core wrapper type
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
/// Each `At` has its own trace, so nesting allocates two `Box<AtTrace>`
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
    trace: Option<Box<AtTrace>>,
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
                // Try to create trace with capacity hint, fall back to no capacity
                let mut trace =
                    AtTrace::try_with_capacity(DEFAULT_TRACE_CAPACITY).unwrap_or_default();
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
        let context = AtContext::Text(Cow::Borrowed(msg));

        match &mut self.trace {
            Some(trace) => {
                trace.try_push_with_context(loc, context);
            }
            None => {
                let mut trace = AtTrace::new();
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
        let context = AtContext::Text(Cow::Owned(f()));

        match &mut self.trace {
            Some(trace) => {
                trace.try_push_with_context(loc, context);
            }
            None => {
                let mut trace = AtTrace::new();
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
    /// use errat::{at, At};
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
        let context = AtContext::Display(boxed_ctx);

        match &mut self.trace {
            Some(trace) => {
                trace.try_push_with_context(loc, context);
            }
            None => {
                let mut trace = AtTrace::new();
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
    /// use errat::at;
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
        let context = AtContext::Debug(boxed_ctx);

        match &mut self.trace {
            Some(trace) => {
                trace.try_push_with_context(loc, context);
            }
            None => {
                let mut trace = AtTrace::new();
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
    /// Requires [`define_at_crate_info!()`](crate::define_at_crate_info!) or
    /// a custom `at_crate_info()` getter.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// // Requires define_at_crate_info!() setup
    /// use errat::{at, At};
    ///
    /// errat::define_at_crate_info!();
    ///
    /// #[derive(Debug)]
    /// enum MyError { Wrapped(String) }
    ///
    /// fn wrap_external_error(msg: &str) -> At<MyError> {
    ///     at(MyError::Wrapped(msg.into()))
    ///         .at_crate(crate::at_crate_info())
    /// }
    /// ```
    #[track_caller]
    #[inline]
    pub fn at_crate(mut self, info: &'static AtCrateInfo) -> Self {
        let loc = Location::caller();
        let context = AtContext::Crate(info);

        match &mut self.trace {
            Some(trace) => {
                trace.try_push_with_context(loc, context);
            }
            None => {
                let mut trace = AtTrace::new();
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
    ///     at(MyError::NotFound).at_skipped_frames()
    /// }
    /// ```
    #[inline]
    pub fn at_skipped_frames(mut self) -> Self {
        // None in locations vec = skipped frame marker
        match &mut self.trace {
            Some(trace) => {
                let _ = trace.try_push_skipped();
            }
            None => {
                let mut trace = AtTrace::new();
                let _ = trace.try_push_skipped();
                if let Some(boxed) = try_box(trace) {
                    self.trace = Some(boxed);
                }
            }
        }
        self
    }

    // ========================================================================
    // with_* methods - add context to the current location (no new location)
    // ========================================================================

    /// Add a static string context to the current location (without adding a new location).
    ///
    /// Use this to attach multiple contexts to the same stack frame.
    /// If the trace is empty, the context is silently dropped.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::at;
    ///
    /// #[derive(Debug)]
    /// enum MyError { NotFound }
    ///
    /// // One location with two contexts:
    /// let err = at(MyError::NotFound)
    ///     .at_str("loading config")
    ///     .with_str("for production environment");
    ///
    /// assert_eq!(err.trace_len(), 2); // at() + at_str(), not 3
    /// ```
    #[inline]
    pub fn with_str(mut self, msg: &'static str) -> Self {
        if let Some(trace) = &mut self.trace {
            let context = AtContext::Text(Cow::Borrowed(msg));
            let _ = trace.try_add_context(context);
        }
        self
    }

    /// Add a lazily-computed string context to the current location.
    ///
    /// Like `with_str`, but for dynamically-constructed strings.
    #[inline]
    pub fn with_string(mut self, f: impl FnOnce() -> String) -> Self {
        if let Some(trace) = &mut self.trace {
            let context = AtContext::Text(Cow::Owned(f()));
            let _ = trace.try_add_context(context);
        }
        self
    }

    /// Add lazily-computed typed context (Display formatted) to the current location.
    ///
    /// Like `at_data`, but doesn't add a new location.
    #[inline]
    pub fn with_data<T: fmt::Display + Send + Sync + 'static>(
        mut self,
        f: impl FnOnce() -> T,
    ) -> Self {
        if let Some(trace) = &mut self.trace {
            let ctx = f();
            if let Some(boxed_ctx) = try_box(ctx) {
                let context = AtContext::Display(boxed_ctx);
                let _ = trace.try_add_context(context);
            }
        }
        self
    }

    /// Add lazily-computed typed context (Debug formatted) to the current location.
    ///
    /// Like `at_debug`, but doesn't add a new location.
    #[inline]
    pub fn with_debug<T: fmt::Debug + Send + Sync + 'static>(
        mut self,
        f: impl FnOnce() -> T,
    ) -> Self {
        if let Some(trace) = &mut self.trace {
            let ctx = f();
            if let Some(boxed_ctx) = try_box(ctx) {
                let context = AtContext::Debug(boxed_ctx);
                let _ = trace.try_add_context(context);
            }
        }
        self
    }

    /// Set the crate info for this trace.
    ///
    /// This is used by `at!()` to provide repository metadata for GitHub links.
    /// Calling this creates the trace if it doesn't exist yet.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// // Requires define_at_crate_info!() setup
    /// errat::define_at_crate_info!();
    ///
    /// #[derive(Debug)]
    /// enum MyError { Oops }
    ///
    /// let err = At::new(MyError::Oops)
    ///     .set_crate_info(crate::at_crate_info())
    ///     .at();
    /// ```
    #[inline]
    pub fn set_crate_info(mut self, info: &'static AtCrateInfo) -> Self {
        match &mut self.trace {
            Some(trace) => {
                trace.set_crate_info(info);
            }
            None => {
                let mut trace = AtTrace::new();
                trace.set_crate_info(info);
                if let Some(boxed) = try_box(trace) {
                    self.trace = Some(boxed);
                }
            }
        }
        self
    }

    /// Get the crate info for this trace, if set.
    #[inline]
    pub fn crate_info(&self) -> Option<&'static AtCrateInfo> {
        self.trace.as_ref().and_then(|t| t.crate_info())
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
    ///
    /// Skipped frame markers (`[...]`) are not included in this iteration.
    /// Use `Debug` formatting to see the full trace with skip markers.
    #[inline]
    pub fn trace_iter(&self) -> impl Iterator<Item = &'static Location<'static>> + '_ {
        self.trace.iter().flat_map(|t| t.iter()).flatten() // Filter out None (skipped frame markers)
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

    /// Iterate over all context entries, newest first.
    ///
    /// Each call to `at_str()`, `at_string()`, `at_data()`, or `at_debug()` creates
    /// a context entry. Use [`AtContextRef`] methods to inspect context data.
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
    ///     .at_str("initializing");
    ///
    /// let texts: Vec<_> = err.contexts()
    ///     .filter_map(|ctx| ctx.as_text())
    ///     .collect();
    /// assert_eq!(texts, vec!["initializing", "loading config"]); // newest first
    /// ```
    pub fn contexts(&self) -> impl Iterator<Item = AtContextRef<'_>> {
        self.trace.iter().flat_map(|t| t.contexts())
    }
}

// ============================================================================
// Debug impl for At<E>
// ============================================================================

impl<E: fmt::Debug> fmt::Debug for At<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Error header
        writeln!(f, "Error: {:?}", self.error)?;

        let Some(trace) = &self.trace else {
            return Ok(());
        };

        writeln!(f)?;

        // Simple iteration: walk locations, check for contexts at each index
        // None = skipped frame marker
        for (i, loc_opt) in trace.iter().enumerate() {
            match loc_opt {
                Some(loc) => {
                    writeln!(f, "    at {}:{}", loc.file(), loc.line())?;
                    for context in trace.contexts_at(i) {
                        match context {
                            AtContext::Text(msg) => writeln!(f, "       ╰─ {}", msg)?,
                            AtContext::Debug(t) => writeln!(f, "       ╰─ {:?}", &**t)?,
                            AtContext::Display(t) => writeln!(f, "       ╰─ {}", &**t)?,
                            AtContext::Crate(_) => {} // Crate boundaries don't display in basic Debug
                        }
                    }
                }
                None => {
                    writeln!(f, "    [...]")?;
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// Enhanced display with AtCrateInfo from trace
// ============================================================================

impl<E: fmt::Debug> At<E> {
    /// Format the error with GitHub links using AtCrateInfo from the trace.
    ///
    /// When you use `at!()` or `.at_crate()`, the crate metadata is stored in
    /// the trace. This method uses that metadata to generate clickable GitHub
    /// links for each location.
    ///
    /// For cross-crate traces, each `at_crate()` call updates the repository
    /// used for subsequent locations until another crate boundary is encountered.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// // Requires define_at_crate_info!() setup
    /// use errat::{at, At};
    ///
    /// errat::define_at_crate_info!();
    ///
    /// #[derive(Debug)]
    /// struct MyError;
    ///
    /// let err = at!(MyError);
    /// println!("{}", err.display_with_meta());
    /// ```
    pub fn display_with_meta(&self) -> impl fmt::Display + '_ {
        DisplayWithMeta { traced: self }
    }
}

/// Wrapper for displaying At<E> with AtCrateInfo enhancements.
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

        // Use crate_info field first (set by at!() macro)
        // at_crate() context entries can override this per-location
        let initial_crate = trace.crate_info();

        // Show crate info if available
        if let Some(info) = initial_crate {
            writeln!(f, "  crate: {}", info.name())?;
        }

        writeln!(f)?;

        // Cache GitHub base URL - rebuild when crate boundary changes
        let mut github_base: Option<String> = initial_crate.and_then(build_github_base);

        // Walk locations, updating GitHub base when we encounter crate boundaries
        // None = skipped frame marker
        for (i, loc_opt) in trace.iter().enumerate() {
            // Check for crate boundary at this location - rebuild URL only when crate changes
            for context in trace.contexts_at(i) {
                if let AtContext::Crate(info) = context {
                    github_base = build_github_base(info);
                }
            }

            match loc_opt {
                Some(loc) => {
                    write_location_meta(f, loc, github_base.as_deref())?;

                    // Show non-crate contexts
                    for context in trace.contexts_at(i) {
                        match context {
                            AtContext::Text(msg) => writeln!(f, "       ╰─ {}", msg)?,
                            AtContext::Debug(t) => writeln!(f, "       ╰─ {:?}", &**t)?,
                            AtContext::Display(t) => writeln!(f, "       ╰─ {}", &**t)?,
                            AtContext::Crate(_) => {} // Already handled above
                        }
                    }
                }
                None => {
                    writeln!(f, "    [...]")?;
                }
            }
        }

        Ok(())
    }
}

/// Build GitHub blob URL base from crate info.
/// Returns `{repo}/blob/{commit}/{crate_path}` or None if repo/commit unavailable.
fn build_github_base(info: &AtCrateInfo) -> Option<String> {
    match (info.repo(), info.commit()) {
        (Some(repo), Some(commit)) => {
            let repo = repo.trim_end_matches('/');
            let crate_path = info.crate_path().unwrap_or("");
            Some(alloc::format!("{}/blob/{}/{}", repo, commit, crate_path))
        }
        _ => None,
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

// ============================================================================
// Display impl for At<E>
// ============================================================================

impl<E: fmt::Display> fmt::Display for At<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

// ============================================================================
// Error impl for At<E>
// ============================================================================

impl<E: core::error::Error> core::error::Error for At<E> {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.error.source()
    }
}

// ============================================================================
// From impl for At<E>
// ============================================================================

impl<E> From<E> for At<E> {
    #[inline]
    fn from(error: E) -> Self {
        At::new(error)
    }
}
