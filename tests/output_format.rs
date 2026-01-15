//! Integration tests for error output formatting.

use core::error::Error;
use errat::{At, ErrorAtExt, ResultAtExt, ResultTraceExt, at, start_at_late};

#[derive(Debug)]
enum TestError {
    NotFound,
    InvalidInput(String),
}

impl core::fmt::Display for TestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TestError::NotFound => write!(f, "not found"),
            TestError::InvalidInput(s) => write!(f, "invalid input: {}", s),
        }
    }
}

impl core::error::Error for TestError {}

// ============================================================================
// Basic Output Structure
// ============================================================================

#[test]
fn debug_output_has_error_header() {
    let err = TestError::NotFound.start_at();
    let output = format!("{:?}", err);

    assert!(
        output.starts_with("Error: NotFound"),
        "Should start with 'Error:' header. Got:\n{}",
        output
    );
}

#[test]
fn debug_output_has_location_lines() {
    let err = TestError::NotFound.start_at();
    let output = format!("{:?}", err);

    assert!(
        output.contains("    at "),
        "Should have indented 'at' location lines. Got:\n{}",
        output
    );
    assert!(
        output.contains("output_format.rs:"),
        "Should reference this file. Got:\n{}",
        output
    );
}

#[test]
fn display_output_is_just_error() {
    let err = TestError::InvalidInput("test".into()).start_at();
    let output = format!("{}", err);

    assert_eq!(
        output, "invalid input: test",
        "Display should show only the error message"
    );
}

// ============================================================================
// Context Formatting
// ============================================================================

#[test]
fn context_uses_corner_prefix() {
    let err = TestError::NotFound.start_at().at_str("doing something");
    let output = format!("{:?}", err);

    assert!(
        output.contains("╰─ doing something"),
        "Context should use '╰─' prefix. Got:\n{}",
        output
    );
}

#[test]
fn multiple_contexts_each_have_prefix() {
    fn inner() -> Result<(), At<TestError>> {
        Err(TestError::NotFound.start_at())
    }

    fn middle() -> Result<(), At<TestError>> {
        inner().at_str("in middle")?;
        Ok(())
    }

    fn outer() -> Result<(), At<TestError>> {
        middle().at_str("in outer")?;
        Ok(())
    }

    let err = outer().unwrap_err();
    let output = format!("{:?}", err);

    assert!(
        output.contains("╰─ in middle"),
        "Should have middle context. Got:\n{}",
        output
    );
    assert!(
        output.contains("╰─ in outer"),
        "Should have outer context. Got:\n{}",
        output
    );
}

#[test]
fn debug_context_shows_debug_format() {
    #[derive(Debug)]
    #[allow(dead_code)]
    struct Info {
        id: u64,
    }

    let err = TestError::NotFound.start_at().at_debug(|| Info { id: 42 });
    let output = format!("{:?}", err);

    assert!(
        output.contains("Info") && output.contains("42"),
        "Debug context should show struct debug format. Got:\n{}",
        output
    );
}

#[test]
fn data_context_shows_display_format() {
    struct Message(String);
    impl core::fmt::Display for Message {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "message: {}", self.0)
        }
    }

    let err = TestError::NotFound
        .start_at()
        .at_data(|| Message("hello".into()));
    let output = format!("{:?}", err);

    assert!(
        output.contains("message: hello"),
        "Display context should use Display format. Got:\n{}",
        output
    );
}

// ============================================================================
// Skip Marker Formatting
// ============================================================================

#[test]
fn skip_marker_shows_brackets() {
    let err = start_at_late!(TestError::NotFound);
    let output = format!("{:?}", err);

    assert!(
        output.contains("[...]"),
        "Skip marker should show '[...]'. Got:\n{}",
        output
    );
}

#[test]
fn at_skipped_adds_skip_marker() {
    let err = at(TestError::NotFound).at_skipped();
    let output = format!("{:?}", err);

    assert!(
        output.contains("[...]"),
        "at_skipped() should add '[...]' marker. Got:\n{}",
        output
    );
}

#[test]
fn trace_skipped_adds_skip_marker() {
    fn fallible() -> Result<(), &'static str> {
        Err("legacy error")
    }

    let err = fallible().trace_skipped().unwrap_err();
    let output = format!("{:?}", err);

    assert!(
        output.contains("[...]"),
        "trace_skipped() should add '[...]' marker. Got:\n{}",
        output
    );
}

// ============================================================================
// Location Order (oldest first)
// ============================================================================

#[test]
fn locations_are_oldest_first() {
    fn level1() -> Result<(), At<TestError>> {
        Err(TestError::NotFound.start_at())
    }

    fn level2() -> Result<(), At<TestError>> {
        level1().at_str("level2")?;
        Ok(())
    }

    fn level3() -> Result<(), At<TestError>> {
        level2().at_str("level3")?;
        Ok(())
    }

    let err = level3().unwrap_err();
    let output = format!("{:?}", err);

    let level2_pos = output.find("level2").expect("should have level2");
    let level3_pos = output.find("level3").expect("should have level3");

    assert!(
        level2_pos < level3_pos,
        "level2 (older) should appear before level3 (newer). Got:\n{}",
        output
    );
}

// ============================================================================
// Multi-line Output Structure
// ============================================================================

#[test]
fn output_has_proper_indentation() {
    let err = TestError::NotFound.start_at().at_str("with context");
    let output = format!("{:?}", err);

    for line in output.lines().skip(1) {
        // Skip the "Error:" header line
        if line.trim().is_empty() {
            continue;
        }
        assert!(
            line.starts_with("    ") || line.starts_with("       "),
            "Non-header lines should be indented. Bad line: '{}'",
            line
        );
    }
}

#[test]
fn context_indentation_deeper_than_location() {
    let err = TestError::NotFound.start_at().at_str("context here");
    let output = format!("{:?}", err);

    let at_line = output
        .lines()
        .find(|l| l.contains("at ") && l.contains(".rs:"))
        .expect("should have 'at' line");
    let context_line = output
        .lines()
        .find(|l| l.contains("╰─"))
        .expect("should have context line");

    let at_indent = at_line.len() - at_line.trim_start().len();
    let ctx_indent = context_line.len() - context_line.trim_start().len();

    assert!(
        ctx_indent > at_indent,
        "Context should be indented more than location. at={}, ctx={}",
        at_indent,
        ctx_indent
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn empty_trace_just_shows_error() {
    let err: At<TestError> = At::new(TestError::NotFound);
    let output = format!("{:?}", err);

    assert!(
        output.contains("NotFound"),
        "Should show error. Got:\n{}",
        output
    );
    // Should not have location lines
    let line_count = output.lines().count();
    assert!(
        line_count <= 2,
        "Empty trace should have minimal output. Got {} lines:\n{}",
        line_count,
        output
    );
}

#[test]
fn long_context_message_not_truncated() {
    let long_msg = "a]".repeat(100);
    let err = TestError::NotFound
        .start_at()
        .at_string(|| long_msg.clone());
    let output = format!("{:?}", err);

    assert!(
        output.contains(&long_msg),
        "Long messages should not be truncated. Got:\n{}",
        output
    );
}

// ============================================================================
// Crate Info in display_with_meta
// ============================================================================

#[test]
fn display_with_meta_shows_crate_name() {
    let err = at!(TestError::NotFound);
    let output = format!("{}", err.display_with_meta());

    assert!(
        output.contains("crate: errat"),
        "display_with_meta should show crate name. Got:\n{}",
        output
    );
}

// ============================================================================
// Nested Errors with source()
// ============================================================================

#[derive(Debug)]
struct IoError {
    msg: &'static str,
}

impl core::fmt::Display for IoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "io error: {}", self.msg)
    }
}

impl Error for IoError {}

#[derive(Debug)]
struct AppError {
    msg: &'static str,
    source: Option<IoError>,
}

impl core::fmt::Display for AppError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "app error: {}", self.msg)
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|e| e as &(dyn Error + 'static))
    }
}

#[test]
fn at_delegates_source_to_inner() {
    let inner = IoError { msg: "disk full" };
    let outer = AppError {
        msg: "save failed",
        source: Some(inner),
    };

    let traced: At<AppError> = at(outer);

    // At<E> should delegate source() to E::source()
    let source = traced.source().expect("should have source");
    let source_msg = format!("{}", source);
    assert!(
        source_msg.contains("disk full"),
        "source() should return inner error. Got: {}",
        source_msg
    );
}

#[test]
fn at_source_is_none_when_inner_has_no_source() {
    let outer = AppError {
        msg: "no cause",
        source: None,
    };

    let traced: At<AppError> = at(outer);
    assert!(
        traced.source().is_none(),
        "source() should be None when inner has no source"
    );
}

#[test]
fn error_chain_traversable() {
    let inner = IoError {
        msg: "connection reset",
    };
    let outer = AppError {
        msg: "request failed",
        source: Some(inner),
    };

    let traced: At<AppError> = at(outer);

    // Walk the error chain
    let mut chain: Vec<String> = vec![];
    let mut current: Option<&(dyn Error + 'static)> = Some(&traced);

    while let Some(err) = current {
        chain.push(format!("{}", err));
        current = err.source();
    }

    // At<E> is transparent - delegates source() directly to E::source()
    // Chain: At<AppError> (displays as AppError) -> IoError
    assert_eq!(
        chain.len(),
        2,
        "Chain should have 2 errors: At<AppError> -> IoError"
    );
    assert!(
        chain[0].contains("request failed"),
        "First should be AppError (via At)"
    );
    assert!(
        chain[1].contains("connection reset"),
        "Second should be IoError"
    );
}

#[test]
fn nested_at_errors() {
    // At<At<E>> - nested traced errors
    let inner_traced: At<IoError> = at(IoError { msg: "read failed" });
    let outer_traced: At<At<IoError>> = at(inner_traced);

    assert_eq!(
        outer_traced.trace_len(),
        1,
        "Outer should have its own trace"
    );
    assert_eq!(
        outer_traced.error().trace_len(),
        1,
        "Inner should have its own trace"
    );

    // Can access the innermost error
    let innermost = outer_traced.error().error();
    assert_eq!(innermost.msg, "read failed");
}

#[derive(Debug)]
struct ThreeLevelError {
    msg: &'static str,
    source: Option<AppError>,
}

impl core::fmt::Display for ThreeLevelError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "level3: {}", self.msg)
    }
}

impl Error for ThreeLevelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|e| e as &(dyn Error + 'static))
    }
}

#[test]
fn three_level_error_chain() {
    let level1 = IoError { msg: "level1" };
    let level2 = AppError {
        msg: "level2",
        source: Some(level1),
    };
    let level3 = ThreeLevelError {
        msg: "level3",
        source: Some(level2),
    };

    let traced: At<ThreeLevelError> = at(level3);

    // Walk full chain - At<E> is transparent, delegates to E::source()
    let mut depth = 0;
    let mut current: Option<&(dyn Error + 'static)> = Some(&traced);
    while let Some(err) = current {
        depth += 1;
        current = err.source();
    }

    // Chain: At<ThreeLevelError> -> AppError -> IoError (At is transparent)
    assert_eq!(
        depth, 3,
        "Should have 3 levels: At<ThreeLevelError> -> AppError -> IoError"
    );
}

#[test]
fn source_preserved_through_trace_operations() {
    let inner = IoError { msg: "original" };
    let outer = AppError {
        msg: "wrapped",
        source: Some(inner),
    };

    // Chain multiple trace operations
    let traced = at(outer).at_str("context1").at_str("context2").at_skipped();

    // source() should still work
    let source = traced.source().expect("should have source");
    assert!(
        format!("{}", source).contains("original"),
        "source should still be accessible after trace operations"
    );
}
