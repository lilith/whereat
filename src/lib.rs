//! # whereat - Lightweight error location tracking
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
//! use whereat::{at, At, ResultAtExt};
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
//! assert_eq!(err.frame_count(), 2);
//! ```
//!
//! ## Adding AtContext
//!
//! Use `.at_str()` for static strings, `.at_string()` for lazy strings, `.at_data()` for Display, `.at_debug()` for Debug:
//!
//! ```rust
//! use whereat::{at, At, ResultAtExt};
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
//! use whereat::{at, At, ResultAtExt};
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
//! Use `map_err(at)` on Results with non-traced errors:
//!
//! ```rust
//! use whereat::{At, at, ResultAtExt};
//!
//! fn external_api() -> Result<(), &'static str> {
//!     Err("external error")
//! }
//!
//! fn wrapper() -> Result<(), At<&'static str>> {
//!     external_api().map_err(at)?;  // converts to At
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

mod at;
mod context;
mod crate_info;
mod ext;
pub mod prelude;
mod trace;

pub use at::At;
pub use context::AtContextRef;
pub use crate_info::{
    AtCrateInfo, AtCrateInfoBuilder, BITBUCKET_LINK_FORMAT, GITEA_LINK_FORMAT, GITHUB_LINK_FORMAT,
    GITLAB_LINK_FORMAT,
};
pub use ext::{ErrorAtExt, ResultAtExt, ResultAtTraceableExt};
pub use trace::{AtFrame, AtFrameOwned, AtTrace, AtTraceBoxed, AtTraceable};

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
