//! Test ergonomics of errat with different error handling approaches.
//!
//! Tests interaction with:
//! - At<E> (errat's wrapper)
//! - AtTraceable (embedded traces)
//! - Regular enums
//! - anyhow
//! - thiserror

// Allow large error types in tests - we're demonstrating API usage, not optimizing for size
#![allow(clippy::result_large_err)]

use errat::{at, At, AtTrace, AtTraceable, BoxedTrace, ResultAtExt, ResultAtTraceableExt, ResultStartAtExt};
use std::error::Error;
use std::fmt;
use std::io;

// ============================================================================
// 1. Regular enum errors (no external crate)
// ============================================================================

#[derive(Debug)]
enum PlainError {
    NotFound,
    InvalidInput(String),
}

impl fmt::Display for PlainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlainError::NotFound => write!(f, "not found"),
            PlainError::InvalidInput(s) => write!(f, "invalid input: {}", s),
        }
    }
}

impl std::error::Error for PlainError {}

// ============================================================================
// 2. thiserror-based errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
enum ThiserrorError {
    #[error("database connection failed")]
    DbConnection,
    #[error("query failed: {0}")]
    Query(String),
    #[error("io error")]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
enum OuterThiserror {
    #[error("inner error occurred")]
    Inner(#[from] ThiserrorError),
}

// ============================================================================
// 3. AtTraceable implementation with BoxedTrace (small footprint)
// ============================================================================

#[derive(Debug)]
struct TraceableError {
    kind: TraceableKind,
    trace: BoxedTrace,  // 8 bytes, not 24-256!
}

#[derive(Debug)]
enum TraceableKind {
    Parse,
    Network,
}

impl AtTraceable for TraceableError {
    fn trace_mut(&mut self) -> &mut AtTrace {
        self.trace.get_or_insert_mut()
    }
}

impl TraceableError {
    #[track_caller]
    fn parse() -> Self {
        Self {
            kind: TraceableKind::Parse,
            trace: BoxedTrace::capture(),
        }
    }

    #[track_caller]
    fn network() -> Self {
        Self {
            kind: TraceableKind::Network,
            trace: BoxedTrace::capture(),
        }
    }

    /// Create without trace (lazy allocation)
    fn lazy(kind: TraceableKind) -> Self {
        Self {
            kind,
            trace: BoxedTrace::new(),
        }
    }
}

impl fmt::Display for TraceableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            TraceableKind::Parse => write!(f, "parse error"),
            TraceableKind::Network => write!(f, "network error"),
        }
    }
}

impl std::error::Error for TraceableError {}

// ============================================================================
// Test: Plain enum with At<E>
// ============================================================================

#[test]
fn plain_enum_with_at() {
    fn inner() -> Result<(), At<PlainError>> {
        Err(at(PlainError::NotFound))
    }

    fn middle() -> Result<(), At<PlainError>> {
        inner().at_str("loading config")?;
        Ok(())
    }

    fn outer() -> Result<(), At<PlainError>> {
        middle().at()?;
        Ok(())
    }

    let err = outer().unwrap_err();

    // Display only shows the error message (from Display impl)
    let display = format!("{}", err);
    println!("Display:\n{}", display);
    assert!(display.contains("not found"), "Display should show error message");

    // Debug shows the full trace with contexts (error uses Debug format)
    let debug = format!("{:?}", err);
    println!("Debug:\n{}", debug);
    assert!(
        debug.contains("NotFound"),
        "Debug should show error variant: {}",
        debug
    );
    assert!(debug.contains("loading config"), "Debug should show context: {}", debug);
    assert!(debug.contains("error_ergonomics.rs"), "Debug should show file: {}", debug);

    // Check we have locations:
    // - at() in inner() creates first location
    // - at_str() in middle() adds context to same location (no new frame)
    // - at() in outer() creates second location
    assert!(err.trace_iter().count() >= 2, "Should have 2+ locations");
}

// ============================================================================
// Test: thiserror with At<E>
// ============================================================================

#[test]
fn thiserror_with_at() {
    fn db_layer() -> Result<(), At<ThiserrorError>> {
        Err(at(ThiserrorError::DbConnection))
    }

    fn service_layer() -> Result<(), At<ThiserrorError>> {
        db_layer().at_str("connecting to users db")?;
        Ok(())
    }

    let err = service_layer().unwrap_err();

    // Display shows the error message
    let display = format!("{}", err);
    assert!(
        display.contains("database connection failed"),
        "Display should show thiserror message"
    );

    // Debug shows the full trace with contexts
    let debug = format!("{:?}", err);
    assert!(
        debug.contains("connecting to users db"),
        "Debug should show context"
    );
}

// ============================================================================
// Test: thiserror with source chain + at_error
// ============================================================================

#[test]
fn thiserror_source_chain_with_at_error() {
    fn io_operation() -> Result<(), io::Error> {
        Err(io::Error::new(io::ErrorKind::NotFound, "file missing"))
    }

    fn db_operation() -> Result<(), At<ThiserrorError>> {
        // Convert io::Error to ThiserrorError::Io via From, then wrap in At
        io_operation()
            .map_err(ThiserrorError::from)
            .start_at()
            .at_str("reading config file")?;
        Ok(())
    }

    let err = db_operation().unwrap_err();

    // Display shows the error message
    let display = format!("{}", err);
    assert!(display.contains("io error"), "Display should show thiserror message");

    // Debug shows the context
    let debug = format!("{:?}", err);
    assert!(debug.contains("reading config file"), "Debug should show context");
}

// ============================================================================
// Test: Nested thiserror with At<E>
// ============================================================================

#[test]
fn nested_thiserror_with_at() {
    fn inner_fails() -> Result<(), ThiserrorError> {
        Err(ThiserrorError::Query("syntax error".into()))
    }

    fn outer_wraps() -> Result<(), At<OuterThiserror>> {
        inner_fails()
            .map_err(OuterThiserror::from)
            .start_at()
            .at_str("executing user query")?;
        Ok(())
    }

    let err = outer_wraps().unwrap_err();

    // Display shows the outer error message
    let display = format!("{}", err);
    assert!(
        display.contains("inner error occurred"),
        "Display should show outer error"
    );

    // Debug shows the context
    let debug = format!("{:?}", err);
    assert!(
        debug.contains("executing user query"),
        "Debug should show context"
    );

    // The inner error is in .source() via thiserror's #[from]
    let source = err.error().source();
    assert!(source.is_some(), "Should have source chain from thiserror");
    assert!(
        source.unwrap().to_string().contains("query failed"),
        "Source should be inner error"
    );
}

// ============================================================================
// Test: AtTraceable direct usage
// ============================================================================

#[test]
fn attraceable_direct_usage() {
    fn parser() -> Result<(), TraceableError> {
        Err(TraceableError::parse())
    }

    fn caller() -> Result<(), TraceableError> {
        parser().at_str("parsing header")?;
        Ok(())
    }

    fn outer() -> Result<(), TraceableError> {
        caller().at()?;
        Ok(())
    }

    let err = outer().unwrap_err();

    // TraceableError has embedded trace - verify it's not empty
    assert!(!err.trace.is_empty(), "Trace should not be empty");

    // We can convert to At<TraceableError> to use its API if needed
    // but the trace is already embedded in the error type
}

// ============================================================================
// Test: at_error embeds source errors in trace
// ============================================================================

#[test]
fn at_error_embeds_source() {
    fn io_fails() -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, "access denied")
    }

    fn operation() -> Result<(), At<PlainError>> {
        let io_err = io_fails();
        Err(at(PlainError::NotFound).at_error(io_err))
    }

    let err = operation().unwrap_err();

    // Display only shows the main error
    let display = format!("{}", err);
    println!("Display:\n{}", display);
    assert!(display.contains("not found"), "Display should show main error");

    // Debug shows the embedded error (uses Debug format for error)
    let debug = format!("{:?}", err);
    println!("Debug:\n{}", debug);
    assert!(
        debug.contains("NotFound"),
        "Debug should show main error variant: {}",
        debug
    );
    assert!(
        debug.contains("caused by") && debug.contains("access denied"),
        "Debug should show embedded error: {}",
        debug
    );

    // Verify we can access the embedded error via contexts
    let mut found_error_context = false;
    for ctx in err.contexts() {
        if ctx.is_error() {
            found_error_context = true;
            let inner = ctx.as_error().unwrap();
            assert!(inner.to_string().contains("access denied"));
        }
    }
    assert!(found_error_context, "Should have error context");
}

// ============================================================================
// Test: at_error shows in Debug output (Display only shows error message)
// ============================================================================

#[test]
fn at_error_shows_in_debug() {
    #[derive(Debug)]
    struct SourceError(&'static str);
    impl fmt::Display for SourceError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "source: {}", self.0)
        }
    }
    impl std::error::Error for SourceError {}

    let err = at(PlainError::InvalidInput("bad".into()))
        .at_str("validating")
        .at_error(SourceError("underlying cause"));

    // Display only shows the error message (not trace/contexts)
    let display = format!("{}", err);
    assert!(
        display.contains("invalid input: bad"),
        "Display should show error message: {}",
        display
    );

    // Debug shows the full trace with embedded errors
    let debug = format!("{:?}", err);
    println!("Debug:\n{}", debug);

    assert!(
        debug.contains("underlying cause"),
        "Debug should show embedded error: {}",
        debug
    );
    assert!(
        debug.contains("caused by"),
        "Debug should have 'caused by' prefix: {}",
        debug
    );
}

// ============================================================================
// Test: Converting between At<E> types preserves trace
// ============================================================================

#[test]
fn map_error_preserves_trace() {
    fn inner() -> Result<(), At<PlainError>> {
        Err(at(PlainError::NotFound).at_str("in inner"))
    }

    fn outer() -> Result<(), At<ThiserrorError>> {
        inner()
            .map_err(|e| e.map_error(|_| ThiserrorError::DbConnection))
            .at_str("in outer")?;
        Ok(())
    }

    let err = outer().unwrap_err();

    // Display shows the new error type
    let display = format!("{}", err);
    assert!(
        display.contains("database connection failed"),
        "Display should have new error type"
    );

    // Debug shows both contexts preserved
    let debug = format!("{:?}", err);
    assert!(debug.contains("in inner"), "Debug should preserve inner context");
    assert!(debug.contains("in outer"), "Debug should have outer context");
}

// ============================================================================
// Test: anyhow interop - wrapping At<E> in anyhow
// ============================================================================

#[test]
fn anyhow_wraps_at_error() {
    fn errat_fn() -> Result<(), At<PlainError>> {
        Err(at(PlainError::NotFound).at_str("looking up user"))
    }

    fn anyhow_fn() -> anyhow::Result<()> {
        errat_fn()?; // At<E> implements Error, so this works
        Ok(())
    }

    let err = anyhow_fn().unwrap_err();
    let text = format!("{:?}", err); // anyhow uses Debug for full chain

    assert!(text.contains("not found"), "Should show error");
    // Note: anyhow captures its own backtrace, errat trace is in our Display
}

// ============================================================================
// Test: anyhow interop - embedding anyhow::Error in trace
// ============================================================================

#[test]
fn at_error_with_anyhow() {
    fn anyhow_operation() -> anyhow::Result<i32> {
        anyhow::bail!("anyhow failure")
    }

    fn traced_operation() -> Result<(), At<PlainError>> {
        match anyhow_operation() {
            Ok(_) => Ok(()),
            Err(e) => {
                // Convert anyhow to a boxed error for at_error
                // anyhow::Error doesn't impl Error directly in a way we can use,
                // but we can use its Display to capture the message
                Err(at(PlainError::InvalidInput(e.to_string())).at_str("during anyhow op"))
            }
        }
    }

    let err = traced_operation().unwrap_err();

    // Display shows the error message (which includes the anyhow message)
    let display = format!("{}", err);
    println!("Display:\n{}", display);
    assert!(display.contains("anyhow failure"), "Display should capture anyhow msg");

    // Debug shows the context
    let debug = format!("{:?}", err);
    println!("Debug:\n{}", debug);
    assert!(debug.contains("during anyhow op"), "Debug should have context");
}

// ============================================================================
// Test: Multiple at_error calls create proper chain
// ============================================================================

#[test]
fn multiple_at_error_chain() {
    #[derive(Debug)]
    struct ErrA;
    impl fmt::Display for ErrA {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "error A")
        }
    }
    impl std::error::Error for ErrA {}

    #[derive(Debug)]
    struct ErrB;
    impl fmt::Display for ErrB {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "error B")
        }
    }
    impl std::error::Error for ErrB {}

    let err = at(PlainError::NotFound)
        .at_str("step 1")
        .at_error(ErrA)
        .at_str("step 2")
        .at_error(ErrB);

    // Debug shows all contexts and embedded errors
    let debug = format!("{:?}", err);
    println!("Chained errors (Debug):\n{}", debug);

    assert!(debug.contains("not found") || debug.contains("NotFound"), "Main error");
    assert!(debug.contains("step 1"), "Context 1");
    assert!(debug.contains("step 2"), "Context 2");
    assert!(debug.contains("error A"), "Error A should appear");
    assert!(debug.contains("error B"), "Error B should appear");
    assert!(debug.contains("caused by"), "Should have 'caused by' prefixes");

    // Count error contexts
    let error_count = err.contexts().filter(|c| c.is_error()).count();
    assert_eq!(error_count, 2, "Should have 2 embedded errors");
}

// ============================================================================
// Test: AtTraceable with at_error
// ============================================================================

#[test]
fn attraceable_with_at_error() {
    fn inner_io() -> io::Error {
        io::Error::new(io::ErrorKind::BrokenPipe, "connection reset")
    }

    fn network_op() -> Result<(), TraceableError> {
        Err(TraceableError::network().at_error(inner_io()))
    }

    let err = network_op().unwrap_err();

    // Verify trace is not empty
    assert!(!err.trace.is_empty(), "Trace should not be empty");

    // To access contexts, wrap in At<> and check via its public API
    // Or we could add a public contexts() to AtTraceable in the future
    // For now, we verify the trace exists and the at_error call compiled
}

// ============================================================================
// Test: Verify nested error appears in Debug output
// ============================================================================
//
// NOTE: Display for At<E> only shows the error message (for clean logging).
// The full trace with contexts (including nested errors) appears in Debug.
// This is intentional - use {:?} for detailed error information.

#[test]
fn nested_error_shows_in_debug_output() {
    #[derive(Debug)]
    struct DatabaseError {
        code: i32,
    }
    impl fmt::Display for DatabaseError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "database error code {}", self.code)
        }
    }
    impl std::error::Error for DatabaseError {}

    let err = at(PlainError::NotFound)
        .at_str("looking up record")
        .at_error(DatabaseError { code: 1045 });

    // Display only shows the error message (clean for logs)
    let display = format!("{}", err);
    println!("=== DISPLAY OUTPUT ===\n{}", display);
    assert!(
        display.contains("not found"),
        "Display should show main error message"
    );

    // Debug shows the full trace with nested errors
    let debug = format!("{:?}", err);
    println!("=== DEBUG OUTPUT ===\n{}", debug);

    // CRITICAL: The nested error MUST appear in Debug output
    assert!(
        debug.contains("database error code 1045"),
        "Nested error MUST appear in Debug output!\nActual output:\n{}",
        debug
    );
    assert!(
        debug.contains("caused by"),
        "Should have 'caused by' prefix for nested errors!\nActual output:\n{}",
        debug
    );
    assert!(
        debug.contains("looking up record"),
        "Context should appear in Debug!\nActual output:\n{}",
        debug
    );
}

// ============================================================================
// Test: Debug output shows multiple nested errors with proper formatting
// ============================================================================

#[test]
fn nested_errors_format_correctly() {
    #[derive(Debug)]
    struct ApiError {
        endpoint: &'static str,
    }
    impl fmt::Display for ApiError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "API error at {}", self.endpoint)
        }
    }
    impl std::error::Error for ApiError {}

    let err = at(PlainError::InvalidInput("bad request".into()))
        .at_error(ApiError { endpoint: "/users" });

    let debug = format!("{:?}", err);
    println!("=== DEBUG MODE OUTPUT ===\n{}", debug);

    assert!(
        debug.contains("API error at /users"),
        "Debug should show nested error with Display format!\nActual output:\n{}",
        debug
    );
    assert!(
        debug.contains("caused by"),
        "Debug should prefix nested errors with 'caused by'!\nActual output:\n{}",
        debug
    );
}

// ============================================================================
// Test: BoxedTrace keeps error types small
// ============================================================================

#[test]
fn boxed_trace_small_footprint() {
    // BoxedTrace should be exactly pointer-sized (8 bytes on 64-bit)
    assert_eq!(
        std::mem::size_of::<BoxedTrace>(),
        std::mem::size_of::<*const ()>(),
        "BoxedTrace should be pointer-sized"
    );

    // TraceableError with BoxedTrace should be small
    let traceable_size = std::mem::size_of::<TraceableError>();
    assert!(
        traceable_size <= 24,
        "TraceableError with BoxedTrace should be ≤24 bytes, got {}",
        traceable_size
    );
}

#[test]
fn boxed_trace_lazy_allocation() {
    // Empty BoxedTrace - no allocation
    let err = TraceableError::lazy(TraceableKind::Parse);
    assert!(err.trace.is_empty(), "Lazy trace should be empty initially");

    // Adding context triggers allocation
    let err = err.at_str("context");
    assert!(!err.trace.is_empty(), "Trace should exist after at_str");
}

#[test]
fn boxed_trace_capture() {
    let err = TraceableError::parse();
    assert!(!err.trace.is_empty(), "Capture should create non-empty trace");
    assert_eq!(err.trace.frame_count(), 1, "Should have one frame");
}

// ============================================================================
// Test: frames() unified iteration API
// ============================================================================

#[test]
fn frames_api_on_at() {
    let err = at(PlainError::NotFound)
        .at_str("step 1")
        .at()
        .at_str("step 2");

    let frames: Vec<_> = err.frames().collect();
    assert_eq!(frames.len(), 2, "Should have 2 frames");

    // First frame (oldest) has "step 1" context
    let first = &frames[0];
    assert!(first.location().is_some(), "Should have location");
    let first_contexts: Vec<_> = first.contexts().collect();
    assert!(first_contexts.iter().any(|c| c.as_text() == Some("step 1")));

    // Second frame has "step 2" context
    let second = &frames[1];
    let second_contexts: Vec<_> = second.contexts().collect();
    assert!(second_contexts.iter().any(|c| c.as_text() == Some("step 2")));
}

#[test]
fn frames_api_on_attraceable() {
    fn inner() -> Result<(), TraceableError> {
        Err(TraceableError::parse().at_str("parsing"))
    }

    fn outer() -> Result<(), TraceableError> {
        inner().at()?;
        Ok(())
    }

    let err = outer().unwrap_err();

    // Use frames() on BoxedTrace
    let frames: Vec<_> = err.trace.frames().collect();
    assert!(frames.len() >= 2, "Should have 2+ frames");

    // Check first frame has context
    let has_parsing_ctx = frames.iter().any(|f| {
        f.contexts().any(|c| c.as_text() == Some("parsing"))
    });
    assert!(has_parsing_ctx, "Should find 'parsing' context");
}

#[test]
fn frames_with_skipped_marker() {
    let err = at(PlainError::NotFound)
        .at_skipped_frames()
        .at();

    let frames: Vec<_> = err.frames().collect();
    assert_eq!(frames.len(), 3, "Should have 3 frames");

    // Middle frame should be skipped marker
    assert!(frames[1].is_skipped(), "Middle frame should be skipped marker");
    assert!(frames[1].location().is_none());
}

#[test]
fn frames_with_error_context() {
    #[derive(Debug)]
    struct SourceErr;
    impl fmt::Display for SourceErr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "source error")
        }
    }
    impl std::error::Error for SourceErr {}

    let err = at(PlainError::NotFound)
        .at_error(SourceErr)
        .at_str("with context");

    for frame in err.frames() {
        for ctx in frame.contexts() {
            if ctx.is_error() {
                assert!(ctx.as_error().unwrap().to_string().contains("source"));
            }
        }
    }
}

// ============================================================================
// Test: frame_count() convenience method
// ============================================================================

#[test]
fn frame_count_api() {
    let err = at(PlainError::NotFound);
    assert_eq!(err.frame_count(), 1);

    let err = err.at().at().at();
    assert_eq!(err.frame_count(), 4);
}
