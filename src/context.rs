//! Context data attached to error trace locations.
//!
//! This module provides [`AtContextRef`] for inspecting context data, and the
//! internal [`AtContext`] enum used for storage.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use core::fmt;

use crate::AtCrateInfo;

// ============================================================================
// AtDebugAny Trait - combines Any + Debug in a single trait object
// ============================================================================

/// Trait combining `Any` and `Debug` for type-erased context data.
///
/// This allows storing arbitrary typed data while still being able to:
/// - Debug-print it
/// - Downcast it back to the original type
pub trait AtDebugAny: core::any::Any + fmt::Debug + Send + Sync {
    /// Get a reference to self as `&dyn Any` for downcasting.
    fn as_any(&self) -> &dyn core::any::Any;

    /// Get the type name for diagnostics.
    fn type_name(&self) -> &'static str;
}

impl<T: core::any::Any + fmt::Debug + Send + Sync> AtDebugAny for T {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn type_name(&self) -> &'static str {
        core::any::type_name::<T>()
    }
}

// ============================================================================
// AtDisplayAny Trait - combines Any + Display in a single trait object
// ============================================================================

/// Trait combining `Any` and `Display` for type-erased context data.
///
/// Similar to `AtDebugAny` but for types that implement `Display`.
/// Use this when you want human-readable output instead of debug format.
pub trait AtDisplayAny: core::any::Any + fmt::Display + Send + Sync {
    /// Get a reference to self as `&dyn Any` for downcasting.
    fn as_any(&self) -> &dyn core::any::Any;

    /// Get the type name for diagnostics.
    fn type_name(&self) -> &'static str;
}

impl<T: core::any::Any + fmt::Display + Send + Sync> AtDisplayAny for T {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn type_name(&self) -> &'static str {
        core::any::type_name::<T>()
    }
}

// ============================================================================
// AtContext Enum (internal)
// ============================================================================

/// Internal context data attached to a trace segment.
///
/// This enum is not publicly exposed. Use [`AtContextRef`] to access context data.
#[non_exhaustive]
pub(crate) enum AtContext {
    /// A text message describing what operation was being performed.
    /// Uses `Cow<'static, str>` for zero-copy static strings.
    Text(Cow<'static, str>),
    /// Typed context data formatted via Debug.
    Debug(Box<dyn AtDebugAny>),
    /// Typed context data formatted via Display.
    Display(Box<dyn AtDisplayAny>),
    /// Crate boundary marker - changes the assumed crate for subsequent locations.
    /// Used for generating correct repository links in cross-crate traces.
    Crate(&'static AtCrateInfo),
    /// Marker indicating that some frames were skipped.
    /// Used when starting tracing late or skipping intermediate frames.
    /// Displayed as `[...]` in trace output.
    Skipped,
}

impl AtContext {
    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            AtContext::Text(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn as_crate_info(&self) -> Option<&'static AtCrateInfo> {
        match self {
            AtContext::Crate(info) => Some(info),
            _ => None,
        }
    }

    pub(crate) fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        match self {
            AtContext::Text(_) | AtContext::Crate(_) | AtContext::Skipped => None,
            // Must use (**b) to call as_any on the trait object, not the Box
            // (Box<dyn AtDebugAny> itself implements AtDebugAny through the blanket impl)
            AtContext::Debug(b) => (**b).as_any().downcast_ref(),
            AtContext::Display(b) => (**b).as_any().downcast_ref(),
        }
    }

    pub(crate) fn type_name(&self) -> Option<&'static str> {
        match self {
            AtContext::Text(_) | AtContext::Crate(_) | AtContext::Skipped => None,
            AtContext::Debug(b) => Some((**b).type_name()),
            AtContext::Display(b) => Some((**b).type_name()),
        }
    }

    pub(crate) fn is_display(&self) -> bool {
        matches!(self, AtContext::Text(_) | AtContext::Display(_))
    }

    pub(crate) fn is_crate_boundary(&self) -> bool {
        matches!(self, AtContext::Crate(_))
    }

    pub(crate) fn is_skipped(&self) -> bool {
        matches!(self, AtContext::Skipped)
    }
}

impl fmt::Debug for AtContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtContext::Text(s) => write!(f, "{:?}", s),
            AtContext::Debug(t) => write!(f, "{:?}", &**t),
            AtContext::Display(t) => write!(f, "{}", &**t), // Display types use Display even in Debug
            AtContext::Crate(info) => write!(f, "[crate: {}]", info.name()),
            AtContext::Skipped => write!(f, "[...]"),
        }
    }
}

impl fmt::Display for AtContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtContext::Text(s) => write!(f, "{}", s),
            AtContext::Debug(t) => write!(f, "{:?}", &**t), // Debug types use Debug in Display
            AtContext::Display(t) => write!(f, "{}", &**t),
            AtContext::Crate(info) => write!(f, "[crate: {}]", info.name()),
            AtContext::Skipped => write!(f, "[...]"),
        }
    }
}

// ============================================================================
// AtContextRef - Public wrapper for context access
// ============================================================================

/// A reference to context data attached to a trace location.
///
/// This type provides read-only access to context without exposing internal details.
/// Obtained by iterating over [`crate::At::contexts()`].
///
/// ## Example
///
/// ```rust
/// use errat::{at, At};
///
/// #[derive(Debug)]
/// enum MyError { NotFound }
///
/// let err = at(MyError::NotFound).at_str("while loading");
///
/// for ctx in err.contexts() {
///     if let Some(text) = ctx.as_text() {
///         println!("Context: {}", text);
///     }
/// }
/// ```
#[derive(Clone, Copy)]
pub struct AtContextRef<'a> {
    pub(crate) inner: &'a AtContext,
}

impl<'a> AtContextRef<'a> {
    /// Get as text, if this is a text context (from `at_str` or `at_string`).
    #[inline]
    pub fn as_text(&self) -> Option<&'a str> {
        self.inner.as_text()
    }

    /// Get as crate info, if this is a crate boundary marker.
    #[inline]
    pub fn as_crate_info(&self) -> Option<&'static AtCrateInfo> {
        self.inner.as_crate_info()
    }

    /// Try to downcast to a specific type, if this is typed context.
    ///
    /// Works with contexts added via `at_data()` or `at_debug()`.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::at;
    ///
    /// #[derive(Debug)]
    /// struct RequestInfo { user_id: u64 }
    ///
    /// #[derive(Debug)]
    /// enum MyError { Forbidden }
    ///
    /// let err = at(MyError::Forbidden)
    ///     .at_debug(|| RequestInfo { user_id: 42 });
    ///
    /// for ctx in err.contexts() {
    ///     if let Some(req) = ctx.downcast_ref::<RequestInfo>() {
    ///         assert_eq!(req.user_id, 42);
    ///     }
    /// }
    /// ```
    #[inline]
    pub fn downcast_ref<T: 'static>(&self) -> Option<&'a T> {
        self.inner.downcast_ref()
    }

    /// Get the type name if this is typed context.
    #[inline]
    pub fn type_name(&self) -> Option<&'static str> {
        self.inner.type_name()
    }

    /// Check if this context uses Display formatting.
    ///
    /// Returns `true` for text contexts and `at_data()` contexts.
    #[inline]
    pub fn is_display(&self) -> bool {
        self.inner.is_display()
    }

    /// Check if this is a crate boundary marker.
    #[inline]
    pub fn is_crate_boundary(&self) -> bool {
        self.inner.is_crate_boundary()
    }

    /// Check if this is a skip marker (`[...]`).
    #[inline]
    pub fn is_skipped(&self) -> bool {
        self.inner.is_skipped()
    }
}

impl fmt::Debug for AtContextRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.inner, f)
    }
}

impl fmt::Display for AtContextRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.inner, f)
    }
}
