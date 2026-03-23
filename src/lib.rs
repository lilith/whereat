//! # whereat — error location tracking
//!
//! **150x faster than `backtrace`** — production error tracing without debuginfo, panic, or overhead.
//!
//! ```text
//! Error: UserNotFound
//!    at src/db.rs:142:9
//!       ╰─ user_id = 42
//!    at src/api.rs:89:5
//!       ╰─ in handle_request
//!    at myapp @ https://github.com/you/myapp/blob/a1b2c3d/src/main.rs#L23
//! ```
//!
//! ## Quick Start
//!
//! [`at!()`](at!) creates a traced error. [`.at()?`](ResultAtExt::at) propagates it.
//! [`map_err_at`](ResultAtExt::map_err_at) converts between error types without losing the trace.
//!
//! ```rust
//! use whereat::{at, At, ResultAtExt};
//!
//! #[derive(Debug)]
//! enum AppError { NotFound, Db(DbError) }
//! #[derive(Debug)]
//! struct DbError;
//!
//! fn db_query(id: u64) -> Result<String, At<DbError>> {
//!     if id == 0 { return Err(at(DbError)); }
//!     Ok("alice".into())
//! }
//!
//! fn handler(id: u64) -> Result<String, At<AppError>> {
//!     let user = db_query(id)
//!         .at()                           // new frame at this call site
//!         .at_str("looking up user")      // context on that frame
//!         .map_err_at(AppError::Db)?;     // DbError → AppError, trace preserved
//!     Ok(user)
//! }
//!
//! let err = handler(0).unwrap_err();
//! assert_eq!(err.frame_count(), 2);
//! ```
//!
//! For **clickable GitHub/GitLab links** in traces, add one line to your crate root:
//!
//! ```rust,ignore
//! whereat::define_at_crate_info!();  // once in lib.rs — auto-detects forge from repo URL
//! ```
//!
//! Then use [`at!(err)`](at!) instead of [`at(err)`](at()). For workspace crates:
//! `whereat::define_at_crate_info!(path = "crates/mylib/");`
//!
//! Or use the prelude: `use whereat::prelude::*;` (re-exports [`At`], [`at`], [`ResultAtExt`], [`ErrorAtExt`]).
//!
//! ## API Reference
//!
//! **Create a trace:**
//!
//! | Function | Use when |
//! |----------|----------|
//! | [`at!(err)`](at!) | Default — includes repo links (needs [`define_at_crate_info!`]) |
//! | [`at(err)`](at()) | No setup needed, no links |
//! | [`err.start_at()`](ErrorAtExt::start_at) | Chaining on [`Error`](core::error::Error) types |
//!
//! **Extend a trace** (on `Result<T, At<E>>` via [`ResultAtExt`]):
//!
//! | Method | Effect |
//! |--------|--------|
//! | [`.at()`](ResultAtExt::at) | **New frame** at caller's location |
//! | [`.at_str("msg")`](ResultAtExt::at_str) | Context on **last frame** (zero-cost) |
//! | [`.at_string(\|\| format!(...))`](ResultAtExt::at_string) | Dynamic context (lazy) |
//! | [`.at_fn(\|\| {})`](ResultAtExt::at_fn) | New frame + function name |
//! | [`.at_named("label")`](ResultAtExt::at_named) | New frame + custom label |
//! | [`.at_data(\|\| val)`](ResultAtExt::at_data) | Typed context via Display (lazy) |
//! | [`.at_debug(\|\| val)`](ResultAtExt::at_debug) | Typed context via Debug (lazy) |
//! | [`.at_aside_error(err)`](ResultAtExt::at_aside_error) | Related error (diagnostic, **not** in [`.source()`](core::error::Error::source) chain) |
//! | [`.map_err_at(\|e\| ...)`](ResultAtExt::map_err_at) | Convert error type, **preserve trace** |
//!
//! **Inspect / decompose:**
//!
//! | Method | Returns |
//! |--------|---------|
//! | [`.error()`](At::error) | `&E` |
//! | [`.decompose()`](At::decompose) | `(E, Option<AtTrace>)` — both pieces |
//! | [`.map_error(\|e\| ...)`](At::map_error) | `At<E2>` — convert type, keep trace |
//! | [`.frame_count()`](At::frame_count) | `usize` |
//! | [`.full_trace()`](At::full_trace) | Display formatter with all frames + contexts |
//!
//! ## Frames vs contexts
//!
//! [`.at()`](ResultAtExt::at) creates a **new frame** (a new location).
//! [`.at_str()`](ResultAtExt::at_str) adds **context** to the last frame.
//!
//! ```rust
//! use whereat::{at, At, ResultAtExt};
//!
//! #[derive(Debug)]
//! struct E;
//!
//! // One frame, two contexts
//! let e = at(E).at_str("a").at_str("b");
//! assert_eq!(e.frame_count(), 1);
//!
//! // Two frames: at() creates first, .at() creates second
//! let e = at(E).at().at_str("on second frame");
//! assert_eq!(e.frame_count(), 2);
//! ```
//!
//! ## Converting between error types
//!
//! Use [`map_err_at`](ResultAtExt::map_err_at) — **not** `map_err` — to convert error types.
//! `map_err` on `Result<T, At<E>>` discards the `At<>` wrapper and the trace with it.
//!
//! ```rust
//! use whereat::{at, At, ResultAtExt};
//!
//! #[derive(Debug)]
//! struct Inner;
//! #[derive(Debug)]
//! enum Outer { Wrapped(Inner) }
//!
//! fn inner() -> Result<(), At<Inner>> { Err(at(Inner)) }
//!
//! fn outer() -> Result<(), At<Outer>> {
//!     inner().map_err_at(Outer::Wrapped)?;  // trace preserved
//!     Ok(())
//! }
//! # outer().unwrap_err();
//! ```
//!
//! ## Wrapping external errors
//!
//! For errors from crates that don't use whereat, use [`map_err(at)`](at()) to start tracing:
//!
//! ```rust
//! use whereat::{At, at, ResultAtExt};
//!
//! fn external_api() -> Result<(), &'static str> { Err("oops") }
//!
//! fn wrapper() -> Result<(), At<&'static str>> {
//!     external_api().map_err(at).at_str("calling external API")?;
//!     Ok(())
//! }
//! # wrapper().unwrap_err();
//! ```
//!
//! ## Avoiding trace loss
//!
//! - Use [`map_err_at`](ResultAtExt::map_err_at) at every error type boundary — not `map_err`
//! - [`into_inner()`](At::into_inner) is **deprecated** — use [`decompose()`](At::decompose) or [`map_error()`](At::map_error)
//! - [`at_error()`](At::at_error) is **deprecated** — use [`at_aside_error()`](At::at_aside_error)
//! - Never implement `From<At<X>> for Y` — it discards the trace. Implement `From<X> for Y` and use `map_err_at`.
//! - [`At::source()`](core::error::Error::source) delegates to `E::source()`. Errors from [`at_aside_error()`](At::at_aside_error) are diagnostic context, **not** in the source chain.
//!
//! ## Which approach?
//!
//! | Situation | Use |
//! |-----------|-----|
//! | Existing struct/enum you don't want to modify | Wrap with [`At<YourError>`](At) |
//! | Want traces embedded inside your error type | Implement [`AtTraceable`] |
//!
//! **Wrapper approach** (most common): Return `Result<T, At<YourError>>`.
//!
//! **Embedded approach**: Implement [`AtTraceable`] with an [`AtTrace`] field. See [`AtTraceable`] docs.
//!
//! ## Design
//!
//! - `At<E>` is `sizeof(E) + 8` bytes — error inline, trace boxed (null when empty)
//! - Zero heap allocation on the Ok path
//! - `.at_str("literal")` stores a pointer, no copy or allocation
//! - `.at_string(|| ...)` closures only run on error
//! - `#![no_std]` with `alloc` — `core::error::Error` is used (stable since Rust 1.81)
//! - OOM: trace allocations use `try_reserve` and silently skip. Your error `E` always propagates.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

mod at;
mod context;
mod crate_info;
mod ext;
#[cfg(any(feature = "_termcolor", feature = "_html"))]
mod format;
mod inline_vec;
pub mod prelude;
mod trace;

pub use at::At;
pub use context::AtContextRef;
pub use crate_info::{
    AtCrateInfo, AtCrateInfoBuilder, BITBUCKET_LINK_FORMAT, GITEA_LINK_FORMAT, GITHUB_LINK_FORMAT,
    GITLAB_LINK_FORMAT,
};
pub use ext::{ErrorAtExt, ResultAtExt, ResultAtTraceableExt};
pub use trace::{
    AT_MAX_CONTEXTS, AT_MAX_FRAMES, AtFrame, AtFrameOwned, AtTrace, AtTraceBoxed, AtTraceable,
};

// ============================================================================
// Crate-level error tracking info (for whereat's own at!() / at_crate!() usage)
// ============================================================================
//
// This is what `define_at_crate_info!()` generates. We define it manually here
// because the macro isn't defined yet at this point in the file.

// whereat's own crate info for internal at!() usage in doctests
#[doc(hidden)]
pub(crate) static __AT_CRATE_INFO: AtCrateInfo = AtCrateInfo::builder()
    .name("whereat")
    .repo(option_env!("CARGO_PKG_REPOSITORY"))
    .commit(match option_env!("GIT_COMMIT") {
        Some(c) => Some(c),
        None => match option_env!("GITHUB_SHA") {
            Some(c) => Some(c),
            None => match option_env!("CI_COMMIT_SHA") {
                Some(c) => Some(c),
                None => Some(concat!("v", env!("CARGO_PKG_VERSION"))),
            },
        },
    })
    .module("whereat")
    .build();

#[doc(hidden)]
pub fn at_crate_info() -> &'static AtCrateInfo {
    &__AT_CRATE_INFO
}

/// Internal macro for commit detection chain.
#[doc(hidden)]
#[macro_export]
macro_rules! __whereat_detect_commit {
    () => {
        match option_env!("GIT_COMMIT") {
            Some(c) => Some(c),
            None => match option_env!("GITHUB_SHA") {
                Some(c) => Some(c),
                None => match option_env!("CI_COMMIT_SHA") {
                    Some(c) => Some(c),
                    None => Some(concat!("v", env!("CARGO_PKG_VERSION"))),
                },
            },
        }
    };
}

/// Define crate-level error tracking info. Call once in your crate root (lib.rs or main.rs).
///
/// This creates a static and getter function that `at!()` and `at_crate!()` use.
/// For compile-time configuration, use this macro. For runtime configuration,
/// define your own `at_crate_info()` function using `OnceLock`.
///
/// ## Basic Usage
///
/// ```rust,ignore
/// // In lib.rs or main.rs
/// whereat::define_at_crate_info!();
/// ```
///
/// ## With Options
///
/// ```rust,ignore
/// whereat::define_at_crate_info!(
///     path = "crates/mylib/",
///     meta = &[("team", "platform"), ("service", "auth")],
/// );
/// ```
///
/// ## Runtime Configuration
///
/// For runtime metadata (e.g., instance IDs), define your own getter:
///
/// ```rust,ignore
/// use std::sync::OnceLock;
/// use whereat::AtCrateInfo;
///
/// static CRATE_INFO: OnceLock<AtCrateInfo> = OnceLock::new();
///
/// pub(crate) fn at_crate_info() -> &'static AtCrateInfo {
///     CRATE_INFO.get_or_init(|| {
///         AtCrateInfo::builder()
///             .name_owned(env!("CARGO_PKG_NAME").into())
///             .meta_owned(vec![("instance_id".into(), get_instance_id())])
///             .build()
///     })
/// }
/// ```
///
/// ## Available Options
///
/// - `path = "..."` - Crate path within repository (for workspace crates)
/// - `meta = &[...]` - Custom key-value metadata (compile-time)
///
/// ## How It Works
///
/// The macro captures at compile time:
/// - `CARGO_PKG_NAME` - crate name
/// - `CARGO_PKG_REPOSITORY` - repository URL from Cargo.toml
/// - `GIT_COMMIT` / `GITHUB_SHA` / `CI_COMMIT_SHA` - commit hash (or `v{VERSION}` fallback)
#[macro_export]
macro_rules! define_at_crate_info {
    // Base case: no options (uses CRATE_PATH from env if set)
    () => {
        #[doc(hidden)]
        #[allow(dead_code)]
        static __AT_CRATE_INFO: $crate::AtCrateInfo = $crate::AtCrateInfo::builder()
            .name(env!("CARGO_PKG_NAME"))
            .repo(option_env!("CARGO_PKG_REPOSITORY"))
            .commit($crate::__whereat_detect_commit!())
            .path(option_env!("CRATE_PATH"))
            .module(module_path!())
            .build();

        #[doc(hidden)]
        #[allow(dead_code)]
        pub(crate) fn at_crate_info() -> &'static $crate::AtCrateInfo {
            &__AT_CRATE_INFO
        }
    };

    // With path only
    (path = $path:literal $(,)?) => {
        #[doc(hidden)]
        #[allow(dead_code)]
        static __AT_CRATE_INFO: $crate::AtCrateInfo = $crate::AtCrateInfo::builder()
            .name(env!("CARGO_PKG_NAME"))
            .repo(option_env!("CARGO_PKG_REPOSITORY"))
            .commit($crate::__whereat_detect_commit!())
            .path(Some($path))
            .module(module_path!())
            .build();

        #[doc(hidden)]
        #[allow(dead_code)]
        pub(crate) fn at_crate_info() -> &'static $crate::AtCrateInfo {
            &__AT_CRATE_INFO
        }
    };

    // With meta only (uses CRATE_PATH from env if set)
    (meta = $meta:expr $(,)?) => {
        #[doc(hidden)]
        #[allow(dead_code)]
        static __AT_CRATE_INFO: $crate::AtCrateInfo = $crate::AtCrateInfo::builder()
            .name(env!("CARGO_PKG_NAME"))
            .repo(option_env!("CARGO_PKG_REPOSITORY"))
            .commit($crate::__whereat_detect_commit!())
            .path(option_env!("CRATE_PATH"))
            .module(module_path!())
            .meta($meta)
            .build();

        #[doc(hidden)]
        #[allow(dead_code)]
        pub(crate) fn at_crate_info() -> &'static $crate::AtCrateInfo {
            &__AT_CRATE_INFO
        }
    };

    // With path and meta
    (path = $path:literal, meta = $meta:expr $(,)?) => {
        #[doc(hidden)]
        #[allow(dead_code)]
        static __AT_CRATE_INFO: $crate::AtCrateInfo = $crate::AtCrateInfo::builder()
            .name(env!("CARGO_PKG_NAME"))
            .repo(option_env!("CARGO_PKG_REPOSITORY"))
            .commit($crate::__whereat_detect_commit!())
            .path(Some($path))
            .module(module_path!())
            .meta($meta)
            .build();

        #[doc(hidden)]
        #[allow(dead_code)]
        pub(crate) fn at_crate_info() -> &'static $crate::AtCrateInfo {
            &__AT_CRATE_INFO
        }
    };

    // With meta and path (reversed order)
    (meta = $meta:expr, path = $path:literal $(,)?) => {
        $crate::define_at_crate_info!(path = $path, meta = $meta);
    };
}

/// Start tracing an error with crate metadata for repository links.
///
/// Requires `define_at_crate_info!()` or a custom `at_crate_info()` function.
///
/// ## Setup (once in lib.rs)
///
/// ```rust,ignore
/// whereat::define_at_crate_info!();
/// ```
///
/// ## Usage
///
/// ```rust,ignore
/// use whereat::{at, At};
///
/// fn find_user(id: u64) -> Result<String, At<MyError>> {
///     if id == 0 {
///         return Err(at!(MyError::NotFound));
///     }
///     Ok(format!("User {}", id))
/// }
/// ```
///
/// ## Without Crate Info
///
/// If you don't need GitHub links, use the `at()` function instead:
///
/// ```rust
/// use whereat::{at, At};
///
/// #[derive(Debug)]
/// struct MyError;
///
/// let err: At<MyError> = at(MyError);  // No crate info, no getter needed
/// ```
#[macro_export]
#[allow(clippy::crate_in_macro_def)] // Intentional: calls caller's crate getter
macro_rules! at {
    ($err:expr) => {{
        $crate::At::wrap($err)
            .set_crate_info(crate::at_crate_info())
            .at()
    }};
}

/// Add crate boundary marker to a Result with an `At<E>` error.
///
/// Requires `define_at_crate_info!()` or a custom `at_crate_info()` function.
/// Use at crate boundaries when consuming errors from dependencies.
///
/// ## Setup (once in lib.rs)
///
/// ```rust,ignore
/// whereat::define_at_crate_info!();
/// ```
///
/// ## Usage
///
/// ```rust,ignore
/// use whereat::{at_crate, At, ResultAtExt};
///
/// fn my_function() -> Result<(), At<DepError>> {
///     at_crate!(dependency::call())?;  // Mark crate boundary
///     Ok(())
/// }
/// ```
#[macro_export]
#[allow(clippy::crate_in_macro_def)] // Intentional: calls caller's crate getter
macro_rules! at_crate {
    ($result:expr) => {{ $crate::ResultAtExt::at_crate($result, crate::at_crate_info()) }};
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
/// use whereat::{at, At};
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
    At::wrap(err).at()
}

// Extension traits are in ext.rs

#[cfg(test)]
mod tests;
