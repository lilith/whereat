//! Crate metadata for cross-crate error tracing with repository links.
//!
//! This module provides [`AtCrateInfo`] and [`AtCrateInfoBuilder`] for capturing
//! static metadata about crates, enabling clickable GitHub links in error traces.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

// ============================================================================
// AtCrateInfo - Static metadata about a crate for cross-crate tracing
// ============================================================================

/// Static metadata about a crate, used for generating repository links.
///
/// Create using [`AtCrateInfo::builder()`] for a fluent const-compatible API,
/// or use the [`define_at_crate_info!()`](crate::define_at_crate_info!) macro for automatic capture.
///
/// ## Builder Pattern (Recommended)
///
/// ```rust
/// use errat::AtCrateInfo;
///
/// static INFO: AtCrateInfo = AtCrateInfo::builder()
///     .name("mylib")
///     .repo(Some("https://github.com/org/repo"))
///     .commit(Some("abc123"))
///     .path(Some("crates/mylib/"))
///     .build();
/// ```
///
/// ## With Custom Metadata
///
/// ```rust
/// use errat::AtCrateInfo;
///
/// static INFO: AtCrateInfo = AtCrateInfo::builder()
///     .name("mylib")
///     .meta(&[("team", "platform"), ("service", "auth")])
///     .build();
/// ```
#[derive(Debug, Clone, Copy)]
pub struct AtCrateInfo {
    name: &'static str,
    repo: Option<&'static str>,
    commit: Option<&'static str>,
    crate_path: Option<&'static str>,
    module: &'static str,
    meta: &'static [(&'static str, &'static str)],
}

impl AtCrateInfo {
    /// Create a builder for constructing AtCrateInfo with a fluent API.
    ///
    /// All builder methods are `const fn`, so you can use this in static contexts.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::AtCrateInfo;
    ///
    /// static INFO: AtCrateInfo = AtCrateInfo::builder()
    ///     .name(env!("CARGO_PKG_NAME"))
    ///     .repo(option_env!("CARGO_PKG_REPOSITORY"))
    ///     .build();
    /// ```
    pub const fn builder() -> AtCrateInfoBuilder {
        AtCrateInfoBuilder::new()
    }

    /// Crate name (from CARGO_PKG_NAME).
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Repository URL (from CARGO_PKG_REPOSITORY).
    pub const fn repo(&self) -> Option<&'static str> {
        self.repo
    }

    /// Git commit hash or tag for generating permalinks.
    pub const fn commit(&self) -> Option<&'static str> {
        self.commit
    }

    /// Path from repository root to crate (e.g., "crates/mylib/").
    pub const fn crate_path(&self) -> Option<&'static str> {
        self.crate_path
    }

    /// Module path where this info was captured.
    pub const fn module(&self) -> &'static str {
        self.module
    }

    /// Custom key-value metadata slice.
    pub const fn meta(&self) -> &'static [(&'static str, &'static str)] {
        self.meta
    }

    /// Look up a custom metadata value by key.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::AtCrateInfo;
    ///
    /// static INFO: AtCrateInfo = AtCrateInfo::builder()
    ///     .name("mylib")
    ///     .meta(&[("team", "platform")])
    ///     .build();
    ///
    /// assert_eq!(INFO.get_meta("team"), Some("platform"));
    /// assert_eq!(INFO.get_meta("unknown"), None);
    /// ```
    pub const fn get_meta(&self, key: &str) -> Option<&'static str> {
        let mut i = 0;
        while i < self.meta.len() {
            let (k, v) = self.meta[i];
            if const_str_eq(k, key) {
                return Some(v);
            }
            i += 1;
        }
        None
    }
}

/// Const-compatible string equality check.
const fn const_str_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Builder for [`AtCrateInfo`] with a fluent, const-compatible API.
///
/// All methods are `const fn`, enabling use in static/const contexts.
///
/// ## Example
///
/// ```rust
/// use errat::AtCrateInfo;
///
/// static INFO: AtCrateInfo = AtCrateInfo::builder()
///     .name("mylib")
///     .repo(Some("https://github.com/org/repo"))
///     .commit(option_env!("GIT_COMMIT"))
///     .path(Some("crates/mylib/"))
///     .meta(&[("team", "platform")])
///     .build();
/// ```
#[derive(Debug, Clone, Copy)]
pub struct AtCrateInfoBuilder {
    name: &'static str,
    repo: Option<&'static str>,
    commit: Option<&'static str>,
    crate_path: Option<&'static str>,
    module: &'static str,
    meta: &'static [(&'static str, &'static str)],
}

impl AtCrateInfoBuilder {
    /// Create a new builder with default values.
    pub const fn new() -> Self {
        Self {
            name: "",
            repo: None,
            commit: None,
            crate_path: None,
            module: "",
            meta: &[],
        }
    }

    /// Set the crate name.
    pub const fn name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    /// Set the repository URL.
    pub const fn repo(mut self, repo: Option<&'static str>) -> Self {
        self.repo = repo;
        self
    }

    /// Set the git commit hash or version tag.
    pub const fn commit(mut self, commit: Option<&'static str>) -> Self {
        self.commit = commit;
        self
    }

    /// Set the crate path within the repository (for workspace crates).
    pub const fn path(mut self, path: Option<&'static str>) -> Self {
        self.crate_path = path;
        self
    }

    /// Set the module path.
    pub const fn module(mut self, module: &'static str) -> Self {
        self.module = module;
        self
    }

    /// Set custom key-value metadata.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use errat::AtCrateInfo;
    ///
    /// static INFO: AtCrateInfo = AtCrateInfo::builder()
    ///     .name("mylib")
    ///     .meta(&[
    ///         ("team", "platform"),
    ///         ("service", "auth"),
    ///         ("oncall", "platform-oncall@example.com"),
    ///     ])
    ///     .build();
    /// ```
    pub const fn meta(mut self, meta: &'static [(&'static str, &'static str)]) -> Self {
        self.meta = meta;
        self
    }

    /// Build the final AtCrateInfo.
    pub const fn build(self) -> AtCrateInfo {
        AtCrateInfo {
            name: self.name,
            repo: self.repo,
            commit: self.commit,
            crate_path: self.crate_path,
            module: self.module,
            meta: self.meta,
        }
    }

    // ========================================================================
    // Runtime (owned) variants - these leak strings for 'static lifetime
    // ========================================================================

    /// Set the crate name from an owned string (leaks memory for static lifetime).
    ///
    /// Use for runtime configuration with `OnceLock`.
    pub fn name_owned(mut self, name: String) -> Self {
        self.name = Box::leak(name.into_boxed_str());
        self
    }

    /// Set the repository URL from an owned string (leaks memory for static lifetime).
    pub fn repo_owned(mut self, repo: Option<String>) -> Self {
        self.repo = repo.map(|s| {
            let leaked: &'static str = Box::leak(s.into_boxed_str());
            leaked
        });
        self
    }

    /// Set the commit hash from an owned string (leaks memory for static lifetime).
    pub fn commit_owned(mut self, commit: Option<String>) -> Self {
        self.commit = commit.map(|s| {
            let leaked: &'static str = Box::leak(s.into_boxed_str());
            leaked
        });
        self
    }

    /// Set the crate path from an owned string (leaks memory for static lifetime).
    pub fn path_owned(mut self, path: Option<String>) -> Self {
        self.crate_path = path.map(|s| {
            let leaked: &'static str = Box::leak(s.into_boxed_str());
            leaked
        });
        self
    }

    /// Set the module path from an owned string (leaks memory for static lifetime).
    pub fn module_owned(mut self, module: String) -> Self {
        self.module = Box::leak(module.into_boxed_str());
        self
    }

    /// Set custom metadata from owned strings (leaks memory for static lifetime).
    ///
    /// ## Example
    ///
    /// ```rust
    /// use std::sync::OnceLock;
    /// use errat::AtCrateInfo;
    ///
    /// static CRATE_INFO: OnceLock<AtCrateInfo> = OnceLock::new();
    ///
    /// fn init_crate_info(instance_id: String) {
    ///     CRATE_INFO.get_or_init(|| {
    ///         AtCrateInfo::builder()
    ///             .name("mylib")
    ///             .module("mylib")
    ///             .meta_owned(vec![
    ///                 ("instance".into(), instance_id),
    ///             ])
    ///             .build()
    ///     });
    /// }
    /// ```
    pub fn meta_owned(mut self, entries: Vec<(String, String)>) -> Self {
        let leaked: &'static [(&'static str, &'static str)] = Box::leak(
            entries
                .into_iter()
                .map(|(k, v)| {
                    let k: &'static str = Box::leak(k.into_boxed_str());
                    let v: &'static str = Box::leak(v.into_boxed_str());
                    (k, v)
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        self.meta = leaked;
        self
    }
}

impl Default for AtCrateInfoBuilder {
    fn default() -> Self {
        Self::new()
    }
}
