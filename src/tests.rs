//! Unit tests for whereat.
//!
//! These tests are in a separate file for organization but remain in the `src/`
//! directory to retain access to `pub(crate)` items like `AtContext`.

use crate::context::AtContext;
use crate::trace::AtTrace;
use crate::{At, ErrorAtExt, ResultAtExt, at};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[derive(Debug, PartialEq, Eq, Hash)]
enum TestError {
    NotFound,
    InvalidInput,
}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestError::NotFound => write!(f, "not found"),
            TestError::InvalidInput => write!(f, "invalid input"),
        }
    }
}

impl core::error::Error for TestError {}

#[test]
fn test_sizeof() {
    use core::mem::size_of;

    // At<E> should be sizeof(E) + 8 (pointer to boxed trace)
    // With alignment, a 1-byte enum becomes 16 bytes total
    assert_eq!(size_of::<Option<Box<AtTrace>>>(), 8);

    let traced_size = size_of::<At<TestError>>();
    let error_size = size_of::<TestError>();
    let pointer_size = size_of::<Option<Box<AtTrace>>>();

    // Should be error + pointer, with possible padding
    assert!(traced_size <= error_size + pointer_size + 8); // Allow for alignment
    assert!(traced_size >= error_size + pointer_size);

    // For a 1-byte enum, should be 16 bytes (1 + 7 padding + 8 pointer)
    assert_eq!(traced_size, 16);
}

#[test]
fn test_sizeof_trace() {
    use core::mem::size_of;

    let trace_size = size_of::<AtTrace>();

    // AtTrace size depends on feature flags:
    // - Without tinyvec/smallvec: 40 bytes (locations Vec 24 + crate_info 8 + contexts Option<Box> 8)
    // - tinyvec-64-bytes: 64 bytes (TinyVec<4 slots> 48 + crate_info 8 + contexts 8)
    // - tinyvec-128-bytes / smallvec-128-bytes: 128 bytes (12 slots)
    // - tinyvec-256-bytes / smallvec-256-bytes: 256 bytes (28 slots)
    // - tinyvec-512-bytes: 512 bytes (60 slots)

    #[cfg(not(any(
        feature = "_tinyvec-64-bytes",
        feature = "_tinyvec-128-bytes",
        feature = "_tinyvec-256-bytes",
        feature = "_tinyvec-512-bytes",
        feature = "_smallvec-128-bytes",
        feature = "_smallvec-256-bytes"
    )))]
    assert_eq!(
        trace_size, 40,
        "AtTrace should be 40 bytes without tinyvec/smallvec"
    );

    #[cfg(all(
        feature = "_tinyvec-64-bytes",
        not(any(
            feature = "_tinyvec-128-bytes",
            feature = "_tinyvec-256-bytes",
            feature = "_smallvec-128-bytes",
            feature = "_smallvec-256-bytes"
        ))
    ))]
    assert_eq!(
        trace_size, 64,
        "AtTrace with tinyvec-64-bytes should be exactly 64 bytes"
    );

    #[cfg(all(
        any(feature = "_tinyvec-128-bytes", feature = "_smallvec-128-bytes"),
        not(any(feature = "_tinyvec-256-bytes", feature = "_smallvec-256-bytes"))
    ))]
    assert_eq!(
        trace_size, 128,
        "AtTrace with 128-bytes feature should be exactly 128 bytes"
    );

    // smallvec-256-bytes takes precedence over everything
    #[cfg(feature = "_smallvec-256-bytes")]
    assert_eq!(
        trace_size, 256,
        "AtTrace with smallvec-256-bytes should be exactly 256 bytes"
    );

    // tinyvec-256-bytes only if no smallvec and no tinyvec-512
    #[cfg(all(
        feature = "_tinyvec-256-bytes",
        not(any(
            feature = "_smallvec-128-bytes",
            feature = "_smallvec-256-bytes",
            feature = "_tinyvec-512-bytes"
        ))
    ))]
    assert_eq!(
        trace_size, 256,
        "AtTrace with tinyvec-256-bytes should be exactly 256 bytes"
    );

    // tinyvec-512-bytes only if no smallvec features
    #[cfg(all(
        feature = "_tinyvec-512-bytes",
        not(any(feature = "_smallvec-128-bytes", feature = "_smallvec-256-bytes"))
    ))]
    assert_eq!(
        trace_size, 512,
        "AtTrace with tinyvec-512-bytes should be exactly 512 bytes"
    );
}

#[test]
fn test_basic_trace() {
    let err = TestError::NotFound.start_at();
    assert_eq!(*err.error(), TestError::NotFound);
    assert_eq!(err.frame_count(), 1);
    assert!(!err.is_empty());
}

#[test]
fn test_propagation() {
    fn inner() -> Result<(), At<TestError>> {
        Err(TestError::NotFound.start_at())
    }

    fn middle() -> Result<(), At<TestError>> {
        inner().at()
    }

    fn outer() -> Result<(), At<TestError>> {
        middle().at()
    }

    let err = outer().unwrap_err();
    assert_eq!(err.frame_count(), 3);

    // Verify locations are captured
    let locations: Vec<_> = err.locations().collect();
    assert_eq!(locations.len(), 3);

    // All locations should be in this file
    for loc in &locations {
        assert!(loc.file().contains("tests.rs"));
    }
}

#[test]
fn test_result_map_err_at() {
    fn fallible() -> Result<(), &'static str> {
        Err("oops")
    }

    fn wrapper() -> Result<(), At<&'static str>> {
        fallible().map_err(at)?;
        Ok(())
    }

    let err = wrapper().unwrap_err();
    assert_eq!(*err.error(), "oops");
    assert_eq!(err.frame_count(), 1);
}

#[test]
fn test_into_inner() {
    let err = TestError::InvalidInput.start_at();
    let inner = err.into_inner();
    assert_eq!(inner, TestError::InvalidInput);
}

#[test]
fn test_first_last_location() {
    fn level1() -> Result<(), At<TestError>> {
        Err(TestError::NotFound.start_at())
    }

    fn level2() -> Result<(), At<TestError>> {
        level1().at()
    }

    fn level3() -> Result<(), At<TestError>> {
        level2().at()
    }

    let err = level3().unwrap_err();

    let first = err.first_location().unwrap();
    let last = err.last_location().unwrap();

    // First should be from level1, last from level3
    assert!(first.line() < last.line());
}

#[test]
fn test_display_debug() {
    let err = TestError::NotFound.start_at();

    // Display should just show the error
    let display = alloc::format!("{}", err);
    assert_eq!(display, "not found");

    // Debug should include trace
    let debug = alloc::format!("{:?}", err);
    assert!(debug.contains("NotFound"));
    assert!(debug.contains("at"));
    assert!(debug.contains("tests.rs"));
}

#[test]
fn test_no_trace() {
    let err: At<TestError> = At::wrap(TestError::NotFound);
    assert_eq!(err.frame_count(), 0);
    assert!(err.is_empty());
    assert!(err.first_location().is_none());
    assert!(err.last_location().is_none());
}

#[test]
fn test_from_impl() {
    let err: At<TestError> = TestError::NotFound.into();
    assert_eq!(*err.error(), TestError::NotFound);
    assert!(err.is_empty()); // From doesn't add trace
}

#[test]
fn test_error_mut() {
    #[derive(Debug)]
    struct MutableError {
        count: u32,
    }

    let mut err = at(MutableError { count: 0 });
    err.error_mut().count = 42;
    assert_eq!(err.error().count, 42);
}

#[test]
fn test_larger_error_type() {
    #[derive(Debug)]
    #[allow(dead_code)]
    struct LargeError {
        message: String,
        code: u64,
        data: [u8; 32],
    }

    let err = at(LargeError {
        message: String::from("test"),
        code: 42,
        data: [0; 32],
    });

    assert_eq!(err.frame_count(), 1);
    assert_eq!(err.error().code, 42);
}

#[test]
fn test_at_str() {
    let err = TestError::NotFound.start_at().at_str("while fetching user");
    assert_eq!(err.frame_count(), 1); // same line = one location with context
    // Use contexts() to find text context
    let text = err.contexts().find_map(|c| c.as_text());
    assert_eq!(text, Some("while fetching user"));
}

#[test]
fn test_at_fn_captures_function_name() {
    fn my_function_name() -> At<TestError> {
        // at() creates first frame, at_fn() creates second with function name
        at(TestError::NotFound).at_fn(|| {})
    }

    let err = my_function_name();
    assert_eq!(err.frame_count(), 2); // at() + at_fn() = 2 frames

    // The function name should appear in the debug output
    let debug = alloc::format!("{:?}", err);
    assert!(
        debug.contains("my_function_name"),
        "Debug output should contain function name: {}",
        debug
    );
}

#[test]
fn test_at_fn_adds_frame() {
    fn inner() -> Result<(), At<TestError>> {
        Err(at(TestError::NotFound))
    }

    fn outer() -> Result<(), At<TestError>> {
        inner().at_fn(|| {})
    }

    let err = outer().unwrap_err();
    assert_eq!(err.frame_count(), 2); // at() + at_fn() = 2 frames

    let debug = alloc::format!("{:?}", err);
    assert!(
        debug.contains("outer"),
        "Should capture outer function name"
    );
}

#[test]
fn test_at_named_adds_frame_with_label() {
    fn inner() -> Result<(), At<TestError>> {
        Err(at(TestError::NotFound))
    }

    fn outer() -> Result<(), At<TestError>> {
        inner().at_named("validation_phase")?;
        Ok(())
    }

    let err = outer().unwrap_err();
    assert_eq!(err.frame_count(), 2); // at() + at_named() = 2 frames

    let debug = alloc::format!("{:?}", err);
    assert!(
        debug.contains("validation_phase"),
        "Should contain custom label: {}",
        debug
    );
}

#[test]
fn test_str_propagation() {
    fn inner() -> Result<(), At<TestError>> {
        Err(TestError::NotFound.start_at())
    }

    fn outer() -> Result<(), At<TestError>> {
        // at_str adds context to last frame, use .at() first if you want a new frame
        inner().at_str("during initialization")?;
        Ok(())
    }

    let err = outer().unwrap_err();
    assert_eq!(err.frame_count(), 1); // at_str doesn't add new frame
    let text = err.contexts().find_map(|c| c.as_text());
    assert_eq!(text, Some("during initialization"));
}

#[test]
fn test_map_err_at_with_context() {
    fn fallible() -> Result<(), &'static str> {
        Err("oops")
    }

    fn wrapper() -> Result<(), At<&'static str>> {
        fallible().map_err(at).at_str("while doing something")?;
        Ok(())
    }

    let err = wrapper().unwrap_err();
    assert_eq!(*err.error(), "oops");
    let text = err.contexts().find_map(|c| c.as_text());
    assert_eq!(text, Some("while doing something"));
}

#[test]
fn test_debug_with_message() {
    let err = TestError::NotFound.start_at().at_str("context info");
    let debug = alloc::format!("{:?}", err);
    assert!(debug.contains("NotFound"));
    assert!(debug.contains("╰─ context info"));
    assert!(debug.contains("tests.rs"));
}

#[test]
fn test_dbg_ctx_typed() {
    #[derive(Debug)]
    struct RequestInfo {
        user_id: u64,
    }

    let err = TestError::NotFound
        .start_at()
        .at_debug(|| RequestInfo { user_id: 42 });

    assert_eq!(err.frame_count(), 1); // at_debug adds context to existing frame

    // Retrieve typed context
    let mut found = false;
    for ctx in err.contexts() {
        if let Some(req) = ctx.downcast_ref::<RequestInfo>() {
            assert_eq!(req.user_id, 42);
            found = true;
        }
    }
    assert!(found, "should find RequestInfo context");
}

#[test]
fn test_multiple_contexts() {
    fn level1() -> Result<(), At<TestError>> {
        Err(TestError::NotFound.start_at())
    }

    fn level2() -> Result<(), At<TestError>> {
        level1().at_str("in level2")?;
        Ok(())
    }

    fn level3() -> Result<(), At<TestError>> {
        level2().at_str("in level3")?;
        Ok(())
    }

    let err = level3().unwrap_err();

    // at_str adds context to last frame, doesn't create new frames
    assert_eq!(err.frame_count(), 1);

    // Should have 2 context messages (level2 and level3)
    let contexts: Vec<_> = err.contexts().collect();
    assert_eq!(contexts.len(), 2);

    // Most recent first
    assert_eq!(contexts[0].as_text(), Some("in level3"));
    assert_eq!(contexts[1].as_text(), Some("in level2"));
}

#[test]
fn test_context_enum() {
    let text_ctx = AtContext::Text(String::from("hello").into());
    assert_eq!(text_ctx.as_text(), Some("hello"));
    assert!(text_ctx.downcast_ref::<u32>().is_none());

    // Debug context - requires Debug (u32 implements Debug)
    let debug_ctx = AtContext::Debug(Box::new(42u32));
    assert_eq!(debug_ctx.as_text(), None);
    assert_eq!(debug_ctx.downcast_ref::<u32>(), Some(&42));

    // Verify Debug output works
    let debug_str = alloc::format!("{:?}", debug_ctx);
    assert!(debug_str.contains("42"));

    // Display context - requires Display (u32 implements Display)
    let display_ctx = AtContext::Display(Box::new(99u32));
    assert_eq!(display_ctx.as_text(), None);
    assert_eq!(display_ctx.downcast_ref::<u32>(), Some(&99));

    // Verify Display output works
    let display_str = alloc::format!("{}", display_ctx);
    assert!(display_str.contains("99"));

    // is_display should be true for Text and Display
    assert!(text_ctx.is_display());
    assert!(!debug_ctx.is_display());
    assert!(display_ctx.is_display());
}

#[test]
fn test_typed_context_debug_output() {
    #[derive(Debug)]
    #[allow(dead_code)]
    struct MyContext {
        id: u64,
        name: &'static str,
    }

    let err = TestError::NotFound.start_at().at_debug(|| MyContext {
        id: 123,
        name: "test",
    });

    let debug = alloc::format!("{:?}", err);
    // Should contain the Debug output of MyContext
    assert!(debug.contains("MyContext"));
    assert!(debug.contains("123"));
    assert!(debug.contains("test"));
}

#[test]
fn test_ctx_data() {
    // Use a type that has both Display and Debug but we want Display formatting
    let err = TestError::NotFound
        .start_at()
        .at_data(|| "user-friendly message");

    assert_eq!(err.frame_count(), 1); // at_data adds context to existing frame

    // Check that Display formatting is used in output
    let debug = alloc::format!("{:?}", err);
    assert!(debug.contains("╰─ user-friendly message"));

    // Downcast should still work
    let mut found = false;
    for ctx in err.contexts() {
        if ctx.downcast_ref::<&str>().is_some() {
            found = true;
            assert!(ctx.is_display());
        }
    }
    assert!(found, "should find string context");
}

#[test]
fn test_mixed_context_types() {
    #[derive(Debug)]
    #[allow(dead_code)]
    struct DebugInfo {
        code: u32,
    }

    let err = TestError::NotFound
        .start_at()
        .at_str("text message")
        .at_debug(|| DebugInfo { code: 42 })
        .at_data(|| "display message");

    // All context methods add to the existing frame, not new ones
    assert_eq!(err.frame_count(), 1);

    // Should have 3 contexts
    let contexts: Vec<_> = err.contexts().collect();
    assert_eq!(contexts.len(), 3);

    // Most recent first (display, debug, text)
    assert!(contexts[0].is_display()); // display message
    assert!(!contexts[1].is_display()); // DebugInfo (Debug)
    assert!(contexts[2].is_display()); // text message
}

#[test]
fn test_trace_format_structure() {
    // Test that trace format shows locations oldest-first with contexts
    fn level1() -> Result<(), At<TestError>> {
        Err(TestError::NotFound.start_at())
    }

    fn level2() -> Result<(), At<TestError>> {
        level1().at_str("in level2")?;
        Ok(())
    }

    fn level3() -> Result<(), At<TestError>> {
        level2().at_str("in level3")?;
        Ok(())
    }

    let err = level3().unwrap_err();
    let debug = alloc::format!("{:?}", err);

    // Verify structure:
    // - Error header
    assert!(debug.contains("Error: NotFound"));

    // - Locations with contexts
    assert!(debug.contains("╰─ in level2"));
    assert!(debug.contains("╰─ in level3"));

    // - Location lines present (path separator varies by platform)
    assert!(
        debug.contains("at src/tests.rs:") || debug.contains("at src\\tests.rs:"),
        "Debug output should contain location: {}",
        debug
    );

    // Verify order: level2 context before level3 context (oldest first)
    let level2_pos = debug.find("in level2").unwrap();
    let level3_pos = debug.find("in level3").unwrap();
    assert!(
        level2_pos < level3_pos,
        "level2 should appear before level3 (oldest first)"
    );
}

#[test]
fn test_trace_origin_comes_first() {
    fn origin() -> Result<(), At<TestError>> {
        Err(TestError::NotFound.start_at())
    }

    fn wrapper() -> Result<(), At<TestError>> {
        origin().at_str("wrapping")?;
        Ok(())
    }

    let err = wrapper().unwrap_err();
    let debug = alloc::format!("{:?}", err);

    // The first "at" line should be from origin (lower line number)
    // and the context "wrapping" should come after
    let lines: Vec<&str> = debug.lines().collect();

    // Find first "at" line (path separator varies by platform)
    let first_at = lines
        .iter()
        .find(|l| l.contains("at src/tests.rs:") || l.contains("at src\\tests.rs:"))
        .expect("should find location line in debug output");

    // It should be the origin location (before the wrapper's context)
    // The origin .start_at() call will be at a lower line than wrapper's .at_str()
    assert!(
        !first_at.contains("╰─"),
        "First location should be origin without context"
    );
}

#[test]
fn test_partial_eq_compares_error_only() {
    // Same error, different traces
    fn location1() -> At<TestError> {
        TestError::NotFound.start_at()
    }
    fn location2() -> At<TestError> {
        TestError::NotFound.start_at()
    }

    let err1 = location1();
    let err2 = location2();

    // Different traces (different source locations)
    assert!(err1.first_location() != err2.first_location());

    // But errors should be equal because the inner E is equal
    assert_eq!(err1, err2);

    // Different errors should not be equal
    let err3 = at(TestError::InvalidInput);
    assert_ne!(err1, err3);
}

#[test]
fn test_as_ref() {
    let err = at(TestError::NotFound);

    // AsRef gives us &E
    let inner: &TestError = err.as_ref();
    assert_eq!(*inner, TestError::NotFound);

    // Should be same as .error()
    assert!(core::ptr::eq(err.as_ref(), err.error()));
}

#[test]
fn test_map_err_at() {
    #[derive(Debug, PartialEq)]
    struct Error1;
    #[derive(Debug, PartialEq)]
    struct Error2;

    fn inner() -> Result<(), At<Error1>> {
        Err(at(Error1).at_str("inner context"))
    }

    fn outer() -> Result<(), At<Error2>> {
        // map_err_at converts Error1 -> Error2 while preserving trace
        inner().map_err_at(|_| Error2)?;
        Ok(())
    }

    let err = outer().unwrap_err();
    assert_eq!(*err.error(), Error2);
    assert_eq!(err.frame_count(), 1); // Trace preserved
    let text = err.contexts().find_map(|c| c.as_text());
    assert_eq!(text, Some("inner context")); // Context preserved
}

#[test]
fn test_hash_ignores_trace() {
    use core::hash::{Hash, Hasher};

    // Simple hasher for testing
    struct TestHasher(u64);
    impl Hasher for TestHasher {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 = self.0.wrapping_mul(31).wrapping_add(b as u64);
            }
        }
    }

    fn hash_one<T: Hash>(val: &T) -> u64 {
        let mut h = TestHasher(0);
        val.hash(&mut h);
        h.finish()
    }

    // Same error, different traces (different locations)
    fn loc1() -> At<TestError> {
        TestError::NotFound.start_at()
    }
    fn loc2() -> At<TestError> {
        TestError::NotFound.start_at()
    }

    let err1 = loc1();
    let err2 = loc2();

    // Different traces
    assert!(err1.first_location() != err2.first_location());

    // But same hash (because E is the same)
    assert_eq!(hash_one(&err1), hash_one(&err2));

    // Different error = different hash
    let err3 = at(TestError::InvalidInput);
    assert_ne!(hash_one(&err1), hash_one(&err3));
}

// ============================================================================
// Pretty Formatter Tests
// ============================================================================

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_display_contains_error() {
    let err = at(TestError::NotFound).at_str("context");
    let output = alloc::format!("{}", err.display_color());

    // Should contain the error type
    assert!(output.contains("NotFound"), "Output: {}", output);
    // Should contain the context
    assert!(output.contains("context"), "Output: {}", output);
    // Should contain location info
    assert!(output.contains("tests.rs"), "Output: {}", output);
}

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_meta_display_contains_crate_info() {
    let err = crate::at!(TestError::NotFound);
    let output = alloc::format!("{}", err.display_color_meta());

    // Should contain crate info
    assert!(output.contains("whereat"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_display_contains_markup() {
    let err = at(TestError::NotFound).at_str("context");
    let output = alloc::format!("{}", err.display_html());

    // Should have the wrapper div
    assert!(
        output.contains("class=\"whereat-error\""),
        "Output: {}",
        output
    );
    // Should have error header
    assert!(
        output.contains("class=\"error-header\""),
        "Output: {}",
        output
    );
    // Should contain the error
    assert!(output.contains("NotFound"), "Output: {}", output);
    // Should contain context
    assert!(output.contains("context"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_styled_includes_css() {
    let err = at(TestError::NotFound);
    let output = alloc::format!("{}", err.display_html_styled());

    // Should include style tag
    assert!(output.contains("<style>"), "Output: {}", output);
    assert!(output.contains(".whereat-error"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_escapes_special_chars() {
    let err = at(TestError::NotFound)
        .at_string(|| alloc::string::String::from("<script>alert('xss')</script>"));
    let output = alloc::format!("{}", err.display_html());

    // Should escape angle brackets
    assert!(output.contains("&lt;script&gt;"), "Output: {}", output);
    assert!(output.contains("&#39;"), "Output: {}", output);
    // Should NOT contain unescaped script tag
    assert!(!output.contains("<script>"), "Output: {}", output);
}
