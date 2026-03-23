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

    let ptr = size_of::<usize>();

    // At<E> should be sizeof(E) + pointer (pointer to boxed trace)
    assert_eq!(size_of::<Option<Box<AtTrace>>>(), ptr);

    let traced_size = size_of::<At<TestError>>();
    let error_size = size_of::<TestError>();
    let pointer_size = size_of::<Option<Box<AtTrace>>>();

    // Should be error + pointer, with possible padding
    assert!(traced_size <= error_size + pointer_size + ptr); // Allow for alignment
    assert!(traced_size >= error_size + pointer_size);

    // For a 1-byte enum: 1 + padding + pointer = 2*pointer
    // (1 + 7 padding + 8 pointer = 16 on 64-bit, 1 + 3 padding + 4 pointer = 8 on 32-bit)
    assert_eq!(traced_size, 2 * ptr);
}

#[test]
fn test_sizeof_trace() {
    use core::mem::size_of;

    let trace_size = size_of::<AtTrace>();

    // AtTrace size depends on feature flags (sizes shown for 64-bit):
    // - Without tinyvec/smallvec: 112 bytes (InlineVec<4> + crate_info + contexts)
    // - tinyvec-64-bytes: 64 bytes (TinyVec<4 slots> 48 + crate_info 8 + contexts 8)
    // - tinyvec-128-bytes / smallvec-128-bytes: 128 bytes (12 slots)
    // - tinyvec-256-bytes / smallvec-256-bytes: 256 bytes (28 slots)
    // - tinyvec-512-bytes: 512 bytes (60 slots)
    //
    // On 32-bit platforms, all sizes are smaller due to 4-byte pointers.
    // Exact sizes are only asserted on 64-bit; 32-bit asserts the size is
    // at most the 64-bit budget (feature name).

    #[cfg(target_pointer_width = "64")]
    {
        #[cfg(not(any(
            feature = "_tinyvec-64-bytes",
            feature = "_tinyvec-128-bytes",
            feature = "_tinyvec-256-bytes",
            feature = "_tinyvec-512-bytes",
            feature = "_smallvec-128-bytes",
            feature = "_smallvec-256-bytes"
        )))]
        // InlineVec<LocationElem, 4> with 4 inline slots:
        // - len: u8 (1 byte, padded to 8)
        // - inline: [Option<Option<&Location>>; 4] = 64 bytes
        // - heap: Vec<T> = 24 bytes (ptr + len + capacity)
        // Plus crate_info (8) + contexts (8) = 112 bytes total
        assert_eq!(
            trace_size, 112,
            "AtTrace should be 112 bytes with 4 inline slots"
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

    // On 32-bit, just verify the size is at most the 64-bit budget
    #[cfg(target_pointer_width = "32")]
    {
        #[cfg(not(any(
            feature = "_tinyvec-64-bytes",
            feature = "_tinyvec-128-bytes",
            feature = "_tinyvec-256-bytes",
            feature = "_tinyvec-512-bytes",
            feature = "_smallvec-128-bytes",
            feature = "_smallvec-256-bytes"
        )))]
        assert!(
            trace_size <= 112,
            "AtTrace should be <= 112 bytes on 32-bit. Got: {trace_size}"
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
        assert!(
            trace_size <= 64,
            "AtTrace with tinyvec-64-bytes should be <= 64 bytes on 32-bit. Got: {trace_size}"
        );

        #[cfg(all(
            any(feature = "_tinyvec-128-bytes", feature = "_smallvec-128-bytes"),
            not(any(feature = "_tinyvec-256-bytes", feature = "_smallvec-256-bytes"))
        ))]
        assert!(
            trace_size <= 128,
            "AtTrace with 128-bytes feature should be <= 128 bytes on 32-bit. Got: {trace_size}"
        );

        #[cfg(feature = "_smallvec-256-bytes")]
        assert!(
            trace_size <= 256,
            "AtTrace with smallvec-256-bytes should be <= 256 bytes on 32-bit. Got: {trace_size}"
        );

        #[cfg(all(
            feature = "_tinyvec-256-bytes",
            not(any(
                feature = "_smallvec-128-bytes",
                feature = "_smallvec-256-bytes",
                feature = "_tinyvec-512-bytes"
            ))
        ))]
        assert!(
            trace_size <= 256,
            "AtTrace with tinyvec-256-bytes should be <= 256 bytes on 32-bit. Got: {trace_size}"
        );

        #[cfg(all(
            feature = "_tinyvec-512-bytes",
            not(any(feature = "_smallvec-128-bytes", feature = "_smallvec-256-bytes"))
        ))]
        assert!(
            trace_size <= 512,
            "AtTrace with tinyvec-512-bytes should be <= 512 bytes on 32-bit. Got: {trace_size}"
        );
    }
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
fn test_decompose() {
    let err = TestError::InvalidInput.start_at();
    let (inner, trace) = err.decompose();
    assert_eq!(inner, TestError::InvalidInput);
    assert!(trace.is_some());
}

#[test]
#[allow(deprecated)]
fn test_into_inner_deprecated() {
    let err = TestError::InvalidInput.start_at();
    let inner = err.into_inner();
    assert_eq!(inner, TestError::InvalidInput);
}

#[test]
#[allow(deprecated)]
fn test_at_error_deprecated_still_works() {
    let err = at(TestError::NotFound).at_error(core::fmt::Error);
    let output = alloc::format!("{:?}", err);
    assert!(output.contains("caused by"));
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

// ============================================================================
// Depth Limit Tests
// ============================================================================

#[test]
fn test_max_trace_frames_limit() {
    use crate::trace::AT_MAX_FRAMES;

    // Recursive function that adds many frames
    fn add_frames(err: At<TestError>, depth: usize) -> At<TestError> {
        if depth == 0 {
            err
        } else {
            add_frames(err.at(), depth - 1)
        }
    }

    let err = at(TestError::NotFound);
    let err = add_frames(err, AT_MAX_FRAMES + 50);

    // Should be capped at AT_MAX_FRAMES
    assert_eq!(
        err.frame_count(),
        AT_MAX_FRAMES,
        "Frame count should be capped at AT_MAX_FRAMES ({})",
        AT_MAX_FRAMES
    );
}

#[test]
fn test_max_trace_contexts_limit() {
    use crate::trace::AT_MAX_CONTEXTS;

    let mut err = at(TestError::NotFound);

    // Add more contexts than the limit
    for i in 0..(AT_MAX_CONTEXTS + 50) {
        err = err.at_string(|| alloc::format!("context {}", i));
    }

    // Count contexts
    let context_count = err.contexts().count();

    // Should be capped at AT_MAX_CONTEXTS
    assert_eq!(
        context_count, AT_MAX_CONTEXTS,
        "Context count should be capped at AT_MAX_CONTEXTS ({})",
        AT_MAX_CONTEXTS
    );
}

#[test]
fn test_limits_constants_are_128() {
    use crate::trace::{AT_MAX_CONTEXTS, AT_MAX_FRAMES};

    assert_eq!(AT_MAX_FRAMES, 128);
    assert_eq!(AT_MAX_CONTEXTS, 128);
}

// ============================================================================
// Additional Coverage Tests
// ============================================================================

#[test]
fn test_from_parts() {
    use crate::trace::AtTrace;

    let mut trace = AtTrace::new();
    let _ = trace.try_push(core::panic::Location::caller());
    let err = At::<TestError>::from_parts(TestError::NotFound, trace);
    assert_eq!(err.frame_count(), 1);
    assert_eq!(*err.error(), TestError::NotFound);
}

#[test]
fn test_take_trace_and_set_trace() {
    let mut err = at(TestError::NotFound).at_str("context");
    assert_eq!(err.frame_count(), 1);

    let trace = err.take_trace();
    assert!(trace.is_some());
    assert_eq!(err.frame_count(), 0);

    err.set_trace(trace.unwrap());
    assert_eq!(err.frame_count(), 1);
}

#[test]
fn test_at_string_on_at() {
    let err = at(TestError::NotFound).at_string(|| alloc::format!("dynamic {}", 42));
    let text = err.contexts().find_map(|c| c.as_text());
    assert_eq!(text, Some("dynamic 42"));
}

#[test]
fn test_full_trace_display() {
    let err = at(TestError::NotFound)
        .at_str("loading config")
        .at()
        .at_str("initializing");

    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("not found"));
    assert!(output.contains("loading config"));
    assert!(output.contains("initializing"));
    assert!(output.contains("at "));
}

#[test]
fn test_full_trace_with_skipped() {
    let err = at(TestError::NotFound).at_skipped_frames().at();
    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("[...]"));
}

#[test]
fn test_full_trace_with_fn_name() {
    fn my_function() -> At<TestError> {
        at(TestError::NotFound).at_fn(|| {})
    }
    let err = my_function();
    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("in "));
    assert!(output.contains("my_function"));
}

#[test]
fn test_full_trace_with_error_context() {
    #[derive(Debug)]
    struct Inner(&'static str);
    impl fmt::Display for Inner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "inner: {}", self.0)
        }
    }
    impl core::error::Error for Inner {}

    let err = at(TestError::NotFound).at_aside_error(Inner("root cause"));
    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("caused by: inner: root cause"));
}

#[test]
fn test_full_trace_with_debug_and_display_data() {
    #[derive(Debug)]
    struct DbgData(#[allow(dead_code)] u32);

    struct DispData(u32);
    impl fmt::Display for DispData {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "val={}", self.0)
        }
    }

    let err = at(TestError::NotFound)
        .at_debug(|| DbgData(42))
        .at_data(|| DispData(99));

    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("DbgData"));
    assert!(output.contains("42"));
    assert!(output.contains("val=99"));
}

#[test]
fn test_last_error_trace_display() {
    let err = at(TestError::NotFound)
        .at_str("this should not appear")
        .at();

    let output = alloc::format!("{}", err.last_error_trace());
    assert!(output.contains("not found"));
    assert!(output.contains("at "));
    assert!(!output.contains("this should not appear"));
}

#[test]
fn test_last_error_trace_with_skipped() {
    let err = at(TestError::NotFound).at_skipped_frames();
    let output = alloc::format!("{}", err.last_error_trace());
    assert!(output.contains("[...]"));
}

#[test]
fn test_last_error_display() {
    let err = at(TestError::NotFound).at_str("context");
    let output = alloc::format!("{}", err.last_error());
    assert_eq!(output, "not found");
}

#[test]
fn test_display_with_meta_no_trace() {
    let err: At<TestError> = At::wrap(TestError::NotFound);
    let output = alloc::format!("{}", err.display_with_meta());
    assert!(output.contains("NotFound"));
}

#[test]
fn test_display_with_meta_skipped() {
    let err = at(TestError::NotFound)
        .set_crate_info(crate::at_crate_info())
        .at_skipped_frames()
        .at();
    let output = alloc::format!("{}", err.display_with_meta());
    assert!(output.contains("[...]"));
}

#[test]
fn test_display_with_meta_contexts() {
    let err = at(TestError::NotFound)
        .set_crate_info(crate::at_crate_info())
        .at_str("text ctx")
        .at_fn(|| {})
        .at_debug(|| 42u32)
        .at_data(|| "display data")
        .at_aside_error(core::fmt::Error);

    let output = alloc::format!("{}", err.display_with_meta());
    assert!(output.contains("text ctx"));
    assert!(output.contains("in "));
    assert!(output.contains("42"));
    assert!(output.contains("display data"));
    assert!(output.contains("caused by"));
}

#[test]
fn test_into_traceable() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "my error")
        }
    }

    let at_err = at(TestError::NotFound).at_str("context");
    let my_err: MyErr = at_err.into_traceable(|_| MyErr {
        trace: AtTrace::new(),
    });
    assert!(my_err.trace().unwrap().frame_count() >= 1);
}

#[test]
fn test_into_traceable_no_trace() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "my error")
        }
    }

    let at_err: At<TestError> = At::wrap(TestError::NotFound);
    let my_err: MyErr = at_err.into_traceable(|_| MyErr {
        trace: AtTrace::new(),
    });
    assert_eq!(my_err.trace().unwrap().frame_count(), 0);
}

#[test]
fn test_debug_with_skipped_frames() {
    let err = at(TestError::NotFound).at_skipped_frames().at();
    let debug = alloc::format!("{:?}", err);
    assert!(debug.contains("[...]"));
}

#[test]
fn test_context_type_name() {
    let ctx = AtContext::Debug(Box::new(42u32));
    assert!(ctx.type_name().unwrap().contains("u32"));

    let ctx = AtContext::Display(Box::new(42u32));
    assert!(ctx.type_name().unwrap().contains("u32"));

    let text = AtContext::Text("hello".into());
    assert!(text.type_name().is_none());
}

#[test]
fn test_context_function_name() {
    let ctx = AtContext::FunctionName("my_func");
    assert_eq!(ctx.as_function_name(), Some("my_func"));
    assert!(ctx.is_function_name());
    assert!(ctx.as_text().is_none());
}

#[test]
fn test_context_crate_info() {
    let info = crate::at_crate_info();
    let ctx = AtContext::Crate(info);
    assert!(ctx.as_crate_info().is_some());
    assert!(core::ptr::eq(ctx.as_crate_info().unwrap(), info));
    assert!(ctx.is_crate_boundary());
    assert!(ctx.as_text().is_none());
}

#[test]
fn test_context_error() {
    let ctx = AtContext::Error(Box::new(core::fmt::Error));
    assert!(ctx.is_error());
    assert!(ctx.as_error().is_some());
    assert!(ctx.is_display());

    // Verify it's not function name or crate boundary
    assert!(!ctx.is_function_name());
    assert!(!ctx.is_crate_boundary());
}

#[test]
fn test_context_debug_display_fmt() {
    let fn_ctx = AtContext::FunctionName("foo");
    assert_eq!(alloc::format!("{:?}", fn_ctx), "in foo");
    assert_eq!(alloc::format!("{}", fn_ctx), "in foo");

    let crate_ctx = AtContext::Crate(crate::at_crate_info());
    let debug = alloc::format!("{:?}", crate_ctx);
    assert!(debug.contains("[crate:"));
    let display = alloc::format!("{}", crate_ctx);
    assert!(display.contains("[crate:"));

    let err_ctx = AtContext::Error(Box::new(core::fmt::Error));
    let debug = alloc::format!("{:?}", err_ctx);
    assert!(debug.contains("caused by"));
    let display = alloc::format!("{}", err_ctx);
    assert!(display.contains("caused by"));
}

#[test]
fn test_context_ref_display_debug() {
    use crate::context::AtContextRef;

    let ctx = AtContext::Text("hello".into());
    let ctx_ref = AtContextRef { inner: &ctx };
    assert_eq!(alloc::format!("{}", ctx_ref), "hello");
    assert_eq!(alloc::format!("{:?}", ctx_ref), "\"hello\"");
}

#[test]
fn test_context_ref_is_methods() {
    use crate::context::AtContextRef;

    let fn_ctx = AtContext::FunctionName("foo");
    let ref1 = AtContextRef { inner: &fn_ctx };
    assert!(ref1.is_function_name());
    assert_eq!(ref1.as_function_name(), Some("foo"));

    let crate_ctx = AtContext::Crate(crate::at_crate_info());
    let ref2 = AtContextRef { inner: &crate_ctx };
    assert!(ref2.is_crate_boundary());
    assert!(ref2.as_crate_info().is_some());

    let err_ctx = AtContext::Error(Box::new(core::fmt::Error));
    let ref3 = AtContextRef { inner: &err_ctx };
    assert!(ref3.is_error());
    assert!(ref3.as_error().is_some());
}

#[test]
fn test_context_ref_type_name() {
    use crate::context::AtContextRef;

    let dbg_ctx = AtContext::Debug(Box::new(42u32));
    let ref1 = AtContextRef { inner: &dbg_ctx };
    assert!(ref1.type_name().unwrap().contains("u32"));

    let text_ctx = AtContext::Text("hello".into());
    let ref2 = AtContextRef { inner: &text_ctx };
    assert!(ref2.type_name().is_none());
}

#[test]
fn test_trace_default() {
    use crate::trace::AtTrace;

    let trace: AtTrace = Default::default();
    assert!(trace.is_empty());
    assert_eq!(trace.frame_count(), 0);
}

#[test]
fn test_trace_take() {
    use crate::trace::AtTrace;

    let mut trace = AtTrace::capture();
    assert_eq!(trace.frame_count(), 1);

    let taken = trace.take();
    assert_eq!(taken.frame_count(), 1);
    assert_eq!(trace.frame_count(), 0);
}

#[test]
fn test_trace_append() {
    use crate::trace::AtTrace;

    let mut trace1 = AtTrace::capture();
    let trace2 = AtTrace::capture();
    trace1.append(trace2);
    assert_eq!(trace1.frame_count(), 2);
}

#[test]
fn test_trace_prepend() {
    use crate::trace::AtTrace;

    let mut trace1 = AtTrace::capture();
    let trace2 = AtTrace::capture();
    trace1.prepend(trace2);
    assert_eq!(trace1.frame_count(), 2);
}

#[test]
fn test_trace_boxed_from_trace() {
    use crate::trace::{AtTrace, AtTraceBoxed};

    let trace = AtTrace::capture();
    let boxed = AtTraceBoxed::from(trace);
    assert!(!boxed.is_empty());
    assert_eq!(boxed.frame_count(), 1);

    let empty = AtTrace::new();
    let boxed_empty = AtTraceBoxed::from(empty);
    assert!(boxed_empty.is_empty());
}

#[test]
fn test_trace_boxed_into_option() {
    use crate::trace::{AtTrace, AtTraceBoxed};

    let boxed = AtTraceBoxed::from(AtTrace::capture());
    let opt: Option<AtTrace> = boxed.into();
    assert!(opt.is_some());

    let empty = AtTraceBoxed::new();
    let opt: Option<AtTrace> = empty.into();
    assert!(opt.is_none());
}

#[test]
fn test_trace_boxed_debug() {
    use crate::trace::{AtTrace, AtTraceBoxed};

    let empty = AtTraceBoxed::new();
    let debug = alloc::format!("{:?}", empty);
    assert!(debug.contains("empty"));

    let boxed = AtTraceBoxed::from(AtTrace::capture());
    let debug = alloc::format!("{:?}", boxed);
    assert!(!debug.contains("empty"));
}

#[test]
fn test_trace_boxed_set_empty() {
    use crate::trace::{AtTrace, AtTraceBoxed};

    let mut boxed = AtTraceBoxed::from(AtTrace::capture());
    assert!(!boxed.is_empty());

    // Setting an empty trace should clear
    boxed.set(AtTrace::new());
    assert!(boxed.is_empty());
}

#[test]
fn test_trace_boxed_crate_info() {
    use crate::trace::{AtTrace, AtTraceBoxed};

    let mut trace = AtTrace::capture();
    trace.set_crate_info(crate::at_crate_info());
    let boxed = AtTraceBoxed::from(trace);
    assert!(boxed.crate_info().is_some());

    let empty = AtTraceBoxed::new();
    assert!(empty.crate_info().is_none());
}

#[test]
fn test_at_frame_has_contexts() {
    let err = at(TestError::NotFound).at_str("ctx").at();
    let frames: alloc::vec::Vec<_> = err.frames().collect();
    assert!(frames[0].has_contexts());
    assert!(!frames[1].has_contexts());
}

#[test]
fn test_at_frame_debug() {
    let err = at(TestError::NotFound).at_str("ctx").at_skipped_frames();
    let frames: alloc::vec::Vec<_> = err.frames().collect();

    let debug0 = alloc::format!("{:?}", frames[0]);
    assert!(debug0.contains("at "));

    let debug1 = alloc::format!("{:?}", frames[1]);
    assert_eq!(debug1, "[...]");
}

#[test]
fn test_crate_info_builder_all_methods() {
    use crate::AtCrateInfo;

    let info = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/org/repo"))
        .commit(Some("abc123"))
        .path(Some("crates/lib/"))
        .module("test_mod")
        .meta(&[("key", "value")])
        .link_format(crate::GITHUB_LINK_FORMAT)
        .build();

    assert_eq!(info.name(), "test");
    assert_eq!(info.repo(), Some("https://github.com/org/repo"));
    assert_eq!(info.commit(), Some("abc123"));
    assert_eq!(info.crate_path(), Some("crates/lib/"));
    assert_eq!(info.module(), "test_mod");
    assert_eq!(info.meta(), &[("key", "value")]);
    assert_eq!(info.link_format(), crate::GITHUB_LINK_FORMAT);
    assert_eq!(info.get_meta("key"), Some("value"));
    assert_eq!(info.get_meta("missing"), None);
}

#[test]
fn test_crate_info_builder_default() {
    use crate::crate_info::AtCrateInfoBuilder;

    let builder: AtCrateInfoBuilder = Default::default();
    let info = builder.build();
    assert_eq!(info.name(), "");
    assert!(info.repo().is_none());
}

#[test]
fn test_crate_info_owned_methods() {
    use crate::AtCrateInfo;

    let info = AtCrateInfo::builder()
        .name_owned("owned-name".into())
        .repo_owned(Some("https://example.com".into()))
        .commit_owned(Some("abc123".into()))
        .path_owned(Some("crates/lib/".into()))
        .module_owned("my_mod".into())
        .meta_owned(alloc::vec![("k".into(), "v".into())])
        .link_format_owned("{repo}/{file}".into())
        .build();

    assert_eq!(info.name(), "owned-name");
    assert_eq!(info.repo(), Some("https://example.com"));
    assert_eq!(info.commit(), Some("abc123"));
    assert_eq!(info.crate_path(), Some("crates/lib/"));
    assert_eq!(info.module(), "my_mod");
    assert_eq!(info.get_meta("k"), Some("v"));
    assert_eq!(info.link_format(), "{repo}/{file}");
}

#[test]
fn test_crate_info_owned_none_variants() {
    use crate::AtCrateInfo;

    let info = AtCrateInfo::builder()
        .repo_owned(None)
        .commit_owned(None)
        .path_owned(None)
        .build();

    assert!(info.repo().is_none());
    assert!(info.commit().is_none());
    assert!(info.crate_path().is_none());
}

#[test]
fn test_link_format_auto_detect() {
    use crate::AtCrateInfo;

    let github = AtCrateInfo::builder()
        .repo(Some("https://github.com/org/repo"))
        .link_format_auto()
        .build();
    assert_eq!(github.link_format(), crate::GITHUB_LINK_FORMAT);

    let gitlab = AtCrateInfo::builder()
        .repo(Some("https://gitlab.com/org/repo"))
        .link_format_auto()
        .build();
    assert_eq!(gitlab.link_format(), crate::GITLAB_LINK_FORMAT);

    let gitea = AtCrateInfo::builder()
        .repo(Some("https://gitea.example.com/org/repo"))
        .link_format_auto()
        .build();
    assert_eq!(gitea.link_format(), crate::GITEA_LINK_FORMAT);

    let forgejo = AtCrateInfo::builder()
        .repo(Some("https://forgejo.example.com/org/repo"))
        .link_format_auto()
        .build();
    assert_eq!(forgejo.link_format(), crate::GITEA_LINK_FORMAT);

    let codeberg = AtCrateInfo::builder()
        .repo(Some("https://codeberg.org/org/repo"))
        .link_format_auto()
        .build();
    assert_eq!(codeberg.link_format(), crate::GITEA_LINK_FORMAT);

    let bitbucket = AtCrateInfo::builder()
        .repo(Some("https://bitbucket.org/org/repo"))
        .link_format_auto()
        .build();
    assert_eq!(bitbucket.link_format(), crate::BITBUCKET_LINK_FORMAT);

    let unknown = AtCrateInfo::builder()
        .repo(Some("https://example.com/repo"))
        .link_format_auto()
        .build();
    assert_eq!(unknown.link_format(), crate::GITHUB_LINK_FORMAT);

    let no_repo = AtCrateInfo::builder().link_format_auto().build();
    assert_eq!(no_repo.link_format(), crate::GITHUB_LINK_FORMAT);
}

#[test]
fn test_traceable_at_string() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_string(|| alloc::format!("dyn {}", 1));
    let ctx: alloc::vec::Vec<_> = err
        .trace()
        .unwrap()
        .contexts()
        .filter_map(|c| c.as_text().map(alloc::string::ToString::to_string))
        .collect();
    assert!(ctx.iter().any(|s| s == "dyn 1"));
}

#[test]
fn test_traceable_at_data_and_debug() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_data(|| 42u32)
    .at_debug(|| "dbg_val");

    assert_eq!(err.trace().unwrap().contexts().count(), 2);
}

#[test]
fn test_traceable_at_error() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_aside_error(core::fmt::Error);

    let has_error = err.trace().unwrap().contexts().any(|c| c.is_error());
    assert!(has_error);
}

#[test]
fn test_traceable_at_crate() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_crate(crate::at_crate_info());

    assert!(err.trace().unwrap().crate_info().is_some());
}

#[test]
fn test_traceable_at_fn_and_named() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_fn(|| {});
    assert_eq!(err.trace().unwrap().frame_count(), 2);

    let err2 = MyErr {
        trace: AtTrace::capture(),
    }
    .at_named("step1");
    assert_eq!(err2.trace().unwrap().frame_count(), 2);
}

#[test]
fn test_traceable_at_skipped() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_skipped_frames();
    assert_eq!(err.trace().unwrap().frame_count(), 2);
}

#[test]
fn test_traceable_map_traceable() {
    use crate::trace::{AtTrace, AtTraceable};

    struct ErrA {
        trace: AtTrace,
    }
    impl AtTraceable for ErrA {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "A")
        }
    }
    struct ErrB {
        trace: AtTrace,
    }
    impl AtTraceable for ErrB {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "B")
        }
    }

    let a = ErrA {
        trace: AtTrace::capture(),
    }
    .at_str("context");
    let b: ErrB = a.map_traceable(|_| ErrB {
        trace: AtTrace::new(),
    });
    assert!(b.trace().unwrap().frame_count() >= 1);
}

#[test]
fn test_traceable_into_at() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_str("context");
    let at_err: At<&str> = err.into_at(|_| "converted");
    assert_eq!(*at_err.error(), "converted");
    assert!(at_err.frame_count() >= 1);
}

#[test]
fn test_traceable_full_trace_format() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "my error")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_str("context msg")
    .at_fn(|| {})
    .at_debug(|| 42u32)
    .at_data(|| "disp data")
    .at_aside_error(core::fmt::Error);

    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("my error"));
    assert!(output.contains("context msg"));
    assert!(output.contains("in "));
}

#[test]
fn test_traceable_last_error_trace_format() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "my error")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_str("should not appear")
    .at_skipped_frames();

    let output = alloc::format!("{}", err.last_error_trace());
    assert!(output.contains("my error"));
    assert!(output.contains("[...]"));
    assert!(!output.contains("should not appear"));
}

#[test]
fn test_traceable_last_error_format() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "just the message")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    };
    let output = alloc::format!("{}", err.last_error());
    assert_eq!(output, "just the message");
}

#[test]
fn test_result_at_ext_ok_paths() {
    type R = Result<i32, At<TestError>>;
    assert_eq!(R::Ok(42).at().unwrap(), 42);
    assert_eq!(R::Ok(42).at_str("msg").unwrap(), 42);
    assert_eq!(
        R::Ok(42)
            .at_string(|| alloc::string::String::from("dyn"))
            .unwrap(),
        42
    );
    assert_eq!(R::Ok(42).at_data(|| 1u32).unwrap(), 42);
    assert_eq!(R::Ok(42).at_debug(|| 1u32).unwrap(), 42);
    assert_eq!(R::Ok(42).at_aside_error(core::fmt::Error).unwrap(), 42);
    assert_eq!(R::Ok(42).at_crate(crate::at_crate_info()).unwrap(), 42);
    assert_eq!(R::Ok(42).at_fn(|| {}).unwrap(), 42);
    assert_eq!(R::Ok(42).at_named("step").unwrap(), 42);
    assert_eq!(R::Ok(42).map_err_at(|_| ()).unwrap(), 42);
}

#[test]
fn test_result_at_ext_err_paths() {
    fn make_err() -> Result<(), At<TestError>> {
        Err(at(TestError::NotFound))
    }

    // Test error paths for methods not already covered
    let _ = make_err().at_data(|| 42u32).unwrap_err();
    let _ = make_err().at_debug(|| 42u32).unwrap_err();
    let _ = make_err().at_aside_error(core::fmt::Error).unwrap_err();
    let _ = make_err().at_crate(crate::at_crate_info()).unwrap_err();
    let _ = make_err().at_fn(|| {}).unwrap_err();
    let _ = make_err().at_named("step").unwrap_err();
}

#[test]
fn test_result_at_traceable_ext_all() {
    use crate::ResultAtTraceableExt;
    use crate::trace::{AtTrace, AtTraceable};

    #[derive(Debug)]
    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    #[allow(clippy::result_large_err)]
    fn make_err() -> Result<i32, MyErr> {
        Err(MyErr {
            trace: AtTrace::capture(),
        })
    }

    #[allow(clippy::result_large_err)]
    fn make_ok() -> Result<i32, MyErr> {
        Ok(42)
    }

    // Ok paths
    assert_eq!(make_ok().at().unwrap(), 42);
    assert_eq!(make_ok().at_str("msg").unwrap(), 42);
    assert_eq!(
        make_ok()
            .at_string(|| alloc::string::String::from("x"))
            .unwrap(),
        42
    );
    assert_eq!(make_ok().at_data(|| 1u32).unwrap(), 42);
    assert_eq!(make_ok().at_debug(|| 1u32).unwrap(), 42);
    assert_eq!(make_ok().at_aside_error(core::fmt::Error).unwrap(), 42);
    assert_eq!(make_ok().at_crate(crate::at_crate_info()).unwrap(), 42);
    assert_eq!(make_ok().at_fn(|| {}).unwrap(), 42);
    assert_eq!(make_ok().at_named("step").unwrap(), 42);

    // Err paths
    let _ = make_err().at().unwrap_err();
    let _ = make_err().at_str("msg").unwrap_err();
    let _ = make_err()
        .at_string(|| alloc::string::String::from("x"))
        .unwrap_err();
    let _ = make_err().at_data(|| 1u32).unwrap_err();
    let _ = make_err().at_debug(|| 1u32).unwrap_err();
    let _ = make_err().at_aside_error(core::fmt::Error).unwrap_err();
    let _ = make_err().at_crate(crate::at_crate_info()).unwrap_err();
    let _ = make_err().at_fn(|| {}).unwrap_err();
    let _ = make_err().at_named("step").unwrap_err();
}

#[test]
fn test_crate_info_get_meta_const() {
    use crate::AtCrateInfo;

    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .meta(&[("team", "platform"), ("env", "prod")])
        .build();

    assert_eq!(INFO.get_meta("team"), Some("platform"));
    assert_eq!(INFO.get_meta("env"), Some("prod"));
    assert_eq!(INFO.get_meta("missing"), None);
    assert_eq!(INFO.get_meta("tea"), None); // Different length
}

#[test]
fn test_traceable_full_trace_with_crate_boundary() {
    use crate::AtCrateInfo;
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    static C1: AtCrateInfo = AtCrateInfo::builder().name("crate-a").build();
    static C2: AtCrateInfo = AtCrateInfo::builder().name("crate-b").build();

    let mut trace = AtTrace::capture();
    trace.set_crate_info(&C1);
    let err = MyErr { trace }.at_crate(&C2).at();

    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("crate-a"));
    assert!(output.contains("crate-b"));
}

#[test]
fn test_traceable_full_trace_no_trace() {
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr;
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            panic!("should not be called");
        }
        fn trace(&self) -> Option<&AtTrace> {
            None
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let err = MyErr;
    let output = alloc::format!("{}", err.full_trace());
    assert_eq!(output, "err");

    let output2 = alloc::format!("{}", err.last_error_trace());
    assert_eq!(output2, "err");
}

// ============================================================================
// Additional coverage tests — format.rs, at.rs, context.rs, trace.rs, ext.rs
// ============================================================================

#[test]
fn test_at_locations_iterator() {
    let err = at(TestError::NotFound).at_str("msg").at();
    let locs: alloc::vec::Vec<_> = err.locations().collect();
    assert!(locs.len() >= 2);
    for loc in &locs {
        assert!(loc.file().contains("tests.rs"));
    }
}

#[test]
fn test_at_debug_with_skipped_frames() {
    // At<E> Debug impl with a skipped frame marker (None location)
    let mut err = at(TestError::NotFound).at_str("ctx");
    err = err.at_skipped_frames();
    let output = alloc::format!("{:?}", err);
    assert!(output.contains("[...]"));
    assert!(output.contains("NotFound"));
    assert!(output.contains("ctx"));
}

#[test]
fn test_full_trace_display_with_nested_error_chain() {
    // Exercise lines 1101-1110 in at.rs (nested error source chain)
    use core::error::Error;

    #[derive(Debug)]
    struct Inner;
    impl fmt::Display for Inner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "inner error")
        }
    }
    impl Error for Inner {}

    #[derive(Debug)]
    struct Outer(Inner);
    impl fmt::Display for Outer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "outer error")
        }
    }
    impl Error for Outer {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    let err = at(TestError::NotFound).at_aside_error(Outer(Inner));
    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("caused by: outer error"));
    assert!(output.contains("caused by: inner error"));
}

#[test]
fn test_full_trace_display_with_debug_and_display_ctx() {
    // Exercise lines 1112-1113 (the else branch: non-text, non-fn, non-error contexts)
    let err = at(TestError::NotFound)
        .at_data(|| "display_val")
        .at_debug(|| 99u32);
    let output = alloc::format!("{}", err.full_trace());
    // Display contexts fall through to the else branch, which calls ctx Display
    assert!(output.contains("display_val") || output.contains("99"));
}

#[test]
fn test_display_with_meta_skipped_frames() {
    // Exercise line 910 (display_with_meta None branch)
    let err = at(TestError::NotFound).at_skipped_frames();
    let output = alloc::format!("{}", err.display_with_meta());
    assert!(output.contains("[...]"));
}

#[test]
fn test_display_with_meta_with_link_template() {
    // Exercise line 988 (write_location_meta with link_template)
    use crate::AtCrateInfo;

    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test-crate")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc123"))
        .path(Some("src"))
        .build();

    let err = At::wrap(TestError::NotFound).set_crate_info(&INFO).at();
    let output = alloc::format!("{}", err.display_with_meta());
    // Should contain a link with file/line substituted
    assert!(output.contains("github.com"), "Output: {}", output);
}

#[test]
fn test_context_downcast_ref() {
    use crate::context::AtContext;

    // Debug variant — downcast succeeds
    let ctx = AtContext::Debug(Box::new(42u32));
    assert_eq!(ctx.downcast_ref::<u32>(), Some(&42u32));
    assert_eq!(ctx.downcast_ref::<i32>(), None);

    // Display variant — downcast succeeds
    let ctx = AtContext::Display(Box::new(alloc::string::String::from("hello")));
    assert_eq!(
        ctx.downcast_ref::<alloc::string::String>(),
        Some(&alloc::string::String::from("hello"))
    );

    // Text — always None
    let ctx = AtContext::Text(alloc::borrow::Cow::Borrowed("hi"));
    assert_eq!(ctx.downcast_ref::<alloc::string::String>(), None);

    // FunctionName — always None
    let ctx = AtContext::FunctionName("fn_name");
    assert_eq!(ctx.downcast_ref::<&str>(), None);

    // Error — always None
    let ctx = AtContext::Error(Box::new(core::fmt::Error));
    assert_eq!(ctx.downcast_ref::<core::fmt::Error>(), None);
}

#[test]
fn test_context_type_name_all_variants() {
    use crate::context::AtContext;

    // Debug — has type_name
    let ctx = AtContext::Debug(Box::new(42u32));
    assert!(ctx.type_name().is_some());

    // Display — has type_name
    let ctx = AtContext::Display(Box::new(alloc::string::String::from("x")));
    assert!(ctx.type_name().is_some());

    // Text — None
    let ctx = AtContext::Text(alloc::borrow::Cow::Borrowed("hi"));
    assert!(ctx.type_name().is_none());
}

#[test]
fn test_context_display_fmt_all_variants() {
    use crate::context::AtContext;

    // Display variant in Debug fmt
    let ctx = AtContext::Display(Box::new(42u32));
    let dbg_output = alloc::format!("{:?}", ctx);
    assert!(dbg_output.contains("42"), "Output: {}", dbg_output);

    // Display variant in Display fmt
    let disp_output = alloc::format!("{}", ctx);
    assert!(disp_output.contains("42"), "Output: {}", disp_output);

    // Crate variant
    use crate::AtCrateInfo;
    static INFO: AtCrateInfo = AtCrateInfo::builder().name("my-crate").build();
    let ctx = AtContext::Crate(&INFO);
    let dbg_output = alloc::format!("{:?}", ctx);
    assert!(dbg_output.contains("my-crate"), "Output: {}", dbg_output);
    let disp_output = alloc::format!("{}", ctx);
    assert!(disp_output.contains("my-crate"), "Output: {}", disp_output);

    // Error variant
    let ctx = AtContext::Error(Box::new(core::fmt::Error));
    let dbg_output = alloc::format!("{:?}", ctx);
    assert!(dbg_output.contains("caused by:"), "Output: {}", dbg_output);
    let disp_output = alloc::format!("{}", ctx);
    assert!(
        disp_output.contains("caused by:"),
        "Output: {}",
        disp_output
    );
}

#[test]
fn test_trace_try_add_crate_boundary_same_crate() {
    // Exercise line 296/300 — same crate info ptr → no-op
    use crate::AtCrateInfo;
    use crate::trace::AtTrace;

    static INFO: AtCrateInfo = AtCrateInfo::builder().name("same").build();

    let mut trace = AtTrace::capture();
    trace.set_crate_info(&INFO);
    let frames_before = trace.frame_count();
    // Add same crate info again — should be a no-op (no new context)
    trace.try_add_crate_boundary(core::panic::Location::caller(), &INFO);
    assert_eq!(trace.frame_count(), frames_before);
}

#[test]
fn test_trace_try_add_crate_boundary_different_crate() {
    // Exercise line 303 — different crate info → adds context
    use crate::AtCrateInfo;
    use crate::trace::AtTrace;

    static C1: AtCrateInfo = AtCrateInfo::builder().name("crate-a").build();
    static C2: AtCrateInfo = AtCrateInfo::builder().name("crate-b").build();

    let mut trace = AtTrace::capture();
    trace.set_crate_info(&C1);
    trace.try_add_crate_boundary(core::panic::Location::caller(), &C2);
    // Should have a crate boundary context now
    let _output = alloc::format!("{:?}", trace);
    // The boundary context is stored — visible when formatting At
    assert!(trace.crate_info().is_some());
}

#[test]
fn test_trace_pop_first_with_contexts() {
    // Exercise lines 455-465 (pop_first draining contexts)
    use crate::trace::AtTrace;

    let mut trace = AtTrace::capture();
    trace.try_add_context(
        core::panic::Location::caller(),
        crate::context::AtContext::Text(alloc::borrow::Cow::Borrowed("ctx1")),
    );
    // Add a second frame
    trace.try_push(core::panic::Location::caller());
    trace.try_add_context(
        core::panic::Location::caller(),
        crate::context::AtContext::Text(alloc::borrow::Cow::Borrowed("ctx2")),
    );

    let first = trace.pop_first();
    assert!(first.is_some());
    let frame = first.unwrap();
    assert!(frame.context_count() > 0);
}

#[test]
fn test_trace_push_with_contexts() {
    // Exercise line 475 (push with contexts on AtFrameOwned)
    use crate::trace::AtTrace;

    let mut trace = AtTrace::capture();
    trace.try_add_context(
        core::panic::Location::caller(),
        crate::context::AtContext::Text(alloc::borrow::Cow::Borrowed("existing")),
    );

    // Pop first, then push it back
    let frame = trace.pop_first().unwrap();
    let count_before = trace.frame_count();
    trace.push(frame);
    assert_eq!(trace.frame_count(), count_before + 1);
}

#[test]
fn test_trace_push_first_with_contexts() {
    // Exercise lines 534, 541 (push_first with contexts)
    use crate::trace::AtTrace;

    let mut trace = AtTrace::capture();
    trace.try_push(core::panic::Location::caller()); // Add a second frame

    // Pop from the end, add context via builder, then push_first
    let frame = trace.pop_first().unwrap().with_str("prepended");
    trace.push_first(frame);
    assert!(trace.frame_count() >= 2);
}

#[test]
fn test_traceable_at_first_pop_and_insert() {
    // Exercise lines 1208-1209 (at_first_pop, at_first_insert)
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let mut err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_str("ctx");

    let count_before = err.trace().unwrap().frame_count();
    let frame = err.at_first_pop().unwrap();
    assert_eq!(err.trace().unwrap().frame_count(), count_before - 1);
    err.at_first_insert(frame);
    assert_eq!(err.trace().unwrap().frame_count(), count_before);
}

#[test]
fn test_traceable_full_trace_with_nested_error_chain() {
    // Exercise lines 1393-1402 in trace.rs (nested error source chain in FullTraceDisplay)
    use crate::trace::{AtTrace, AtTraceable};
    use core::error::Error;

    #[derive(Debug)]
    struct Inner;
    impl fmt::Display for Inner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "inner")
        }
    }
    impl Error for Inner {}

    #[derive(Debug)]
    struct Outer(Inner);
    impl fmt::Display for Outer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "outer")
        }
    }
    impl Error for Outer {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "my error")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_aside_error(Outer(Inner));

    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("caused by: outer"), "Output: {}", output);
    assert!(output.contains("caused by: inner"), "Output: {}", output);
}

#[test]
fn test_traceable_full_trace_with_skipped_frames_display() {
    // Exercise line 1381 (FullTraceDisplay skipped frame marker)
    use crate::trace::{AtTrace, AtTraceable};

    struct MyErr {
        trace: AtTrace,
    }
    impl AtTraceable for MyErr {
        fn trace_mut(&mut self) -> &mut AtTrace {
            &mut self.trace
        }
        fn trace(&self) -> Option<&AtTrace> {
            Some(&self.trace)
        }
        fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "err")
        }
    }

    let err = MyErr {
        trace: AtTrace::capture(),
    }
    .at_skipped_frames()
    .at();

    let output = alloc::format!("{}", err.full_trace());
    assert!(output.contains("[...]"), "Output: {}", output);
}

#[test]
fn test_ext_at_string_err_path() {
    // Exercise line 201 in ext.rs
    fn make_err() -> Result<(), At<TestError>> {
        Err(at(TestError::NotFound))
    }
    let result = make_err().at_string(|| alloc::string::String::from("dynamic context"));
    let err = result.unwrap_err();
    let output = alloc::format!("{:?}", err);
    assert!(output.contains("dynamic context"), "Output: {}", output);
}

#[test]
fn test_crate_info_owned_with_some_values() {
    // Exercise lines 338, 348, 358 in crate_info.rs
    use crate::crate_info::AtCrateInfoBuilder;

    let info = AtCrateInfoBuilder::new()
        .name("test")
        .repo_owned(Some(alloc::string::String::from(
            "https://github.com/test/test",
        )))
        .commit_owned(Some(alloc::string::String::from("abc123")))
        .path_owned(Some(alloc::string::String::from("crates/test")))
        .build();

    assert_eq!(info.repo(), Some("https://github.com/test/test"));
    assert_eq!(info.commit(), Some("abc123"));
    assert_eq!(info.crate_path(), Some("crates/test"));
}

// ============================================================================
// Format.rs coverage — termcolor and HTML formatters
// ============================================================================

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_display_with_all_context_types() {
    use crate::AtCrateInfo;

    static C1: AtCrateInfo = AtCrateInfo::builder().name("crate-a").build();
    static C2: AtCrateInfo = AtCrateInfo::builder().name("crate-b").build();

    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&C1)
        .at()
        .at_str("text context")
        .at_fn(|| {})
        .at_debug(|| 42u32)
        .at_data(|| "display data")
        .at_aside_error(core::fmt::Error)
        .at_crate(&C2)
        .at();

    let output = alloc::format!("{}", err.display_color());
    assert!(output.contains("NotFound"), "Output: {}", output);
    assert!(output.contains("text context"), "Output: {}", output);
    assert!(output.contains("42"), "Output: {}", output);
    assert!(output.contains("display data"), "Output: {}", output);
    assert!(output.contains("caused by"), "Output: {}", output);
}

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_display_with_skipped_frames() {
    let err = at(TestError::NotFound).at_skipped_frames();
    let output = alloc::format!("{}", err.display_color());
    assert!(output.contains("[...]"), "Output: {}", output);
}

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_display_no_trace() {
    let err = At::wrap(TestError::NotFound);
    let output = alloc::format!("{}", err.display_color());
    assert!(output.contains("NotFound"), "Output: {}", output);
}

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_meta_with_crate_boundaries() {
    use crate::AtCrateInfo;

    static C1: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-a")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc123"))
        .build();
    static C2: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-b")
        .repo(Some("https://github.com/user/repo2"))
        .commit(Some("def456"))
        .build();

    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&C1)
        .at()
        .at_str("context")
        .at_fn(|| {})
        .at_debug(|| 42u32)
        .at_data(|| "disp")
        .at_aside_error(core::fmt::Error)
        .at_crate(&C2)
        .at();

    let output = alloc::format!("{}", err.display_color_meta());
    assert!(output.contains("crate-a"), "Output: {}", output);
    assert!(output.contains("crate-b"), "Output: {}", output);
    // Link URL should be present
    assert!(output.contains("github.com"), "Output: {}", output);
}

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_meta_with_skipped_frames() {
    let err = at(TestError::NotFound).at_skipped_frames();
    let output = alloc::format!("{}", err.display_color_meta());
    assert!(output.contains("[...]"), "Output: {}", output);
}

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_meta_no_trace() {
    let err = At::wrap(TestError::NotFound);
    let output = alloc::format!("{}", err.display_color_meta());
    assert!(output.contains("NotFound"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_display_with_all_context_types() {
    use crate::AtCrateInfo;

    static C1: AtCrateInfo = AtCrateInfo::builder().name("crate-a").build();
    static C2: AtCrateInfo = AtCrateInfo::builder().name("crate-b").build();

    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&C1)
        .at()
        .at_str("text context")
        .at_fn(|| {})
        .at_debug(|| 42u32)
        .at_data(|| "display data")
        .at_aside_error(core::fmt::Error)
        .at_crate(&C2)
        .at();

    let output = alloc::format!("{}", err.display_html());
    assert!(output.contains("context-text"), "Output: {}", output);
    assert!(output.contains("context-fn"), "Output: {}", output);
    assert!(output.contains("context-data"), "Output: {}", output);
    assert!(output.contains("context-error"), "Output: {}", output);
    assert!(output.contains("crate-boundary"), "Output: {}", output);
    assert!(output.contains("crate-a"), "Output: {}", output);
    assert!(output.contains("crate-b"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_display_with_skipped_frames() {
    let err = at(TestError::NotFound).at_skipped_frames();
    let output = alloc::format!("{}", err.display_html());
    assert!(output.contains("skip-marker"), "Output: {}", output);
    assert!(output.contains("[...]"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_display_no_trace() {
    let err = At::wrap(TestError::NotFound);
    let output = alloc::format!("{}", err.display_html());
    assert!(output.contains("NotFound"), "Output: {}", output);
    // Should close div immediately since no trace
    assert!(output.contains("</div>"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_display_with_link_template() {
    use crate::AtCrateInfo;

    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test-crate")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc123"))
        .path(Some("crates/test"))
        .build();

    let err = At::wrap(TestError::NotFound).set_crate_info(&INFO).at();
    let output = alloc::format!("{}", err.display_html());
    // Should have <a href=...> with link
    assert!(output.contains("<a href="), "Output: {}", output);
    assert!(output.contains("github.com"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_display_with_crate_boundary_and_links() {
    use crate::AtCrateInfo;

    static C1: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-a")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc123"))
        .build();
    static C2: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-b")
        .repo(Some("https://github.com/user/repo2"))
        .commit(Some("def456"))
        .build();

    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&C1)
        .at()
        .at_str("msg")
        .at_crate(&C2)
        .at();

    let output = alloc::format!("{}", err.display_html());
    assert!(output.contains("crate-boundary"), "Output: {}", output);
    assert!(output.contains("crate-a"), "Output: {}", output);
    assert!(output.contains("crate-b"), "Output: {}", output);
    assert!(output.contains("crate-info"), "Output: {}", output);
}

#[test]
fn test_at_debug_with_all_context_types() {
    // Exercise all branches in At Debug impl (lines 800-810)
    use crate::AtCrateInfo;

    static INFO: AtCrateInfo = AtCrateInfo::builder().name("test").build();

    let err = at(TestError::NotFound)
        .at_str("text msg")
        .at_fn(|| {})
        .at_debug(|| 42u32)
        .at_data(|| "display val")
        .at_aside_error(core::fmt::Error)
        .at_crate(&INFO);

    let output = alloc::format!("{:?}", err);
    assert!(output.contains("text msg"), "Output: {}", output);
    assert!(output.contains("in "), "Output: {}", output);
    assert!(output.contains("42"), "Output: {}", output);
    assert!(output.contains("display val"), "Output: {}", output);
    assert!(output.contains("caused by"), "Output: {}", output);
    // Crate boundary should NOT show in basic Debug
    assert!(!output.contains("[crate:"), "Output: {}", output);
}

#[test]
fn test_display_with_meta_all_context_types() {
    // Exercise all branches in display_with_meta (lines 900-910)
    use crate::AtCrateInfo;

    static C1: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-a")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc123"))
        .build();
    static C2: AtCrateInfo = AtCrateInfo::builder().name("crate-b").build();

    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&C1)
        .at()
        .at_str("text")
        .at_fn(|| {})
        .at_debug(|| 42u32)
        .at_data(|| "disp")
        .at_aside_error(core::fmt::Error)
        .at_crate(&C2);

    let output = alloc::format!("{}", err.display_with_meta());
    assert!(output.contains("text"), "Output: {}", output);
    assert!(output.contains("42"), "Output: {}", output);
    assert!(output.contains("disp"), "Output: {}", output);
    assert!(output.contains("caused by"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_escape_ampersand_and_quotes() {
    // Exercise lines 445, 446 in format.rs (& and " escaping)
    let err = at(TestError::NotFound).at_string(|| alloc::string::String::from("x&y \"quoted\""));
    let output = alloc::format!("{}", err.display_html());
    assert!(output.contains("&amp;"), "Output: {}", output);
    assert!(output.contains("&quot;"), "Output: {}", output);
}

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_display_crate_boundary_with_other_contexts() {
    // Exercise line 70 (continue on Crate context in TermColorDisplay)
    // Need a frame that has BOTH a crate boundary AND other contexts
    use crate::AtCrateInfo;

    static C1: AtCrateInfo = AtCrateInfo::builder().name("a").build();
    static C2: AtCrateInfo = AtCrateInfo::builder().name("b").build();

    // Build error with crate boundary and text context on same frame
    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&C1)
        .at()
        .at_str("before boundary")
        .at_crate(&C2);

    let output = alloc::format!("{}", err.display_color());
    assert!(output.contains("before boundary"), "Output: {}", output);
}

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_meta_link_url_substitution() {
    // Exercise lines 155-163 and 217 (link template in TermColorMetaDisplay)
    use crate::AtCrateInfo;

    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test-crate")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc123"))
        .path(Some("crates/test"))
        .build();

    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&INFO)
        .at()
        .at_str("msg");

    let output = alloc::format!("{}", err.display_color_meta());
    // Should contain a URL from the link template
    assert!(output.contains("github.com"), "Output: {}", output);
}

#[cfg(feature = "_termcolor")]
#[test]
fn test_termcolor_meta_crate_boundary_with_contexts() {
    // Exercise line 173 (continue on Crate context in TermColorMetaDisplay)
    use crate::AtCrateInfo;

    static C1: AtCrateInfo = AtCrateInfo::builder()
        .name("a")
        .repo(Some("https://github.com/user/a"))
        .commit(Some("abc"))
        .build();
    static C2: AtCrateInfo = AtCrateInfo::builder()
        .name("b")
        .repo(Some("https://github.com/user/b"))
        .commit(Some("def"))
        .build();

    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&C1)
        .at()
        .at_str("context text")
        .at_crate(&C2);

    let output = alloc::format!("{}", err.display_color_meta());
    assert!(output.contains("context text"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_display_link_and_crate_skip_in_contexts() {
    // Exercise lines 374 (link URL), 396 (continue crate), 464 (build_link_base)
    use crate::AtCrateInfo;

    static C1: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-a")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc123"))
        .path(Some("src"))
        .build();
    static C2: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-b")
        .repo(Some("https://github.com/user/repo2"))
        .commit(Some("def456"))
        .build();

    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&C1)
        .at()
        .at_str("context msg")
        .at_fn(|| {})
        .at_debug(|| 42u32)
        .at_data(|| "disp")
        .at_aside_error(core::fmt::Error)
        .at_crate(&C2)
        .at();

    let output = alloc::format!("{}", err.display_html());
    // Should have link from C1
    assert!(
        output.contains("<a href="),
        "Should have link. Output: {}",
        output
    );
    // Should have context-text, context-fn etc.
    assert!(output.contains("context-text"), "Output: {}", output);
    assert!(output.contains("context-fn"), "Output: {}", output);
    // Should have crate boundary
    assert!(output.contains("crate-boundary"), "Output: {}", output);
}

#[cfg(feature = "_html")]
#[test]
fn test_html_display_skipped_frames_with_link() {
    // Exercise line 429 (None/skip-marker in HTML) with link context
    use crate::AtCrateInfo;

    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc"))
        .build();

    let err = At::wrap(TestError::NotFound)
        .set_crate_info(&INFO)
        .at()
        .at_skipped_frames();

    let output = alloc::format!("{}", err.display_html());
    assert!(output.contains("skip-marker"), "Output: {}", output);
}

#[test]
fn test_at_trace_boxed_new_const() {
    // Exercise lines 777-778 (AtTraceBoxed::new)
    use crate::trace::AtTraceBoxed;

    let boxed = AtTraceBoxed::new();
    assert!(boxed.as_ref().is_none());
    let dbg = alloc::format!("{:?}", boxed);
    assert!(dbg.contains("None") || dbg.contains("AtTraceBoxed"));
}

#[test]
fn test_context_vec_new_path() {
    // Exercise lines 154-155 (context_vec_new — only called via try_add_context when no contexts exist)
    use crate::trace::AtTrace;

    // Create a fresh trace with no contexts, then add one
    let mut trace = AtTrace::new();
    trace.try_push(core::panic::Location::caller());
    trace.try_add_context(
        core::panic::Location::caller(),
        crate::context::AtContext::Text(alloc::borrow::Cow::Borrowed("first")),
    );
    assert_eq!(trace.contexts().count(), 1);
}

#[test]
fn test_context_vec_limit() {
    // Exercise line 167 (try_push_context limit check)
    use crate::trace::AtTrace;

    let mut trace = AtTrace::new();
    trace.try_push(core::panic::Location::caller());
    // Push many contexts to hit the limit
    for i in 0..200 {
        trace.try_add_context(
            core::panic::Location::caller(),
            crate::context::AtContext::Text(alloc::borrow::Cow::Owned(alloc::format!("ctx{}", i))),
        );
    }
    // Should have been capped at AT_MAX_CONTEXTS
    assert!(trace.contexts().count() <= 128);
}

#[test]
fn test_trace_pop_last_with_contexts_break() {
    // Exercise line 459 (break in pop() when contexts span multiple frames)
    use crate::trace::AtTrace;

    let mut trace = AtTrace::capture();
    trace.try_add_context(
        core::panic::Location::caller(),
        crate::context::AtContext::Text(alloc::borrow::Cow::Borrowed("first-ctx")),
    );
    // Add second frame with a different context
    trace.try_push(core::panic::Location::caller());
    trace.try_add_context(
        core::panic::Location::caller(),
        crate::context::AtContext::Text(alloc::borrow::Cow::Borrowed("second-ctx")),
    );

    // Pop the last frame — the while loop should break when it encounters
    // contexts belonging to the first frame
    let popped = trace.pop();
    assert!(popped.is_some());
    let frame = popped.unwrap();
    assert!(frame.context_count() > 0);
    // First frame's context should still be in the trace
    assert!(trace.contexts().count() > 0);
}

#[test]
fn test_at_locations_covers_iterator() {
    // Exercise line 563 (At::locations())
    // Using a longer chain to ensure the iterator body gets instrumented
    let err = at(TestError::NotFound).at_str("a").at_str("b").at_str("c");
    let locs: alloc::vec::Vec<_> = err.locations().collect();
    assert!(!locs.is_empty());
    // Verify locations are from this file
    assert!(locs.iter().all(|l| l.file().contains("tests.rs")));
}

#[test]
fn test_at_frames_iterator() {
    // Exercise lines 400, 402 (AtTrace::frames() iterator)
    use crate::trace::AtTrace;

    let mut trace = AtTrace::capture();
    trace.try_push(core::panic::Location::caller());
    trace.try_push(core::panic::Location::caller());

    let frames: alloc::vec::Vec<_> = trace.frames().collect();
    assert_eq!(frames.len(), 3);
    for frame in &frames {
        assert!(frame.location().is_some());
    }
}

#[test]
fn test_context_debug_any_type_name() {
    // Exercise lines 31, 57 (AtDebugAny::type_name, AtDisplayAny::type_name)
    use crate::context::AtContext;

    let ctx = AtContext::Debug(Box::new(42u32));
    let tn = ctx.type_name().unwrap();
    assert!(tn.contains("u32"), "type_name: {}", tn);

    let ctx = AtContext::Display(Box::new(42u32));
    let tn = ctx.type_name().unwrap();
    assert!(tn.contains("u32"), "type_name: {}", tn);
}

#[test]
fn test_context_display_fmt_for_display_variant() {
    // Exercise line 181 (AtContext Display impl for Display variant)
    use crate::context::AtContext;

    let ctx = AtContext::Display(Box::new(alloc::string::String::from("hello")));
    let disp = alloc::format!("{}", ctx);
    assert_eq!(disp, "hello");

    // FunctionName in Display
    let ctx = AtContext::FunctionName("my_fn");
    let disp = alloc::format!("{}", ctx);
    assert!(disp.contains("my_fn"));
}

#[test]
fn test_context_downcast_ref_all_none_arms() {
    // Exercise lines 127, 128, 129 individually
    use crate::context::AtContext;

    // Text — always None
    let ctx = AtContext::Text(alloc::borrow::Cow::Borrowed("hi"));
    assert!(ctx.downcast_ref::<u32>().is_none());

    // FunctionName — always None
    let ctx = AtContext::FunctionName("fn");
    assert!(ctx.downcast_ref::<u32>().is_none());

    // Crate — always None
    use crate::AtCrateInfo;
    static INFO: AtCrateInfo = AtCrateInfo::builder().name("x").build();
    let ctx = AtContext::Crate(&INFO);
    assert!(ctx.downcast_ref::<u32>().is_none());

    // Error — always None
    let ctx = AtContext::Error(Box::new(core::fmt::Error));
    assert!(ctx.downcast_ref::<u32>().is_none());
}
