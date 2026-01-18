//! A fake dependency crate for testing cross-crate error tracing.
//!
//! This crate has its own `define_at_crate_info!()` with a different
//! repository URL, allowing tests to verify that `at_crate!()` properly
//! switches context when crossing crate boundaries.

use whereat::{At, ResultAtExt};

// This crate's own crate info - different repo from whereat
whereat::define_at_crate_info!();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeDepError {
    NotFound { key: String },
    ConnectionFailed,
    Timeout,
}

/// Returns an error originating from this crate.
/// The trace will point to fake-dep's repository.
#[track_caller]
pub fn fetch_data(key: &str) -> Result<String, At<FakeDepError>> {
    Err(whereat::at!(FakeDepError::NotFound {
        key: key.to_string()
    }))
}

/// Returns an error with additional context.
#[track_caller]
pub fn fetch_with_context(key: &str) -> Result<String, At<FakeDepError>> {
    fetch_data(key).at_str("fetching from remote")?;
    Ok(String::new())
}

/// Simulates a deeper call stack within this crate.
#[track_caller]
pub fn deep_operation() -> Result<(), At<FakeDepError>> {
    level_one()?;
    Ok(())
}

#[track_caller]
fn level_one() -> Result<(), At<FakeDepError>> {
    level_two().at().at_str("in level_one")?; // .at() creates new frame
    Ok(())
}

#[track_caller]
fn level_two() -> Result<(), At<FakeDepError>> {
    Err(whereat::at!(FakeDepError::ConnectionFailed))
}

/// Get this crate's info for testing.
pub fn crate_info() -> &'static whereat::AtCrateInfo {
    at_crate_info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_has_crate_info() {
        let err = fetch_data("test").unwrap_err();
        let info = err.crate_info().expect("should have crate info");
        assert_eq!(info.name(), "fake-dep");
    }

    #[test]
    fn deep_error_has_multiple_frames() {
        let err = deep_operation().unwrap_err();
        assert!(
            err.frame_count() >= 2,
            "Should have multiple frames, got {}",
            err.frame_count()
        );
    }
}
