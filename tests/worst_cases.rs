//! Worst-case scenarios for errat usage patterns.
//!
//! These tests explore edge cases and problematic patterns to ensure
//! the library behaves correctly under stress and misuse.

use errat::{At, ResultAtExt, ResultStartAtExt, at};

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
enum TestError {
    Failed,
    IoError,
    ParseError,
}

// ============================================================================
// Nested At<At<E>> - Accidental double-wrapping
// ============================================================================

/// User accidentally wraps an already-wrapped error.
/// This is wasteful but should still work correctly.
#[test]
fn nested_at_at_compiles_but_wasteful() {
    fn inner() -> Result<(), At<TestError>> {
        Err(at(TestError::Failed))
    }

    // User forgets inner already returns At<E> and wraps again
    fn outer_bad() -> Result<(), At<At<TestError>>> {
        let result = inner();
        // This compiles but creates At<At<E>> - wasteful!
        Err(at(result.unwrap_err()))
    }

    let err = outer_bad().unwrap_err();
    // The outer At has its own trace
    assert!(err.trace_len() >= 1);
    // The inner At (now the error value) also has a trace
    assert!(err.error().trace_len() >= 1);
}

/// Correct pattern: use .at() to extend existing trace
#[test]
fn correct_pattern_extends_trace() {
    fn inner() -> Result<(), At<TestError>> {
        Err(at(TestError::Failed))
    }

    fn outer_good() -> Result<(), At<TestError>> {
        inner().at() // Extends trace, doesn't wrap
    }

    let err = outer_good().unwrap_err();
    assert_eq!(err.trace_len(), 2); // inner + outer
}

// ============================================================================
// Deep call stacks
// ============================================================================

/// 20-level deep call stack should work without issues
#[test]
fn deep_call_stack_20_levels() {
    fn level(n: u32) -> Result<(), At<TestError>> {
        if n == 0 {
            Err(at(TestError::Failed))
        } else {
            level(n - 1).at()
        }
    }

    let err = level(19).unwrap_err();
    assert_eq!(err.trace_len(), 20);
}

/// 100-level deep call stack (stress test)
#[test]
fn deep_call_stack_100_levels() {
    fn level(n: u32) -> Result<(), At<TestError>> {
        if n == 0 {
            Err(at(TestError::Failed))
        } else {
            level(n - 1).at()
        }
    }

    let err = level(99).unwrap_err();
    assert_eq!(err.trace_len(), 100);
}

// ============================================================================
// Many contexts per location
// ============================================================================

/// Adding many contexts to a single location
#[test]
fn many_contexts_single_location() {
    let err = at(TestError::Failed)
        .at_str("context 1")
        .at_str("context 2")
        .at_str("context 3")
        .at_str("context 4")
        .at_str("context 5")
        .at_str("context 6")
        .at_str("context 7")
        .at_str("context 8")
        .at_str("context 9")
        .at_str("context 10");

    // Only 1 location (from at()), all contexts attach to it
    assert_eq!(err.trace_len(), 1);

    // All 10 contexts should be present
    let contexts: Vec<_> = err.contexts().collect();
    assert_eq!(contexts.len(), 10);
}

/// Mixed context types on single location
#[test]
fn mixed_contexts_single_location() {
    #[allow(dead_code)]
    #[derive(Debug)]
    struct RequestId(u64);

    let err = at(TestError::Failed)
        .at_str("static message")
        .at_string(|| format!("dynamic {}", 42))
        .at_debug(|| RequestId(12345))
        .at_data(|| "display data");

    assert_eq!(err.trace_len(), 1);
    assert_eq!(err.contexts().count(), 4);
}

// ============================================================================
// Hot loops with errors
// ============================================================================

/// Simulates a hot loop where every iteration fails.
/// This is the worst-case for allocation pressure.
#[test]
fn hot_loop_all_errors() {
    fn process_item(_i: usize) -> Result<(), At<TestError>> {
        Err(at(TestError::Failed).at_str("processing"))
    }

    let mut errors = Vec::new();
    for i in 0..1000 {
        if let Err(e) = process_item(i) {
            errors.push(e);
        }
    }

    assert_eq!(errors.len(), 1000);
    // Each error has its own trace
    for err in &errors {
        assert_eq!(err.trace_len(), 1);
    }
}

/// Hot loop with occasional errors (more realistic)
#[test]
fn hot_loop_occasional_errors() {
    fn process_item(i: usize) -> Result<usize, At<TestError>> {
        if i % 100 == 0 {
            Err(at(TestError::Failed))
        } else {
            Ok(i * 2)
        }
    }

    let mut successes = 0;
    let mut failures = 0;

    for i in 0..10000 {
        match process_item(i) {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
    }

    assert_eq!(successes, 9900);
    assert_eq!(failures, 100);
}

// ============================================================================
// Recursive functions
// ============================================================================

/// Recursive descent with tracing at each level
#[test]
fn recursive_descent_traced() {
    fn parse_nested(depth: u32, max: u32) -> Result<u32, At<TestError>> {
        if depth >= max {
            Err(at(TestError::ParseError).at_str("max depth exceeded"))
        } else {
            parse_nested(depth + 1, max).at_str("parsing nested")
        }
    }

    let err = parse_nested(0, 5).unwrap_err();
    // 6 locations: depth 0,1,2,3,4 call .at_str(), depth 5 creates error
    // But at_str adds to last frame, so: 1 from at(), 5 from at_str()
    // Wait - at_str adds to LAST frame, not new frame. So:
    // - depth=5: at() creates 1 location, at_str adds context to it
    // - depth=4: at_str adds context to last (depth 5's location)
    // - etc...
    // All at_str calls add to the same location (the one from at())
    // So we get 1 location with 6 contexts
    assert_eq!(err.trace_len(), 1);
    assert_eq!(err.contexts().count(), 6); // 1 from error + 5 from recursion
}

/// Recursive descent with explicit .at() at each level
#[test]
fn recursive_descent_explicit_at() {
    fn parse_nested(depth: u32, max: u32) -> Result<u32, At<TestError>> {
        if depth >= max {
            Err(at(TestError::ParseError))
        } else {
            parse_nested(depth + 1, max).at()
        }
    }

    let err = parse_nested(0, 5).unwrap_err();
    // 6 locations: 1 from at() + 5 from .at() calls
    assert_eq!(err.trace_len(), 6);
}

// ============================================================================
// Helper functions called from loops
// ============================================================================

/// Helper called multiple times from same location.
/// Each call is logically distinct but has same file:line.
#[test]
fn helper_from_loop_same_location() {
    #[track_caller]
    fn validate(value: i32) -> Result<i32, At<TestError>> {
        if value < 0 {
            Err(at(TestError::Failed).at_data(|| value))
        } else {
            Ok(value)
        }
    }

    // Multiple calls from same line - each is a distinct error
    let results: Vec<_> = [-1, -2, -3].iter().map(|&v| validate(v)).collect();

    for (i, result) in results.iter().enumerate() {
        let err = result.as_ref().unwrap_err();
        // Each error has its own trace and context
        assert_eq!(err.trace_len(), 1);
        // Context contains the negative value
        let ctx = err.contexts().next().unwrap();
        let value = ctx.downcast_ref::<i32>().unwrap();
        assert_eq!(*value, -(i as i32 + 1));
    }
}

// ============================================================================
// Error type size impact
// ============================================================================

/// Large error types should still work
#[test]
fn large_error_type() {
    #[allow(dead_code)]
    #[derive(Debug)]
    struct LargeError {
        data: [u8; 256],
        message: String,
    }

    let large = LargeError {
        data: [0u8; 256],
        message: "test error".to_string(),
    };

    let err = at(large);
    assert_eq!(err.trace_len(), 1);
    assert_eq!(err.error().data.len(), 256);
}

// ============================================================================
// start_at vs at usage
// ============================================================================

/// Using start_at on external errors
#[test]
fn start_at_external_errors() {
    fn external_api() -> Result<(), &'static str> {
        Err("external error")
    }

    fn wrapper() -> Result<(), At<&'static str>> {
        external_api().start_at()
    }

    fn outer() -> Result<(), At<&'static str>> {
        wrapper().at()
    }

    let err = outer().unwrap_err();
    assert_eq!(err.trace_len(), 2); // start_at + at
    assert_eq!(*err.error(), "external error");
}

/// Mixing start_at and at in call chain
#[test]
fn mixed_start_at_and_at() {
    fn level_0() -> Result<(), &'static str> {
        Err("base error")
    }

    fn level_1() -> Result<(), At<&'static str>> {
        level_0().start_at()
    }

    fn level_2() -> Result<(), At<&'static str>> {
        level_1().at()
    }

    fn level_3() -> Result<(), At<&'static str>> {
        level_2().at_str("in level 3")
    }

    fn level_4() -> Result<(), At<&'static str>> {
        level_3().at()
    }

    let err = level_4().unwrap_err();
    // level_1: start_at creates location
    // level_2: at() creates location
    // level_3: at_str adds context to level_2's location (no new location!)
    // level_4: at() creates location
    assert_eq!(err.trace_len(), 3);
}

// ============================================================================
// Trace display formatting edge cases
// ============================================================================

/// Empty context string
#[test]
fn empty_context_string() {
    let err = at(TestError::Failed).at_str("");
    let display = format!("{:?}", err);
    // Should handle empty string gracefully
    assert!(display.contains("Failed"));
}

/// Very long context string
#[test]
fn very_long_context_string() {
    let long_msg = "x".repeat(10000);
    let err = at(TestError::Failed).at_string(|| long_msg.clone());
    let display = format!("{:?}", err);
    // Should include the long string
    assert!(display.contains(&long_msg));
}

/// Unicode in context
#[test]
fn unicode_context() {
    let err = at(TestError::Failed)
        .at_str("日本語テスト")
        .at_string(|| "émojis: 🦀🔥✨".to_string());

    let display = format!("{:?}", err);
    assert!(display.contains("日本語テスト"));
    assert!(display.contains("🦀"));
}

// ============================================================================
// Concurrent usage (if relevant)
// ============================================================================

/// Errors can be sent across threads
#[test]
fn error_is_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<At<TestError>>();
    assert_sync::<At<TestError>>();
}

/// Create error in thread, consume in main
#[test]
fn error_across_threads() {
    use std::thread;

    let handle = thread::spawn(|| -> Result<(), At<TestError>> {
        Err(at(TestError::Failed).at_str("from thread"))
    });

    let result = handle.join().unwrap();
    let err = result.unwrap_err();
    assert_eq!(err.trace_len(), 1);
}
