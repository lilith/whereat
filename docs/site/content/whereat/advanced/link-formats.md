+++
title = "Link Formats"
description = "Customize source links for GitHub, GitLab, Gitea, Bitbucket, or custom forges"
weight = 3
+++

# Link Formats

## Auto-Detection

`define_at_crate_info!()` auto-detects your forge from the repository URL in Cargo.toml:

```toml
[package]
repository = "https://github.com/you/repo"  # → GitHub format
```

The `link_format_auto()` builder method also detects from URLs at runtime.

## Supported Forges

| Forge | Constant | Link Pattern |
|-------|----------|-------------|
| GitHub | `GITHUB_LINK_FORMAT` | `{repo}/blob/{commit}/{path}{file}#L{line}` |
| GitLab | `GITLAB_LINK_FORMAT` | `{repo}/-/blob/{commit}/{path}{file}#L{line}` |
| Gitea/Forgejo | `GITEA_LINK_FORMAT` | `{repo}/src/commit/{commit}/{path}{file}#L{line}` |
| Bitbucket | `BITBUCKET_LINK_FORMAT` | `{repo}/src/{commit}/{path}{file}#lines-{line}` |

## Manual Selection

```rust
use whereat::{AtCrateInfo, GITLAB_LINK_FORMAT};

static INFO: AtCrateInfo = AtCrateInfo::builder()
    .name("mylib")
    .repo(Some("https://gitlab.com/org/repo"))
    .commit(Some("abc123"))
    .link_format(GITLAB_LINK_FORMAT)
    .build();
```

## Custom Format

Define your own format string with these placeholders:

| Placeholder | Replaced With |
|-------------|--------------|
| `{repo}` | Repository URL (trailing slash stripped) |
| `{commit}` | Commit hash or version tag |
| `{path}` | Path from git root to crate (e.g., `crates/mylib/`) |
| `{file}` | Source file path (e.g., `src/lib.rs`) |
| `{line}` | Line number |

```rust
const MY_FORMAT: &str = "{repo}/browse/{path}{file}?at={commit}#L{line}";

static INFO: AtCrateInfo = AtCrateInfo::builder()
    .name("mylib")
    .link_format(MY_FORMAT)
    .build();
```

## Runtime Construction

For dynamic environments where you compute paths at startup:

```rust
use std::sync::OnceLock;
use whereat::AtCrateInfo;

static CRATE_INFO: OnceLock<AtCrateInfo> = OnceLock::new();

pub(crate) fn at_crate_info() -> &'static AtCrateInfo {
    CRATE_INFO.get_or_init(|| {
        let path = std::env::var("CRATE_PATH_IN_REPO")
            .unwrap_or_else(|_| "crates/mylib/".into());

        AtCrateInfo::builder()
            .name(env!("CARGO_PKG_NAME"))
            .repo(option_env!("CARGO_PKG_REPOSITORY"))
            .commit(option_env!("GIT_COMMIT"))
            .path_owned(Some(path))
            .build()
    })
}
```

The `_owned()` builder methods (`repo_owned`, `commit_owned`, `path_owned`) accept `String` and leak them via `Box::leak` for `'static` lifetime.
