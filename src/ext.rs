//! Extension traits for ergonomic error tracing on Results.
//!
//! This module provides extension traits for calling `.at()` and other tracing
//! methods directly on `Result` types, avoiding verbose `map_err` boilerplate.
//!
//! - [`ErrorAtExt`]: Call `.start_at()` on `Error` types to wrap in `At<E>`
//! - [`ResultAtExt`]: Call `.at()` on `Result<T, At<E>>` to extend the trace
//! - [`ResultStartAtExt`]: Call `.start_at()` on `Result<T, E>` to begin tracing
//! - [`ResultAtTraceableExt`]: Call `.at()` on `Result<T, E>` where E: AtTraceable

use alloc::string::String;
use core::fmt;

use crate::AtCrateInfo;
use crate::at::At;
use crate::trace::AtTraceable;

// ============================================================================
// ErrorAtExt Trait - for calling .start_at() directly on error values
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
    /// For crate-aware tracing with repository links, use `at!(err)` instead.
    /// Requires [`define_at_crate_info!()`](crate::define_at_crate_info!) in your crate root.
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

    /// Add static string context to last location (or create one if empty).
    #[track_caller]
    fn at_str(self, msg: &'static str) -> Result<T, At<E>>;

    /// Add lazily-computed string context to last location (or create one if empty).
    #[track_caller]
    fn at_string(self, f: impl FnOnce() -> String) -> Result<T, At<E>>;

    /// Add lazily-computed typed context (Display) to last location (or create one if empty).
    #[track_caller]
    fn at_data<C: fmt::Display + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>>;

    /// Add lazily-computed typed context (Debug) to last location (or create one if empty).
    #[track_caller]
    fn at_debug<C: fmt::Debug + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, At<E>>;

    /// Add an error as context to the last location (or create one if empty).
    #[track_caller]
    fn at_error<Err: core::error::Error + Send + Sync + 'static>(
        self,
        err: Err,
    ) -> Result<T, At<E>>;

    /// Add crate boundary marker to last location (or create one if empty).
    #[track_caller]
    fn at_crate(self, info: &'static AtCrateInfo) -> Result<T, At<E>>;

    /// Add a skip marker to indicate skipped frames.
    fn at_skipped_frames(self) -> Result<T, At<E>>;

    /// Add a location frame with the caller's function name as context.
    ///
    /// Captures both file:line:col AND the function name at zero runtime cost.
    /// Pass an empty closure `|| {}` - its type includes the parent function name.
    #[track_caller]
    fn at_fn<F: Fn()>(self, marker: F) -> Result<T, At<E>>;

    /// Add a location frame with an explicit name as context.
    ///
    /// Like [`at_fn`](Self::at_fn) but with an explicit label.
    #[track_caller]
    fn at_named(self, name: &'static str) -> Result<T, At<E>>;

    /// Convert the error type while preserving the trace.
    ///
    /// This is a convenience method that combines `map_err` with `At::map_error`.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::{at, At, ResultAtExt};
    ///
    /// #[derive(Debug)]
    /// struct InternalError;
    /// #[derive(Debug)]
    /// struct PublicError;
    ///
    /// fn internal() -> Result<(), At<InternalError>> {
    ///     Err(at(InternalError))
    /// }
    ///
    /// fn public() -> Result<(), At<PublicError>> {
    ///     // Clean conversion that preserves trace
    ///     internal().map_err_at(|_| PublicError)?;
    ///     Ok(())
    /// }
    /// ```
    fn map_err_at<E2, F>(self, f: F) -> Result<T, At<E2>>
    where
        F: FnOnce(E) -> E2;
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
    fn at_error<Err: core::error::Error + Send + Sync + 'static>(
        self,
        err: Err,
    ) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_error(err)),
        }
    }

    #[track_caller]
    #[inline]
    fn at_crate(self, info: &'static AtCrateInfo) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_crate(info)),
        }
    }

    #[inline]
    fn at_skipped_frames(self) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_skipped_frames()),
        }
    }

    #[track_caller]
    #[inline]
    fn at_fn<F: Fn()>(self, marker: F) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_fn(marker)),
        }
    }

    #[track_caller]
    #[inline]
    fn at_named(self, name: &'static str) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.at_named(name)),
        }
    }

    #[inline]
    fn map_err_at<E2, F>(self, f: F) -> Result<T, At<E2>>
    where
        F: FnOnce(E) -> E2,
    {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.map_error(f)),
        }
    }
}

// ============================================================================
// ResultStartAtExt - for starting traces on non-At errors
// ============================================================================

/// Extension trait for converting non-traced errors to traced errors.
///
/// Use `.start_at()` on `Result<T, E>` to wrap the error in `At<E>` and capture
/// the first location. For Results that already have `At<E>`, use `ResultAtExt::at()`.
///
/// ## Example
///
/// ```rust
/// use errat::ResultStartAtExt;
///
/// fn fallible() -> Result<(), &'static str> {
///     Err("something went wrong")
/// }
///
/// fn wrapper() -> Result<(), errat::At<&'static str>> {
///     fallible().start_at()?;  // converts to At and captures location
///     Ok(())
/// }
/// ```
pub trait ResultStartAtExt<T, E> {
    /// Convert the error to `At<E>` and add the caller's location.
    ///
    /// Use this to wrap errors from code that doesn't use errat.
    /// Chain with `ResultAtExt` methods for context: `.start_at().at_str("msg")?`
    #[track_caller]
    fn start_at(self) -> Result<T, At<E>>;

    /// Convert to `At<E>` with a skip marker indicating late tracing.
    ///
    /// Use when wrapping errors where earlier frames were not traced.
    /// The `[...]` marker indicates the trace started late.
    #[track_caller]
    fn start_at_late(self) -> Result<T, At<E>>;
}

impl<T, E> ResultStartAtExt<T, E> for Result<T, E> {
    #[track_caller]
    #[inline]
    fn start_at(self) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(At::new(e).at()),
        }
    }

    #[track_caller]
    #[inline]
    fn start_at_late(self) -> Result<T, At<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(At::new(e).at_skipped_frames()),
        }
    }
}

// ============================================================================
// ResultAtTraceableExt - for Results with AtTraceable errors
// ============================================================================

/// Extension trait for `Result<T, E>` where `E` implements [`AtTraceable`].
///
/// Provides the same ergonomics as [`ResultAtExt`] but for custom error types
/// that embed their own trace.
///
/// ## Example
///
/// ```rust
/// use errat::{AtTrace, AtTraceable, ResultAtTraceableExt};
///
/// struct MyError {
///     msg: &'static str,
///     trace: AtTrace,
/// }
///
/// impl AtTraceable for MyError {
///     fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
///     fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
///     fn fmt_message(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
/// fn inner() -> Result<(), MyError> {
///     Err(MyError::new("oops"))
/// }
///
/// fn outer() -> Result<(), MyError> {
///     inner().at_str("in outer")?;  // Works directly on Result!
///     Ok(())
/// }
/// ```
pub trait ResultAtTraceableExt<T, E: AtTraceable> {
    /// Add the caller's location to the error trace if this is `Err`.
    #[track_caller]
    fn at(self) -> Result<T, E>;

    /// Add static string context to last location (or create one if empty).
    #[track_caller]
    fn at_str(self, msg: &'static str) -> Result<T, E>;

    /// Add lazily-computed string context to last location (or create one if empty).
    #[track_caller]
    fn at_string(self, f: impl FnOnce() -> String) -> Result<T, E>;

    /// Add lazily-computed typed context (Display) to last location (or create one if empty).
    #[track_caller]
    fn at_data<C: fmt::Display + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, E>;

    /// Add lazily-computed typed context (Debug) to last location (or create one if empty).
    #[track_caller]
    fn at_debug<C: fmt::Debug + Send + Sync + 'static>(self, f: impl FnOnce() -> C)
    -> Result<T, E>;

    /// Add an error as context to the last location (or create one if empty).
    #[track_caller]
    fn at_error<Err: core::error::Error + Send + Sync + 'static>(self, err: Err) -> Result<T, E>;

    /// Add crate boundary marker to last location (or create one if empty).
    #[track_caller]
    fn at_crate(self, info: &'static AtCrateInfo) -> Result<T, E>;

    /// Add a skip marker to indicate skipped frames.
    fn at_skipped_frames(self) -> Result<T, E>;

    /// Add a location frame with the caller's function name as context.
    ///
    /// Captures both file:line:col AND the function name at zero runtime cost.
    /// Pass an empty closure `|| {}` - its type includes the parent function name.
    #[track_caller]
    fn at_fn<F: Fn()>(self, marker: F) -> Result<T, E>;

    /// Add a location frame with an explicit name as context.
    ///
    /// Like [`at_fn`](Self::at_fn) but with an explicit label.
    #[track_caller]
    fn at_named(self, name: &'static str) -> Result<T, E>;
}

impl<T, E: AtTraceable> ResultAtTraceableExt<T, E> for Result<T, E> {
    #[track_caller]
    #[inline]
    fn at(self) -> Result<T, E> {
        self.map_err(|e| e.at())
    }

    #[track_caller]
    #[inline]
    fn at_str(self, msg: &'static str) -> Result<T, E> {
        self.map_err(|e| e.at_str(msg))
    }

    #[track_caller]
    #[inline]
    fn at_string(self, f: impl FnOnce() -> String) -> Result<T, E> {
        self.map_err(|e| e.at_string(f))
    }

    #[track_caller]
    #[inline]
    fn at_data<C: fmt::Display + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, E> {
        self.map_err(|e| e.at_data(f))
    }

    #[track_caller]
    #[inline]
    fn at_debug<C: fmt::Debug + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> C,
    ) -> Result<T, E> {
        self.map_err(|e| e.at_debug(f))
    }

    #[track_caller]
    #[inline]
    fn at_error<Err: core::error::Error + Send + Sync + 'static>(self, err: Err) -> Result<T, E> {
        self.map_err(|e| e.at_error(err))
    }

    #[track_caller]
    #[inline]
    fn at_crate(self, info: &'static AtCrateInfo) -> Result<T, E> {
        self.map_err(|e| e.at_crate(info))
    }

    #[inline]
    fn at_skipped_frames(self) -> Result<T, E> {
        self.map_err(|e| e.at_skipped_frames())
    }

    #[track_caller]
    #[inline]
    fn at_fn<F: Fn()>(self, marker: F) -> Result<T, E> {
        self.map_err(|e| e.at_fn(marker))
    }

    #[track_caller]
    #[inline]
    fn at_named(self, name: &'static str) -> Result<T, E> {
        self.map_err(|e| e.at_named(name))
    }
}
