//! Integration tests for true cross-crate error tracing.
//!
//! These tests use the `fake-dep` crate which has its own `define_at_crate_info!()`
//! with a different repository URL, verifying that `at_crate!()` properly tracks
//! crate boundary crossings.

use fake_dep::{FakeDepError, deep_operation, fetch_data, fetch_with_context};
use whereat::{At, ResultAtExt, at_crate};

// This crate's own info (whereat test crate)
whereat::define_at_crate_info!();

// ============================================================================
// Basic Cross-Crate Boundary Tests
// ============================================================================

#[test]
fn foreign_error_has_foreign_crate_info() {
    let err = fetch_data("test-key").unwrap_err();

    let info = err.crate_info().expect("should have crate info");
    assert_eq!(
        info.name(),
        "fake-dep",
        "Error should originate from fake-dep"
    );
}

#[test]
fn at_crate_marks_boundary_crossing() {
    fn my_function() -> Result<String, At<FakeDepError>> {
        at_crate!(fetch_data("user-123"))?;
        Ok(String::new())
    }

    let err = my_function().unwrap_err();

    // The error originated in fake-dep (crate_info from at!())
    let origin_info = err.crate_info().expect("should have origin crate info");
    assert_eq!(origin_info.name(), "fake-dep");

    // at_crate!() should have added a boundary context
    let boundary_contexts: Vec<_> = err
        .contexts()
        .filter_map(|ctx| ctx.as_crate_info())
        .collect();

    assert!(
        !boundary_contexts.is_empty(),
        "at_crate!() should add boundary context. Got {} contexts total.",
        err.contexts().count()
    );

    // The boundary should be for THIS crate (cross_crate test crate)
    // Note: in test context, the crate name is "whereat" since tests run as part of whereat
    let boundary = boundary_contexts.last().expect("should have boundary");
    assert_eq!(
        boundary.name(),
        "whereat",
        "Boundary should be from consuming crate"
    );
}

#[test]
fn boundary_context_has_different_repo_than_origin() {
    fn wrapper() -> Result<String, At<FakeDepError>> {
        at_crate!(fetch_data("key"))?;
        Ok(String::new())
    }

    let err = wrapper().unwrap_err();

    let origin_info = err.crate_info().expect("should have origin info");
    let origin_repo = origin_info.repo().unwrap_or("");

    let boundary_contexts: Vec<_> = err
        .contexts()
        .filter_map(|ctx| ctx.as_crate_info())
        .collect();

    if let Some(boundary) = boundary_contexts.last() {
        let boundary_repo = boundary.repo().unwrap_or("");

        // Repos should be different (fake-dep vs whereat)
        assert_ne!(
            origin_repo, boundary_repo,
            "Origin repo ({}) should differ from boundary repo ({})",
            origin_repo, boundary_repo
        );
    }
}

// ============================================================================
// Multiple Boundary Crossings
// ============================================================================

#[test]
fn nested_at_crate_calls_accumulate_boundaries() {
    fn outer() -> Result<String, At<FakeDepError>> {
        at_crate!(inner())?;
        Ok(String::new())
    }

    fn inner() -> Result<String, At<FakeDepError>> {
        at_crate!(fetch_data("nested"))?;
        Ok(String::new())
    }

    let err = outer().unwrap_err();

    let boundary_count = err
        .contexts()
        .filter(|ctx| ctx.as_crate_info().is_some())
        .count();

    assert!(
        boundary_count >= 2,
        "Should have at least 2 boundary contexts (inner + outer), got {}",
        boundary_count
    );
}

#[test]
fn deep_foreign_error_preserves_foreign_frames() {
    fn my_caller() -> Result<(), At<FakeDepError>> {
        at_crate!(deep_operation())?;
        Ok(())
    }

    let err = my_caller().unwrap_err();

    // deep_operation() creates multiple frames within fake-dep
    // plus our at_crate!() adds a boundary
    assert!(
        err.frame_count() >= 2,
        "Should preserve foreign frames. Got {} frames",
        err.frame_count()
    );

    // Origin should still be fake-dep
    let info = err.crate_info().expect("should have crate info");
    assert_eq!(info.name(), "fake-dep");
}

// ============================================================================
// Context Preservation Across Boundaries
// ============================================================================

#[test]
fn foreign_context_preserved_after_boundary() {
    fn wrapper() -> Result<String, At<FakeDepError>> {
        // fetch_with_context adds "fetching from remote" context in fake-dep
        at_crate!(fetch_with_context("key"))?;
        Ok(String::new())
    }

    let err = wrapper().unwrap_err();
    let debug_output = format!("{:?}", err);

    // The context from fake-dep should be preserved
    assert!(
        debug_output.contains("fetching from remote"),
        "Foreign context should be preserved. Got:\n{}",
        debug_output
    );
}

#[test]
fn can_add_context_after_boundary() {
    fn wrapper() -> Result<String, At<FakeDepError>> {
        at_crate!(fetch_data("key")).at_str("in my wrapper")?;
        Ok(String::new())
    }

    let err = wrapper().unwrap_err();
    let debug_output = format!("{:?}", err);

    assert!(
        debug_output.contains("in my wrapper"),
        "Should be able to add context after at_crate!(). Got:\n{}",
        debug_output
    );
}

// ============================================================================
// Display/Debug Output Verification
// ============================================================================

#[test]
fn debug_output_shows_crate_boundaries() {
    fn wrapper() -> Result<String, At<FakeDepError>> {
        at_crate!(fetch_data("key"))?;
        Ok(String::new())
    }

    let err = wrapper().unwrap_err();
    let debug_output = format!("{:?}", err);

    // Should show locations from both crates
    assert!(
        debug_output.contains(".rs:"),
        "Debug should show file locations. Got:\n{}",
        debug_output
    );
}

#[test]
fn display_with_meta_shows_github_links() {
    fn wrapper() -> Result<String, At<FakeDepError>> {
        at_crate!(fetch_data("key"))?;
        Ok(String::new())
    }

    let err = wrapper().unwrap_err();
    let meta_output = format!("{}", err.display_with_meta());

    // Should contain repository information
    // fake-dep's repo is "https://github.com/example/fake-dep"
    assert!(
        meta_output.contains("fake-dep") || meta_output.contains("github"),
        "display_with_meta should show crate/repo info. Got:\n{}",
        meta_output
    );
}

// ============================================================================
// Crate Info API Tests
// ============================================================================

#[test]
fn fake_dep_crate_info_is_correct() {
    let info = fake_dep::crate_info();

    assert_eq!(info.name(), "fake-dep");
    assert_eq!(info.repo(), Some("https://github.com/example/fake-dep"));
}

#[test]
fn our_crate_info_differs_from_fake_dep() {
    let our_info = crate::at_crate_info();
    let their_info = fake_dep::crate_info();

    assert_ne!(
        our_info.name(),
        their_info.name(),
        "Crate names should differ"
    );

    assert_ne!(our_info.repo(), their_info.repo(), "Repos should differ");
}

// ============================================================================
// Error Type Conversion Across Boundaries
// ============================================================================

#[derive(Debug)]
#[allow(dead_code)]
enum MyAppError {
    Upstream(FakeDepError),
    Other(String),
}

impl From<FakeDepError> for MyAppError {
    fn from(e: FakeDepError) -> Self {
        MyAppError::Upstream(e)
    }
}

#[test]
fn map_error_preserves_trace_across_boundary() {
    fn wrapper() -> Result<String, At<MyAppError>> {
        at_crate!(fetch_data("key")).map_err(|e| e.map_error(MyAppError::from))?;
        Ok(String::new())
    }

    let err = wrapper().unwrap_err();

    // Should still have frames from the original error
    assert!(
        err.frame_count() >= 1,
        "Trace should be preserved after map_error. Got {} frames",
        err.frame_count()
    );

    // Error type should be MyAppError now
    match err.error() {
        MyAppError::Upstream(FakeDepError::NotFound { key }) => {
            assert_eq!(key, "key");
        }
        other => panic!("Expected Upstream(NotFound), got {:?}", other),
    }
}
