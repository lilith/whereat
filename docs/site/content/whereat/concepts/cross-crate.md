+++
title = "Cross-Crate Tracing"
description = "GitHub links and crate boundary markers"
weight = 2
+++

# Cross-Crate Tracing

When errors cross crate boundaries, file paths alone can be confusing. `src/lib.rs:42` — which crate? whereat solves this with crate metadata.

## Setup

In your crate root:

```rust
whereat::define_at_crate_info!();
```

This macro defines a static `AtCrateInfo` and a `pub(crate) fn at_crate_info()` getter. It captures at compile time:
- `CARGO_PKG_NAME` — your crate name
- `CARGO_PKG_REPOSITORY` — repo URL from Cargo.toml
- Git commit hash (via `GIT_COMMIT`, `GITHUB_SHA`, or `CI_COMMIT_SHA` env vars, with `v{VERSION}` fallback)

## Using at!()

The `at!()` macro wraps an error AND attaches your crate's metadata:

```rust
whereat::define_at_crate_info!();
use whereat::{at, At};

fn find_user(id: u64) -> Result<String, At<MyError>> {
    Err(at!(MyError::NotFound))  // includes crate info
}
```

Traces then include clickable source links:

```
Error: NotFound
    at src/db.rs:5:9
       https://github.com/you/repo/blob/abc123/src/db.rs#L5
```

## Crate Boundaries

When consuming errors from another crate, use `at_crate!()` to mark the boundary:

```rust
fn call_library() -> Result<(), At<LibError>> {
    at_crate!(lib::do_thing())?;  // marks "errors below here came from lib"
    Ok(())
}
```

{% mermaid() %}
graph TB
    subgraph "your-app"
        F1["at src/main.rs:23:5"]
    end
    B["─── your-app (above) → some-lib (below) ───"]
    subgraph "some-lib"
        F2["at src/lib.rs:42:9"]
        F3["at src/internal.rs:15:13"]
    end
    F1 --> B --> F2 --> F3
{% end %}

The boundary shows up in formatted output so you can tell which crate each frame belongs to.

## Supported Forges

whereat auto-detects the link format from your repository URL:

| Forge | URL Pattern | Link Format |
|-------|-------------|-------------|
| GitHub | `github.com` | `repo/blob/commit/path/file#Lline` |
| GitLab | `gitlab.com` or `gitlab.` | `repo/-/blob/commit/path/file#Lline` |
| Gitea/Forgejo | `gitea.` or `codeberg.org` | `repo/src/commit/commit/path/file#Lline` |
| Bitbucket | `bitbucket.org` | `repo/src/commit/path/file#lines-line` |

You can also set it manually:

```rust
use whereat::{AtCrateInfo, GITLAB_LINK_FORMAT};

static INFO: AtCrateInfo = AtCrateInfo::builder()
    .name("mylib")
    .repo(Some("https://gitlab.com/org/repo"))
    .link_format(GITLAB_LINK_FORMAT)
    .build();
```

Or define a custom format with `{repo}`, `{commit}`, `{path}`, `{file}`, `{line}` placeholders.
