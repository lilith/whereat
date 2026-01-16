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
//! ## Adding AtContext
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
//! Use `.start_at()` on Results with non-traced errors:
//!
//! ```rust
//! use errat::{At, ResultStartAtExt, ResultAtExt};
//!
//! fn external_api() -> Result<(), &'static str> {
//!     Err("external error")
//! }
//!
//! fn wrapper() -> Result<(), At<&'static str>> {
//!     external_api().start_at()?;  // converts to At
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
mod trace;

pub use at::At;
pub use context::{AtContextRef, AtDebugAny, AtDisplayAny};
pub use crate_info::{AtCrateInfo, AtCrateInfoBuilder};
pub use ext::{ErrorAtExt, ResultAtExt, ResultAtTraceableExt, ResultStartAtExt};
pub use trace::{AtTrace, AtTraceable};

// ============================================================================
// Crate-level error tracking info (for errat's own at!() / at_crate!() usage)
// ============================================================================
//
// This is what `define_at_crate_info!()` generates. We define it manually here
// because the macro isn't defined yet at this point in the file.

// errat's own crate info for internal at!() usage in doctests
#[doc(hidden)]
pub(crate) static __AT_CRATE_INFO: AtCrateInfo = AtCrateInfo::builder()
    .name("errat")
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
    .module("errat")
    .build();

#[doc(hidden)]
pub fn at_crate_info() -> &'static AtCrateInfo {
    &__AT_CRATE_INFO
}

/// Internal macro for commit detection chain.
#[doc(hidden)]
#[macro_export]
macro_rules! __errat_detect_commit {
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
/// errat::define_at_crate_info!();
/// ```
///
/// ## With Options
///
/// ```rust,ignore
/// errat::define_at_crate_info!(
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
/// use errat::AtCrateInfo;
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
            .commit($crate::__errat_detect_commit!())
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
            .commit($crate::__errat_detect_commit!())
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
            .commit($crate::__errat_detect_commit!())
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
            .commit($crate::__errat_detect_commit!())
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
/// errat::define_at_crate_info!();
/// ```
///
/// ## Usage
///
/// ```rust,ignore
/// use errat::{at, At};
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
/// use errat::{at, At};
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
        $crate::At::new($err)
            .set_crate_info(crate::at_crate_info())
            .at()
    }};
}

/// Add crate boundary marker to a Result with an At<E> error.
///
/// Requires `define_at_crate_info!()` or a custom `at_crate_info()` function.
/// Use at crate boundaries when consuming errors from dependencies.
///
/// ## Setup (once in lib.rs)
///
/// ```rust,ignore
/// errat::define_at_crate_info!();
/// ```
///
/// ## Usage
///
/// ```rust,ignore
/// use errat::{at_crate, At, ResultAtExt};
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

// Extension traits are in ext.rs

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::AtContext;
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::fmt;

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
        assert_eq!(size_of::<Option<Box<AtTrace>>>(), 8);

        let traced_size = size_of::<At<TestError>>();
        let error_size = size_of::<TestError>();
        let pointer_size = size_of::<Option<Box<AtTrace>>>();

        // Should be error + pointer, with possible padding
        assert!(traced_size <= error_size + pointer_size + 8); // Allow for alignment
        assert!(traced_size >= error_size + pointer_size);

        // For a 1-byte enum, should be 16 bytes (1 + 7 padding + 8 pointer)
        assert_eq!(traced_size, 16);
    }

    #[test]
    fn test_sizeof_trace() {
        use core::mem::size_of;

        let trace_size = size_of::<AtTrace>();

        // AtTrace size depends on feature flags:
        // - Without tinyvec: 40 bytes (locations Vec 24 + crate_info 8 + contexts Option<Box> 8)
        // - tinyvec-64-bytes: 64 bytes (TinyVec<4 slots> 48 + crate_info 8 + contexts 8)
        // - tinyvec-128-bytes: 128 bytes (TinyVec<12 slots> 112 + crate_info 8 + contexts 8)
        // - tinyvec-256-bytes: 256 bytes (TinyVec<28 slots> 240 + crate_info 8 + contexts 8)

        #[cfg(not(any(
            feature = "tinyvec-64-bytes",
            feature = "tinyvec-128-bytes",
            feature = "tinyvec-256-bytes"
        )))]
        assert_eq!(trace_size, 40, "AtTrace should be 40 bytes without tinyvec");

        #[cfg(all(
            feature = "tinyvec-64-bytes",
            not(any(feature = "tinyvec-128-bytes", feature = "tinyvec-256-bytes"))
        ))]
        assert_eq!(
            trace_size, 64,
            "AtTrace with tinyvec-64-bytes should be exactly 64 bytes"
        );

        #[cfg(all(feature = "tinyvec-128-bytes", not(feature = "tinyvec-256-bytes")))]
        assert_eq!(
            trace_size, 128,
            "AtTrace with tinyvec-128-bytes should be exactly 128 bytes"
        );

        #[cfg(feature = "tinyvec-256-bytes")]
        assert_eq!(
            trace_size, 256,
            "AtTrace with tinyvec-256-bytes should be exactly 256 bytes"
        );
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
            fallible().start_at()?;
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
        // Use contexts() to find text context
        let text = err.contexts().find_map(|c| c.as_text());
        assert_eq!(text, Some("while fetching user"));
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
        let text = err.contexts().find_map(|c| c.as_text());
        assert_eq!(text, Some("during initialization"));
    }

    #[test]
    fn test_start_at_with_context() {
        fn fallible() -> Result<(), &'static str> {
            Err("oops")
        }

        fn wrapper() -> Result<(), At<&'static str>> {
            fallible().start_at().at_str("while doing something")?;
            Ok(())
        }

        let err = wrapper().unwrap_err();
        assert_eq!(*err.error(), "oops");
        let text = err.contexts().find_map(|c| c.as_text());
        assert_eq!(text, Some("while doing something"));
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
        let text_ctx = AtContext::Text(String::from("hello").into());
        assert_eq!(text_ctx.as_text(), Some("hello"));
        assert!(text_ctx.downcast_ref::<u32>().is_none());

        // Debug context - requires Debug (u32 implements Debug)
        let debug_ctx = AtContext::Debug(Box::new(42u32));
        assert_eq!(debug_ctx.as_text(), None);
        assert_eq!(debug_ctx.downcast_ref::<u32>(), Some(&42));

        // Verify Debug output works
        let debug_str = alloc::format!("{:?}", debug_ctx);
        assert!(debug_str.contains("42"));

        // Display context - requires Display (u32 implements Display)
        let display_ctx = AtContext::Display(Box::new(99u32));
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
