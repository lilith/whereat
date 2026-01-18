//! # whereat - Lightweight error location tracking
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
//! ## Try It Now
//!
//! No setup required — just wrap errors with [`at()`] and propagate with [`.at()`](ResultAtExt::at):
//!
//! ```rust
//! use whereat::{at, At, ResultAtExt};
//!
//! #[derive(Debug)]
//! enum MyError { NotFound }
//!
//! fn inner() -> Result<(), At<MyError>> {
//!     Err(at(MyError::NotFound))  // Wrap error, capture location
//! }
//!
//! fn outer() -> Result<(), At<MyError>> {
//!     inner().at_str("looking up user")?;  // Add context
//!     Ok(())
//! }
//!
//! let err = outer().unwrap_err();
//! println!("{:?}", err);  // Shows locations + context
//! ```
//!
//! ## Production Setup
//!
//! For **clickable GitHub links** in traces, add one line to your crate root and use [`at!()`](at!):
//!
//! ```rust,ignore
//! // In lib.rs or main.rs
//! whereat::define_at_crate_info!();
//!
//! fn find_user(id: u64) -> Result<String, At<MyError>> {
//!     Err(at!(MyError::NotFound))  // Now includes repo links in traces
//! }
//! ```
//!
//! The `at!()` macro desugars to:
//! ```rust,ignore
//! At::wrap(err)
//!     .set_crate_info(crate::at_crate_info())  // Enables GitHub/GitLab links
//!     .at()                                     // Captures file:line:col
//! ```
//!
//! ## Which Approach?
//!
//! | Situation | Use |
//! |-----------|-----|
//! | Existing struct/enum you don't want to modify | Wrap with [`At<YourError>`](At) |
//! | Want traces embedded inside your error type | Implement [`AtTraceable`] trait |
//!
//! **Wrapper approach** (most common): Return `Result<T, At<YourError>>` from functions.
//!
//! **Embedded approach**: Implement [`AtTraceable`] and store an [`AtTrace`] (or `Box<AtTrace>`)
//! field inside your error type. Return `Result<T, YourError>` directly.
//!
//! ## Starting a Trace
//!
//! | Function | Crate info | Use when |
//! |----------|------------|----------|
//! | [`at(err)`](at()) | ❌ None | Prototyping — no setup needed |
//! | [`at!(err)`](at!) | ✅ GitHub links | **Production** — requires [`define_at_crate_info!()`](define_at_crate_info) |
//! | [`err.start_at()`](ErrorAtExt::start_at) | ❌ None | Chaining on `Error` trait types |
//!
//! Start with `at()` to try things out. Upgrade to `at!()` before shipping — you'll want
//! those clickable links when debugging production issues.
//!
//! ## Extending a Trace
//!
//! **Create a new location frame** (call site is recorded):
//!
//! | Method | Effect |
//! |--------|--------|
//! | [`.at()`](ResultAtExt::at) | New frame with just file:line:col |
//! | [`.at_fn(\|\| {})`](ResultAtExt::at_fn) | New frame + captures function name |
//! | [`.at_named("step")`](ResultAtExt::at_named) | New frame + custom label |
//!
//! **Add context to the last frame** (no new location):
//!
//! | Method | Effect |
//! |--------|--------|
//! | [`.at_str("msg")`](ResultAtExt::at_str) | Static string (zero-cost) |
//! | [`.at_string(\|\| format!(...))`](ResultAtExt::at_string) | Dynamic string (lazy) |
//! | [`.at_data(\|\| value)`](ResultAtExt::at_data) | Typed via Display (lazy) |
//! | [`.at_debug(\|\| value)`](ResultAtExt::at_debug) | Typed via Debug (lazy) |
//! | [`.at_error(source_err)`](ResultAtExt::at_error) | Attach a source error |
//!
//! **Key distinction**: `.at()` creates a NEW frame. `.at_str()` and friends add to the LAST frame.
//!
//! ```rust
//! use whereat::{at, At, ResultAtExt};
//!
//! #[derive(Debug)]
//! struct MyError;
//!
//! fn example() -> Result<(), At<MyError>> {
//!     // One frame with two contexts attached
//!     let e = at(MyError).at_str("a").at_str("b");
//!     assert_eq!(e.frame_count(), 1);
//!
//!     // Two frames: at() creates first, .at() creates second
//!     let e = at(MyError).at().at_str("on second frame");
//!     assert_eq!(e.frame_count(), 2);
//!     Ok(())
//! }
//! # example().ok();
//! ```
//!
//! ## Foreign Crates and Errors
//!
//! When consuming errors from other crates, use [`at_crate!()`](at_crate) to mark the boundary.
//! This ensures traces show your crate's GitHub links, not confusing paths from dependencies.
//!
//! ```rust,ignore
//! whereat::define_at_crate_info!();  // Once in lib.rs
//!
//! use whereat::{at_crate, At, ResultAtExt};
//!
//! fn call_dependency() -> Result<(), At<DependencyError>> {
//!     at_crate!(dependency::do_thing())?;  // Marks crate boundary
//!     Ok(())
//! }
//! ```
//!
//! The `at_crate!()` macro desugars to:
//! ```rust,ignore
//! result.at_crate(crate::at_crate_info())  // Adds boundary marker with your crate's info
//! ```
//!
//! For plain errors without traces (e.g., `std::io::Error`), use `map_err(at)` to start tracing:
//!
//! ```rust
//! use whereat::{At, at, ResultAtExt};
//!
//! fn external_api() -> Result<(), &'static str> {
//!     Err("external error")
//! }
//!
//! fn wrapper() -> Result<(), At<&'static str>> {
//!     external_api().map_err(at).at_str("calling external API")?;
//!     Ok(())
//! }
//! ```
//!
//! ## Design Goals
//!
//! - **Tiny overhead**: `At<E>` is `sizeof(E) + 8` bytes; zero heap allocation on the Ok path
//! - **Zero-cost context**: `.at_str("literal")` stores a pointer, no copy or allocation
//! - **Lazy evaluation**: `.at_string(|| ...)` closures only run on error
//! - **no_std compatible**: Works with just `core` + `alloc`
//!
//! ## OOM Behavior
//!
//! Trace allocations are fallible where possible — on OOM, trace entries are silently skipped
//! but your error `E` always propagates (it's stored inline). See the README for details.

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
