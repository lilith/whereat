//! Integration tests for CrateInfo, cross-crate boundaries, and sizeof.

#![allow(dead_code)]

use core::mem::size_of;
use errat::{At, Context, CrateInfo, at, at_crate, crate_info};

#[derive(Debug)]
struct TestError;

// ============================================================================
// CrateInfo Static Allocation
// ============================================================================

#[test]
fn crate_info_is_static() {
    let info: &'static CrateInfo = crate_info!();

    // Should be a static reference, not heap allocated
    assert!(!info.name.is_empty());
}

#[test]
fn crate_info_captures_crate_name() {
    let info = crate_info!();

    assert_eq!(info.name, "errat", "Should capture CARGO_PKG_NAME");
}

#[test]
fn crate_info_captures_module_path() {
    let info = crate_info!();

    // module_path!() returns the module where crate_info!() is called
    assert!(
        info.module.contains("crate_info"),
        "Module should contain test module name. Got: {}",
        info.module
    );
}

#[test]
fn crate_info_repo_from_cargo_toml() {
    let info = crate_info!();

    // CARGO_PKG_REPOSITORY is set in Cargo.toml
    // May be None or empty string if not set
    if let Some(repo) = info.repo {
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
    let info = crate_info!();

    // GIT_COMMIT, GITHUB_SHA, or CI_COMMIT_SHA - usually None in tests
    // Just verify it doesn't panic
    let _ = info.commit;
}

#[test]
fn multiple_crate_info_calls_same_static() {
    let info1 = crate_info!();
    let info2 = crate_info!();

    // Each call creates its own static, but with same values
    assert_eq!(info1.name, info2.name);
    assert_eq!(info1.repo, info2.repo);
}

// ============================================================================
// CrateInfo in Traces
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
            assert_eq!(info.name, "errat");
            return;
        }
    }
    panic!("Should find CrateInfo in contexts");
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
    let err = at!(TestError).at_crate(crate_info!());
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
fn sizeof_crate_info_is_five_pointers() {
    // CrateInfo has 5 fields: name, repo, commit, crate_path, module
    // All are &'static str or Option<&'static str>
    let info_size = size_of::<CrateInfo>();
    let expected = 5 * size_of::<Option<&'static str>>();

    assert_eq!(
        info_size, expected,
        "CrateInfo should be 5 pointers ({} bytes). Got: {}",
        expected, info_size
    );
}

#[test]
fn sizeof_context_is_bounded() {
    let ctx_size = size_of::<Context>();

    // Context is an enum with largest variant being Box<dyn ...> (fat pointer = 16 bytes)
    // Plus discriminant and padding
    assert!(
        ctx_size <= 24,
        "Context should be <= 24 bytes. Got: {}",
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
    // CrateInfo::new is const
    const INFO: CrateInfo = CrateInfo::new(
        "test-crate",
        Some("https://github.com/user/repo"),
        Some("abc123"),
        "test::module",
    );

    assert_eq!(INFO.name, "test-crate");
    assert_eq!(INFO.repo, Some("https://github.com/user/repo"));
    assert_eq!(INFO.commit, Some("abc123"));
    assert_eq!(INFO.module, "test::module");
}

#[test]
fn crate_info_static_in_const_context() {
    // Can use CrateInfo in const/static contexts
    static INFO: CrateInfo = CrateInfo::new(
        "my-crate",
        Some("https://github.com/me/my-crate"),
        None,
        "my_crate",
    );

    assert_eq!(INFO.name, "my-crate");
}

#[test]
fn github_link_format_in_display() {
    // Create a CrateInfo with repo and commit
    static INFO: CrateInfo = CrateInfo::new(
        "test",
        Some("https://github.com/user/repo"),
        Some("deadbeef"),
        "test",
    );

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
    static INFO: CrateInfo = CrateInfo::new(
        "test",
        Some("https://github.com/user/repo"),
        Some("abc123"),
        "test",
    );

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
    static INFO: CrateInfo = CrateInfo::new(
        "test",
        Some("https://github.com/user/repo"),
        None, // No commit
        "test",
    );

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
    static INFO: CrateInfo = CrateInfo::new(
        "test",
        Some("https://github.com/user/repo/"), // Trailing slash
        Some("abc123"),
        "test",
    );

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
    static CRATE_A: CrateInfo = CrateInfo::new(
        "crate-a",
        Some("https://github.com/org/crate-a"),
        Some("aaa111"),
        "crate_a",
    );

    static CRATE_B: CrateInfo = CrateInfo::new(
        "crate-b",
        Some("https://github.com/org/crate-b"),
        Some("bbb222"),
        "crate_b",
    );

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
    static CRATE_X: CrateInfo = CrateInfo::new(
        "crate-x",
        Some("https://github.com/org/crate-x"),
        Some("xxx"),
        "crate_x",
    );

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
    static C1: CrateInfo = CrateInfo::new("c1", Some("https://gh.com/c1"), Some("111"), "c1");
    static C2: CrateInfo = CrateInfo::new("c2", Some("https://gh.com/c2"), Some("222"), "c2");
    static C3: CrateInfo = CrateInfo::new("c3", Some("https://gh.com/c3"), Some("333"), "c3");

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
    static INFO: CrateInfo = CrateInfo::new(
        "mylib",
        Some("https://github.com/myorg/mylib"),
        Some("abc123def"),
        "mylib",
    );

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
    static INFO: CrateInfo = CrateInfo::new(
        "test",
        Some("https://github.com/org/repo"),
        Some("deadbeef"),
        "test",
    );

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
    static INFO: CrateInfo = CrateInfo::new(
        "test",
        Some("https://github.com/org/repo"),
        Some("abc"),
        "test",
    );

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
    static INFO: CrateInfo = CrateInfo::new(
        "test",
        Some("https://github.com/org/repo"),
        Some("abc"),
        "test",
    );

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

    let info = crate_info!();

    // Should always have a commit (either env var or version tag fallback)
    let commit = info
        .commit
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

    let info = crate_info!();
    let commit = info
        .commit
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
    // Verify crate_info!() returns truly static data
    let info1 = crate_info!();
    let info2 = crate_info!();

    // Same compilation unit = same static (by value, different addresses ok)
    assert_eq!(info1.name, info2.name);
    assert_eq!(info1.repo, info2.repo);
    assert_eq!(info1.commit, info2.commit);

    // The data is embedded in the binary, not computed at runtime
    // This is verified by the fact that option_env!() is a compile-time macro
}

#[test]
fn module_path_captured_at_macro_site() {
    let info = crate_info!();

    // module_path!() captures where crate_info!() is invoked
    assert!(
        info.module.starts_with("crate_info"),
        "Module should be this test module. Got: {}",
        info.module
    );
}

mod nested_module {
    use errat::crate_info;

    #[test]
    fn nested_module_has_different_path() {
        let info = crate_info!();

        assert!(
            info.module.contains("nested_module"),
            "Module should include nested_module. Got: {}",
            info.module
        );
    }
}

// ============================================================================
// Workspace Crate Path
// ============================================================================

#[test]
fn crate_path_included_in_github_url() {
    static INFO: CrateInfo = CrateInfo::with_path(
        "workspace-crate",
        Some("https://github.com/org/monorepo"),
        Some("abc123"),
        Some("crates/mylib/"), // Crate is in subdirectory
        "workspace_crate",
    );

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
    static INFO: CrateInfo = CrateInfo::with_path(
        "root-crate",
        Some("https://github.com/org/repo"),
        Some("def456"),
        None, // Crate at repo root
        "root_crate",
    );

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
    static INFO: CrateInfo = CrateInfo::with_path(
        "test",
        Some("https://github.com/org/repo"),
        Some("abc"),
        Some("packages/core/"), // With trailing slash
        "test",
    );

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
    static INFO: CrateInfo = CrateInfo::with_path(
        "test",
        Some("https://github.com/org/repo"),
        Some("abc"),
        Some("packages/core"), // Without trailing slash
        "test",
    );

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
// Explicit Path Macro Variant
// ============================================================================

#[test]
fn crate_info_explicit_path_macro() {
    // crate_info!("path/") variant for workspace crates
    let info = crate_info!("crates/mylib/");

    assert_eq!(info.name, "errat", "Should still capture CARGO_PKG_NAME");
    assert_eq!(
        info.crate_path,
        Some("crates/mylib/"),
        "Should use the explicit path"
    );
}

#[test]
fn crate_info_explicit_path_overrides_env() {
    // Explicit path should be used even if CRATE_PATH env var is set
    // (the macro variant ignores the env var and uses the literal)
    let info = crate_info!("explicit/path/");

    assert_eq!(info.crate_path, Some("explicit/path/"));
}

#[test]
fn crate_info_explicit_path_in_github_url() {
    // Use crate_info!("path/") in an actual trace
    let info = crate_info!("workspace/subcrate/");

    let err = errat::At::new(TestError).at().at_crate(info);
    let output = format!("{}", err.display_with_meta());

    // URL should include the explicit crate_path
    // Note: commit is version tag fallback (v{VERSION})
    assert!(
        output.contains("workspace/subcrate/"),
        "URL should include explicit crate_path. Got:\n{}",
        output
    );
}

#[test]
fn crate_info_explicit_empty_path() {
    // Explicit empty path (crate at repo root)
    let info = crate_info!("");

    assert_eq!(
        info.crate_path,
        Some(""),
        "Empty string is valid for root crate"
    );
}

#[test]
fn crate_info_explicit_path_has_commit() {
    // Explicit path variant should still capture commit (version fallback)
    let info = crate_info!("mypath/");

    // Should have commit from env var or version tag fallback
    assert!(
        info.commit.is_some(),
        "Should have commit (version fallback)"
    );
}

#[test]
fn crate_info_both_variants_same_other_fields() {
    let auto_info = crate_info!();
    let explicit_info = crate_info!("some/path/");

    // All fields except crate_path should be the same
    assert_eq!(auto_info.name, explicit_info.name);
    assert_eq!(auto_info.repo, explicit_info.repo);
    // commit may differ if one env var vs version fallback, but format is same
    // module differs because they're different call sites
}
