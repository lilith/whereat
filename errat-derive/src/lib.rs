//! Derive macros for errat error tracing.
//!
//! This crate provides `#[derive(TracedError)]` for automatic error type setup.

use proc_macro::TokenStream;

/// Derive macro for creating traced error types.
///
/// ## Example
///
/// ```ignore
/// use errat::TracedError;
///
/// #[derive(TracedError)]
/// #[errat(repo = "https://github.com/user/repo")]
/// enum MyError {
///     #[error("not found: {0}")]
///     NotFound(String),
///
///     #[error("io error")]
///     #[from]
///     Io(std::io::Error),
/// }
/// ```
#[proc_macro_derive(TracedError, attributes(errat, error, from))]
pub fn derive_traced_error(_input: TokenStream) -> TokenStream {
    // TODO: Implement derive macro
    TokenStream::new()
}
