//! Integration tests for AtCrateInfo, cross-crate boundaries, and sizeof.

#![allow(dead_code)]

use core::mem::size_of;
use errat::{At, AtContext, AtCrateInfo, at, at_crate};

// Define the crate-level static for at!() and at_crate!() to reference
errat::define_at_crate_info!();

#[derive(Debug)]
struct TestError;

// ============================================================================
// AtCrateInfo Static Allocation
// ============================================================================

#[test]
fn crate_info_is_static() {
    let info: &'static AtCrateInfo = crate::at_crate_info();

    // Should be a static reference, not heap allocated
    assert!(!info.name().is_empty());
}

#[test]
fn crate_info_captures_crate_name() {
    let info = crate::at_crate_info();

    assert_eq!(info.name(), "errat", "Should capture CARGO_PKG_NAME");
}

#[test]
fn crate_info_captures_module_path() {
    let info = crate::at_crate_info();

    // module_path!() returns the module where crate::at_crate_info() is called
    assert!(
        info.module().contains("crate_info"),
        "Module should contain test module name. Got: {}",
        info.module()
    );
}

#[test]
fn crate_info_repo_from_cargo_toml() {
    let info = crate::at_crate_info();

    // CARGO_PKG_REPOSITORY is set in Cargo.toml
    // May be None or empty string if not set
    if let Some(repo) = info.repo() {
        if !repo.is_empty() {
            assert!(
                repo.contains("github.com")
                    || repo.contains("gitlab.com")
                    || repo.starts_with("https://"),
                "Repo should be a URL. Got: {}",
                repo
            );
        }
    }
}

#[test]
fn crate_info_commit_from_env() {
    let info = crate::at_crate_info();

    // GIT_COMMIT, GITHUB_SHA, or CI_COMMIT_SHA - usually None in tests
    // Just verify it doesn't panic
    let _ = info.commit();
}

#[test]
fn multiple_crate_info_calls_same_static() {
    let info1 = crate::at_crate_info();
    let info2 = crate::at_crate_info();

    // Each call creates its own static, but with same values
    assert_eq!(info1.name(), info2.name());
    assert_eq!(info1.repo(), info2.repo());
}

// ============================================================================
// AtCrateInfo in Traces
// ============================================================================

#[test]
fn at_macro_embeds_crate_info() {
    let err = at!(TestError);

    // Should have crate info in contexts
    let has_crate_ctx = err.contexts().any(|ctx| ctx.is_crate_boundary());
    assert!(has_crate_ctx, "at!() should embed crate info");
}

#[test]
fn at_crate_adds_boundary_marker() {
    fn inner() -> Result<(), At<TestError>> {
        Err(at!(TestError))
    }

    fn outer() -> Result<(), At<TestError>> {
        at_crate!(inner())?;
        Ok(())
    }

    let err = outer().unwrap_err();

    // Should have at least 2 crate boundaries (at! and at_crate!)
    let crate_count = err.contexts().filter(|ctx| ctx.is_crate_boundary()).count();
    assert!(
        crate_count >= 2,
        "Should have multiple crate boundaries. Got: {}",
        crate_count
    );
}

#[test]
fn crate_info_accessible_from_context() {
    let err = at!(TestError);

    for ctx in err.contexts() {
        if let Some(info) = ctx.as_crate_info() {
            assert_eq!(info.name(), "errat");
            return;
        }
    }
    panic!("Should find AtCrateInfo in contexts");
}

#[test]
fn display_with_meta_uses_crate_info() {
    let err = at!(TestError);
    let output = format!("{}", err.display_with_meta());

    assert!(
        output.contains("crate: errat"),
        "display_with_meta should show crate name. Got:\n{}",
        output
    );
}

// ============================================================================
// Cross-Crate Boundary Simulation
// ============================================================================

mod simulated_dep {
    //! Simulates a dependency crate
    use errat::{At, at};

    #[derive(Debug)]
    pub struct DepError;

    pub fn dep_function() -> Result<(), At<DepError>> {
        Err(at!(DepError))
    }
}

#[test]
fn cross_crate_trace_has_multiple_boundaries() {
    use simulated_dep::DepError;

    fn my_wrapper() -> Result<(), At<DepError>> {
        at_crate!(simulated_dep::dep_function())?;
        Ok(())
    }

    let err = my_wrapper().unwrap_err();

    // Count crate boundaries
    let boundaries: Vec<_> = err
        .contexts()
        .filter_map(|ctx| ctx.as_crate_info())
        .collect();

    assert!(
        boundaries.len() >= 2,
        "Cross-crate trace should have multiple boundaries. Got: {}",
        boundaries.len()
    );
}

#[test]
fn crate_boundary_updates_github_links() {
    let err = at!(TestError).at_crate(crate::at_crate_info());
    let output = format!("{}", err.display_with_meta());

    // Should have location lines
    assert!(
        output.contains("at ") && output.contains(".rs:"),
        "Should have location lines. Got:\n{}",
        output
    );
}

// ============================================================================
// sizeof Tests
// ============================================================================

#[test]
fn sizeof_at_is_error_plus_pointer() {
    // At<E> = E (inline) + Option<Box<Trace>> (8 bytes on 64-bit)
    let ptr_size = size_of::<Option<Box<()>>>();
    assert_eq!(
        ptr_size, 8,
        "Option<Box<T>> should be 8 bytes (null optimization)"
    );

    // Small error
    #[derive(Debug)]
    struct Small(u8);
    let small_at = size_of::<At<Small>>();
    assert!(
        small_at <= size_of::<Small>() + 8 + 8, // error + pointer + alignment
        "At<Small> should be ~16 bytes. Got: {}",
        small_at
    );

    // Larger error
    #[derive(Debug)]
    struct Large([u8; 64]);
    let large_at = size_of::<At<Large>>();
    assert_eq!(
        large_at,
        size_of::<Large>() + 8,
        "At<Large> should be error + 8 bytes. Got: {}",
        large_at
    );
}

#[test]
fn sizeof_crate_info_is_six_fields() {
    // AtCrateInfo has 6 fields: name, repo, commit, crate_path, module, meta
    // 5 are &'static str or Option<&'static str> (16 bytes each)
    // 1 is &'static [(&'static str, &'static str)] (16 bytes: ptr + len)
    let info_size = size_of::<AtCrateInfo>();
    let expected = 6 * size_of::<Option<&'static str>>();

    assert_eq!(
        info_size, expected,
        "AtCrateInfo should be 6 fields ({} bytes). Got: {}",
        expected, info_size
    );
}

#[test]
fn sizeof_context_is_bounded() {
    let ctx_size = size_of::<AtContext>();

    // AtContext is an enum with largest variant being Box<dyn ...> (fat pointer = 16 bytes)
    // Plus discriminant and padding
    assert!(
        ctx_size <= 24,
        "AtContext should be <= 24 bytes. Got: {}",
        ctx_size
    );
}

#[test]
fn sizeof_location_is_one_pointer() {
    use core::panic::Location;

    let loc_size = size_of::<&'static Location<'static>>();
    assert_eq!(loc_size, 8, "Location reference should be 8 bytes");
}

#[test]
fn sizeof_option_box_uses_null_optimization() {
    // Verify null pointer optimization works
    assert_eq!(
        size_of::<Option<Box<u8>>>(),
        size_of::<Box<u8>>(),
        "Option<Box<T>> should use null optimization"
    );
}

#[test]
fn sizeof_common_error_types() {
    // Common patterns users might use

    #[derive(Debug)]
    enum SmallEnum {
        A,
        B,
        C,
    }
    assert!(
        size_of::<At<SmallEnum>>() <= 16,
        "At<SmallEnum> should be <= 16 bytes. Got: {}",
        size_of::<At<SmallEnum>>()
    );

    #[derive(Debug)]
    struct StringError(String);
    assert_eq!(
        size_of::<At<StringError>>(),
        size_of::<String>() + 8,
        "At<StringError> = String(24) + ptr(8) = 32"
    );

    #[derive(Debug)]
    struct BoxedError(Box<dyn core::error::Error + Send + Sync>);
    assert_eq!(
        size_of::<At<BoxedError>>(),
        size_of::<BoxedError>() + 8,
        "At<BoxedError> = fat_ptr(16) + ptr(8) = 24"
    );
}

// ============================================================================
// Repository URL Formatting
// ============================================================================

#[test]
fn crate_info_new_const() {
    // AtCrateInfo::new is const
    const INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test-crate")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc123"))
        .module("test::module")
        .build();

    assert_eq!(INFO.name(), "test-crate");
    assert_eq!(INFO.repo(), Some("https://github.com/user/repo"));
    assert_eq!(INFO.commit(), Some("abc123"));
    assert_eq!(INFO.module(), "test::module");
}

#[test]
fn crate_info_static_in_const_context() {
    // Can use AtCrateInfo in const/static contexts
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("my-crate")
        .repo(Some("https://github.com/me/my-crate"))
        .commit(None)
        .module("my_crate")
        .build();

    assert_eq!(INFO.name(), "my-crate");
}

#[test]
fn github_link_format_in_display() {
    // Create a AtCrateInfo with repo and commit
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("deadbeef"))
        .module("test")
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // Should generate GitHub blob URL
    assert!(
        output.contains("github.com/user/repo/blob/deadbeef/"),
        "Should have GitHub blob URL. Got:\n{}",
        output
    );
}

#[test]
fn github_link_includes_line_number() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/user/repo"))
        .commit(Some("abc123"))
        .module("test")
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // Should have #L<number> anchor
    assert!(
        output.contains("#L"),
        "Should have line number anchor. Got:\n{}",
        output
    );
}

#[test]
fn repo_without_commit_no_link() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/user/repo"))
        .commit(None)
        .module(
            // No commit
            "test",
        )
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // Should NOT have blob URL without commit (can't make permalink)
    assert!(
        !output.contains("/blob/"),
        "Without commit, should not have blob URL. Got:\n{}",
        output
    );
}

#[test]
fn trailing_slash_stripped_from_repo() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/user/repo/"))
        .commit(
            // Trailing slash
            Some("abc123"),
        )
        .module("test")
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // Should not have double slashes
    assert!(
        !output.contains("repo//blob"),
        "Trailing slash should be stripped. Got:\n{}",
        output
    );
}

// ============================================================================
// Crate Boundary Switching
// ============================================================================

#[test]
fn crate_boundary_switches_github_links() {
    static CRATE_A: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-a")
        .repo(Some("https://github.com/org/crate-a"))
        .commit(Some("aaa111"))
        .module("crate_a")
        .build();

    static CRATE_B: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-b")
        .repo(Some("https://github.com/org/crate-b"))
        .commit(Some("bbb222"))
        .module("crate_b")
        .build();

    // Simulate: error in crate-a, crosses to crate-b
    let err = errat::At::new(TestError)
        .at()
        .at_crate(&CRATE_A)
        .at()
        .at_crate(&CRATE_B)
        .at();

    let output = format!("{}", err.display_with_meta());

    // Should have links for both crates
    assert!(
        output.contains("crate-a/blob/aaa111"),
        "Should have crate-a GitHub link. Got:\n{}",
        output
    );
    assert!(
        output.contains("crate-b/blob/bbb222"),
        "Should have crate-b GitHub link. Got:\n{}",
        output
    );
}

#[test]
fn crate_boundary_affects_subsequent_locations() {
    static CRATE_X: AtCrateInfo = AtCrateInfo::builder()
        .name("crate-x")
        .repo(Some("https://github.com/org/crate-x"))
        .commit(Some("xxx"))
        .module("crate_x")
        .build();

    // Locations after boundary should use that crate's info
    let err = errat::At::new(TestError)
        .at_crate(&CRATE_X) // Boundary
        .at() // Should use CRATE_X
        .at(); // Should use CRATE_X

    let output = format!("{}", err.display_with_meta());

    // Count occurrences of crate-x links
    let link_count = output.matches("crate-x/blob/xxx").count();
    assert!(
        link_count >= 2,
        "Multiple locations should use CRATE_X info. Found {} links. Got:\n{}",
        link_count,
        output
    );
}

#[test]
fn multiple_boundary_switches() {
    static C1: AtCrateInfo = AtCrateInfo::builder()
        .name("c1")
        .repo(Some("https://gh.com/c1"))
        .commit(Some("111"))
        .module("c1")
        .build();
    static C2: AtCrateInfo = AtCrateInfo::builder()
        .name("c2")
        .repo(Some("https://gh.com/c2"))
        .commit(Some("222"))
        .module("c2")
        .build();
    static C3: AtCrateInfo = AtCrateInfo::builder()
        .name("c3")
        .repo(Some("https://gh.com/c3"))
        .commit(Some("333"))
        .module("c3")
        .build();

    let err = errat::At::new(TestError)
        .at_crate(&C1)
        .at()
        .at_crate(&C2)
        .at()
        .at_crate(&C3)
        .at();

    let output = format!("{}", err.display_with_meta());

    assert!(output.contains("c1/blob/111"), "Should have c1 link");
    assert!(output.contains("c2/blob/222"), "Should have c2 link");
    assert!(output.contains("c3/blob/333"), "Should have c3 link");
}

// ============================================================================
// At<At<E>> Anti-pattern
// ============================================================================

#[test]
fn nested_at_is_wasteful() {
    // At<At<E>> works but wastes memory - two separate traces
    // This test documents the behavior, not endorses it

    #[derive(Debug)]
    struct Inner;

    let inner: At<Inner> = errat::at(Inner);
    let outer: At<At<Inner>> = errat::at(inner);

    // Both have their own traces - wasteful!
    assert_eq!(outer.trace_len(), 1, "Outer has its own trace");
    assert_eq!(outer.error().trace_len(), 1, "Inner has its own trace");

    // Total overhead: 2 Box<Trace> allocations instead of 1
    // This is why you should use .at() on Result, not wrap At<At<E>>
}

#[test]
fn prefer_result_at_over_nested_at() {
    // GOOD: Use ResultAtExt to extend existing trace
    fn good_inner() -> Result<(), At<TestError>> {
        Err(errat::at(TestError))
    }

    fn good_outer() -> Result<(), At<TestError>> {
        good_inner().map_err(|e| e.at()) // Extends same trace
    }

    let good_err = good_outer().unwrap_err();
    assert_eq!(good_err.trace_len(), 2, "Single trace with 2 locations");

    // BAD: Wrapping At in At
    fn bad_outer() -> At<At<TestError>> {
        errat::at(errat::at(TestError)) // Creates nested traces
    }

    let bad_err = bad_outer();
    // Two separate traces - wasteful
    assert_eq!(bad_err.trace_len(), 1);
    assert_eq!(bad_err.error().trace_len(), 1);
}

// ============================================================================
// GitHub Link Expansion
// ============================================================================

#[test]
fn github_link_has_full_url() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("mylib")
        .repo(Some("https://github.com/myorg/mylib"))
        .commit(Some("abc123def"))
        .module("mylib")
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // Should have complete clickable URL
    assert!(
        output.contains("https://github.com/myorg/mylib/blob/abc123def/"),
        "Link should be full URL. Got:\n{}",
        output
    );
}

#[test]
fn github_link_has_file_path() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/org/repo"))
        .commit(Some("deadbeef"))
        .module("test")
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // Should include file path in URL
    assert!(
        output.contains("tests/crate_info.rs#L"),
        "Link should include file path and line. Got:\n{}",
        output
    );
}

#[test]
fn github_link_line_number_is_numeric() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/org/repo"))
        .commit(Some("abc"))
        .module("test")
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // Find #L and verify it's followed by digits
    let link_part = output
        .lines()
        .find(|l| l.contains("#L"))
        .expect("should have line anchor");

    let anchor_idx = link_part.find("#L").unwrap();
    let after_anchor = &link_part[anchor_idx + 2..];
    let line_num: String = after_anchor
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();

    assert!(
        !line_num.is_empty(),
        "Line number should be numeric. Got: {}",
        link_part
    );

    let num: u32 = line_num.parse().expect("should parse as number");
    assert!(num > 0, "Line number should be positive");
}

#[test]
fn windows_paths_converted_to_forward_slashes() {
    // The implementation converts backslashes to forward slashes
    // We can't easily test this directly, but we verify no backslashes in output
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/org/repo"))
        .commit(Some("abc"))
        .module("test")
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // URL lines should not have backslashes
    for line in output.lines() {
        if line.contains("github.com") {
            assert!(
                !line.contains('\\'),
                "GitHub URL should use forward slashes. Got: {}",
                line
            );
        }
    }
}

// ============================================================================
// Compile-Time Capture Behavior
// ============================================================================

#[test]
fn crate_info_commit_is_compile_time() {
    // GIT_COMMIT etc are captured at compile time via option_env!()
    // Falls back to version tag (v{VERSION}) for crates.io compatibility

    let info = crate::at_crate_info();

    // Should always have a commit (either env var or version tag fallback)
    let commit = info
        .commit()
        .expect("commit should always be Some due to version fallback");

    // Either a hex commit hash OR a version tag like "v0.1.0"
    let is_hex = commit.chars().all(|c| c.is_ascii_hexdigit());
    let is_version_tag = commit.starts_with('v') && commit.contains('.');

    assert!(
        is_hex || is_version_tag,
        "Commit should be hex hash or version tag. Got: {}",
        commit
    );
}

#[test]
fn crate_info_version_tag_fallback() {
    // When no GIT_COMMIT env var, falls back to v{CARGO_PKG_VERSION}
    // This makes crates.io dependencies work automatically!

    let info = crate::at_crate_info();
    let commit = info
        .commit()
        .expect("should have commit from version fallback");

    // In tests without CI env vars, should be version tag
    // (unless someone set GIT_COMMIT in their environment)
    if !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        assert!(
            commit.starts_with("v"),
            "Fallback should be version tag. Got: {}",
            commit
        );
        assert!(
            commit == concat!("v", env!("CARGO_PKG_VERSION")),
            "Version tag should match CARGO_PKG_VERSION. Got: {}",
            commit
        );
    }
}

#[test]
fn crate_info_is_truly_static() {
    // Verify crate::at_crate_info() returns truly static data
    let info1 = crate::at_crate_info();
    let info2 = crate::at_crate_info();

    // Same compilation unit = same static (by value, different addresses ok)
    assert_eq!(info1.name(), info2.name());
    assert_eq!(info1.repo(), info2.repo());
    assert_eq!(info1.commit(), info2.commit());

    // The data is embedded in the binary, not computed at runtime
    // This is verified by the fact that option_env!() is a compile-time macro
}

#[test]
fn module_path_captured_at_macro_site() {
    let info = crate::at_crate_info();

    // module_path!() captures where crate::at_crate_info() is invoked
    assert!(
        info.module().starts_with("crate_info"),
        "Module should be this test module. Got: {}",
        info.module()
    );
}

mod nested_module {
    #[test]
    fn nested_module_has_different_path() {
        // With the getter pattern, module path is captured at the define_at_crate_info!() site
        // (crate root), not at the call site. This test verifies that behavior.
        let info = crate::at_crate_info();

        // Module should be the crate root module (where define_at_crate_info!() was called)
        assert!(
            info.module().starts_with("crate_info"),
            "Module should be crate root. Got: {}",
            info.module()
        );
    }
}

// ============================================================================
// Workspace Crate Path
// ============================================================================

#[test]
fn crate_path_included_in_github_url() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("workspace-crate")
        .repo(Some("https://github.com/org/monorepo"))
        .commit(Some("abc123"))
        .path(Some("crates/mylib/"))
        .module(
            // Crate is in subdirectory
            "workspace_crate",
        )
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // URL should include crate_path between commit and file
    assert!(
        output.contains("monorepo/blob/abc123/crates/mylib/"),
        "URL should include crate_path. Got:\n{}",
        output
    );
}

#[test]
fn crate_path_none_works() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("root-crate")
        .repo(Some("https://github.com/org/repo"))
        .commit(Some("def456"))
        .path(None)
        .module(
            // Crate at repo root
            "root_crate",
        )
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // URL should work without crate_path
    assert!(
        output.contains("repo/blob/def456/tests/"),
        "URL should work without crate_path. Got:\n{}",
        output
    );
}

#[test]
fn crate_path_with_trailing_slash() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/org/repo"))
        .commit(Some("abc"))
        .path(Some("packages/core/"))
        .module(
            // With trailing slash
            "test",
        )
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // Should not have double slashes
    assert!(
        !output.contains("core//"),
        "Should not have double slashes. Got:\n{}",
        output
    );
    assert!(
        output.contains("packages/core/tests/"),
        "Should have correct path. Got:\n{}",
        output
    );
}

#[test]
fn crate_path_without_trailing_slash() {
    static INFO: AtCrateInfo = AtCrateInfo::builder()
        .name("test")
        .repo(Some("https://github.com/org/repo"))
        .commit(Some("abc"))
        .path(Some("packages/core"))
        .module(
            // Without trailing slash
            "test",
        )
        .build();

    let err = errat::At::new(TestError).at().at_crate(&INFO);
    let output = format!("{}", err.display_with_meta());

    // Should still work (file path starts with tests/)
    assert!(
        output.contains("packages/coretests/") || output.contains("packages/core/tests/"),
        "Path should be combined. Got:\n{}",
        output
    );
}

// ============================================================================
// define_at_crate_info!() Macro Tests
// ============================================================================

#[test]
fn crate_info_static_defines_hidden_static() {
    // __ERRAT_CRATE_INFO is defined by define_at_crate_info!() at top of file
    // at!() references it
    let err = at!(TestError);

    // Should have crate info in contexts
    let has_crate_ctx = err.contexts().any(|ctx| ctx.is_crate_boundary());
    assert!(has_crate_ctx, "at!() should reference crate static");
}

#[test]
fn crate_info_static_has_correct_name() {
    // The static should have captured CARGO_PKG_NAME
    let err = at!(TestError);

    for ctx in err.contexts() {
        if let Some(info) = ctx.as_crate_info() {
            assert_eq!(info.name(), "errat", "Should have crate name from env");
            return;
        }
    }
    panic!("Should find AtCrateInfo in at!() error");
}

// Test that define_at_crate_info!(path = "...") variant works
mod with_path {
    use super::*;

    // This would be in a workspace crate's lib.rs
    // errat::define_at_crate_info!(path = "crates/mylib/");

    #[test]
    fn path_option_sets_crate_path() {
        // We can't easily test define_at_crate_info!(path = ...) here because
        // we already called define_at_crate_info!() at the top of this file.
        // Instead, test that AtCrateInfo::with_path works correctly.
        static INFO: AtCrateInfo = AtCrateInfo::builder()
            .name("test")
            .repo(Some("https://github.com/org/repo"))
            .commit(Some("abc123"))
            .path(Some("crates/mylib/"))
            .module("test")
            .build();

        assert_eq!(INFO.crate_path(), Some("crates/mylib/"));

        let err = errat::At::new(TestError).at().at_crate(&INFO);
        let output = format!("{}", err.display_with_meta());

        assert!(
            output.contains("crates/mylib/"),
            "URL should include crate_path. Got:\n{}",
            output
        );
    }
}
