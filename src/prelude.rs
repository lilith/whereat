//! Convenient re-exports for common usage.
//!
//! This prelude includes the most commonly used types and traits for error tracing.
//!
//! ## Usage
//!
//! ```rust
//! use whereat::prelude::*;
//!
//! #[derive(Debug)]
//! struct MyError;
//!
//! fn inner() -> Result<(), At<MyError>> {
//!     Err(at(MyError))
//! }
//!
//! fn outer() -> Result<(), At<MyError>> {
//!     inner().at()?;
//!     Ok(())
//! }
//! ```

pub use crate::At;
pub use crate::ResultAtExt;
pub use crate::at;
