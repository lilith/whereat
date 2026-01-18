//! Tests for frame surgery APIs: pop, push, pop_first, push_first
//!
//! These APIs allow manipulation of trace frames for advanced use cases like
//! transferring frames between traces or reordering error context.

use whereat::{at, At, AtFrameOwned, AtTraceable};

#[derive(Debug)]
struct TestError;

impl core::fmt::Display for TestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "test error")
    }
}

impl core::error::Error for TestError {}

// ============================================================================
// Basic pop/push operations
// ============================================================================

#[test]
fn pop_returns_last_frame() {
    let mut err = at(TestError).at_str("first").at().at_str("second");

    // Should have 2 frames
    assert_eq!(err.frame_count(), 2);

    // Pop the last frame
    let frame = err.at_pop().expect("should have a frame");

    // Frame should have the "second" context
    assert_eq!(frame.context_count(), 1);
    let ctx_text: Vec<_> = frame.contexts().filter_map(|c| c.as_text()).collect();
    assert_eq!(ctx_text, vec!["second"]);

    // Error should now have 1 frame
    assert_eq!(err.frame_count(), 1);
}

#[test]
fn pop_empty_trace_returns_none() {
    let mut err = At::wrap(TestError);
    assert!(err.at_pop().is_none());
}

#[test]
fn push_adds_frame_at_end() {
    let mut err = at(TestError).at_str("original");

    assert_eq!(err.frame_count(), 1);

    // Create and push a new frame
    let new_frame = AtFrameOwned::capture().with_str("pushed");
    err.at_push(new_frame);

    assert_eq!(err.frame_count(), 2);

    // Verify order: original first, pushed second
    let debug = format!("{:?}", err);
    let orig_pos = debug.find("original").expect("should have original");
    let push_pos = debug.find("pushed").expect("should have pushed");
    assert!(orig_pos < push_pos, "original should come before pushed");
}

#[test]
fn pop_then_push_roundtrip() {
    let mut err = at(TestError).at_str("context A").at().at_str("context B");

    let original_count = err.frame_count();
    let frame = err.at_pop().expect("should pop");

    assert_eq!(err.frame_count(), original_count - 1);

    err.at_push(frame);
    assert_eq!(err.frame_count(), original_count);

    // Context B should still be present
    let debug = format!("{:?}", err);
    assert!(debug.contains("context B"));
}

// ============================================================================
// First frame operations (pop_first, push_first)
// ============================================================================

#[test]
fn pop_first_returns_oldest_frame() {
    let mut err = at(TestError).at_str("first").at().at_str("second");

    let frame = err.at_first_pop().expect("should have a frame");

    // Frame should have the "first" context (the oldest)
    let ctx_text: Vec<_> = frame.contexts().filter_map(|c| c.as_text()).collect();
    assert_eq!(ctx_text, vec!["first"]);

    // Remaining frame should have "second"
    let debug = format!("{:?}", err);
    assert!(debug.contains("second"));
    assert!(!debug.contains("first"));
}

#[test]
fn push_first_inserts_at_beginning() {
    let mut err = at(TestError).at_str("original");

    let new_frame = AtFrameOwned::capture().with_str("inserted");
    err.at_first_insert(new_frame);

    assert_eq!(err.frame_count(), 2);

    // Verify order: inserted should come before original in output
    // (traces display oldest first)
    let debug = format!("{:?}", err);
    let insert_pos = debug.find("inserted").expect("should have inserted");
    let orig_pos = debug.find("original").expect("should have original");
    assert!(
        insert_pos < orig_pos,
        "inserted should come before original"
    );
}

#[test]
fn pop_first_empty_returns_none() {
    let mut err = At::wrap(TestError);
    assert!(err.at_first_pop().is_none());
}

// ============================================================================
// Frame manipulation with contexts
// ============================================================================

#[test]
fn pop_preserves_multiple_contexts() {
    let mut err = at(TestError)
        .at_str("ctx1")
        .at_str("ctx2")
        .at_str("ctx3");

    let frame = err.at_pop().expect("should pop");

    // All three contexts should be in the frame
    assert_eq!(frame.context_count(), 3);

    let ctx_text: Vec<_> = frame.contexts().filter_map(|c| c.as_text()).collect();
    assert_eq!(ctx_text, vec!["ctx1", "ctx2", "ctx3"]);
}

#[test]
fn push_frame_with_multiple_contexts() {
    let mut err = at(TestError);

    let frame = AtFrameOwned::capture()
        .with_str("static context")
        .with_string("dynamic context".to_string())
        .with_data(42i32);

    err.at_push(frame);

    let debug = format!("{:?}", err);
    assert!(debug.contains("static context"));
    assert!(debug.contains("dynamic context"));
    assert!(debug.contains("42"));
}

// ============================================================================
// Transfer between traces
// ============================================================================

#[test]
fn transfer_frame_between_errors() {
    let mut source = at(TestError).at_str("from source");
    let mut dest = at(TestError).at_str("dest original");

    // Transfer frame from source to dest
    if let Some(frame) = source.at_pop() {
        dest.at_push(frame);
    }

    // Source should be empty, dest should have both
    assert_eq!(source.frame_count(), 0);
    assert_eq!(dest.frame_count(), 2);

    let debug = format!("{:?}", dest);
    assert!(debug.contains("dest original"));
    assert!(debug.contains("from source"));
}

#[test]
fn transfer_all_frames() {
    let mut source = at(TestError)
        .at_str("frame 1")
        .at()
        .at_str("frame 2")
        .at()
        .at_str("frame 3");

    // Start with an empty wrapper (no frames yet)
    let mut dest = At::wrap(TestError);

    // Transfer all frames
    while let Some(frame) = source.at_pop() {
        dest.at_first_insert(frame); // Insert at beginning to preserve order
    }

    assert_eq!(source.frame_count(), 0);
    assert_eq!(dest.frame_count(), 3);

    // Verify all contexts transferred
    let debug = format!("{:?}", dest);
    assert!(debug.contains("frame 1"));
    assert!(debug.contains("frame 2"));
    assert!(debug.contains("frame 3"));
}

// ============================================================================
// AtFrameOwned builder methods
// ============================================================================

#[test]
fn frame_owned_capture_gets_location() {
    let frame = AtFrameOwned::capture();
    assert!(frame.location().is_some());
    assert!(!frame.is_skipped());
}

#[test]
fn frame_owned_new_with_none_is_skipped() {
    let frame = AtFrameOwned::new(None);
    assert!(frame.location().is_none());
    assert!(frame.is_skipped());
}

#[test]
fn frame_owned_builder_chain() {
    let frame = AtFrameOwned::capture()
        .with_str("static")
        .with_string("dynamic".into())
        .with_data("display data")
        .with_debug(vec![1, 2, 3]);

    assert_eq!(frame.context_count(), 4);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn pop_all_then_push_new() {
    let mut err = at(TestError).at_str("will be removed");

    // Pop everything
    while err.at_pop().is_some() {}

    assert_eq!(err.frame_count(), 0);

    // Push new frame
    let frame = AtFrameOwned::capture().with_str("new frame");
    err.at_push(frame);

    assert_eq!(err.frame_count(), 1);
    let debug = format!("{:?}", err);
    assert!(debug.contains("new frame"));
    assert!(!debug.contains("will be removed"));
}

#[test]
fn mixed_pop_and_pop_first() {
    let mut err = at(TestError)
        .at_str("A")
        .at()
        .at_str("B")
        .at()
        .at_str("C");

    // Pop last (C)
    let last = err.at_pop().expect("pop last");
    assert!(last.contexts().any(|c| c.as_text() == Some("C")));

    // Pop first (A)
    let first = err.at_first_pop().expect("pop first");
    assert!(first.contexts().any(|c| c.as_text() == Some("A")));

    // Only B should remain
    assert_eq!(err.frame_count(), 1);
    let debug = format!("{:?}", err);
    assert!(debug.contains("B"));
    assert!(!debug.contains("A"));
    assert!(!debug.contains("C"));
}

#[test]
fn push_skipped_frame_marker() {
    let mut err = at(TestError).at_str("real frame");

    // Push a skipped frames marker
    let skipped = AtFrameOwned::new(None).with_str("skipped section");
    err.at_push(skipped);

    assert_eq!(err.frame_count(), 2);

    // The skipped marker should appear in output
    let debug = format!("{:?}", err);
    assert!(debug.contains("[...]") || debug.contains("skipped"));
}

// ============================================================================
// AtTraceable frame surgery
// ============================================================================

#[derive(Debug)]
struct TraceableError {
    trace: whereat::AtTrace,
}

impl TraceableError {
    fn new() -> Self {
        Self {
            trace: whereat::AtTrace::new(),
        }
    }
}

impl core::fmt::Display for TraceableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "traceable error")
    }
}

impl core::error::Error for TraceableError {}

impl AtTraceable for TraceableError {
    fn trace(&self) -> Option<&whereat::AtTrace> {
        Some(&self.trace)
    }
    fn trace_mut(&mut self) -> &mut whereat::AtTrace {
        &mut self.trace
    }
    fn fmt_message(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "traceable error")
    }
}

#[test]
fn attraceable_pop_push() {
    let mut err = TraceableError::new().at().at_str("context");

    assert_eq!(err.trace().unwrap().frame_count(), 1);

    let frame = err.at_pop().expect("should pop");
    assert_eq!(err.trace().unwrap().frame_count(), 0);

    err.at_push(frame);
    assert_eq!(err.trace().unwrap().frame_count(), 1);
}

#[test]
fn attraceable_first_insert() {
    let mut err = TraceableError::new().at().at_str("original");

    let new_frame = AtFrameOwned::capture().with_str("inserted");
    err.at_first_insert(new_frame);

    assert_eq!(err.trace().unwrap().frame_count(), 2);

    let output = format!("{}", err.full_trace());
    assert!(output.contains("inserted"));
    assert!(output.contains("original"));
}
