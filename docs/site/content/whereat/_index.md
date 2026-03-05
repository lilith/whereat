+++
title = "whereat"
description = "Production error tracing without debuginfo, panic, or overhead"
sort_by = "weight"
weight = 1

[extra]
sidebar = true
+++

# whereat

> Know where the bug is `at()` — without panic!, debuginfo, or overhead.

In production, you strip debuginfo and disable backtraces. When something fails, you get an error type and nothing else. whereat fixes this: replace `?` with `.at()?` in your call tree, and every error carries a trace of file:line:col locations — captured at compile time, zero cost on the happy path.

```
Error: UserNotFound
   at src/db.rs:142:9
      ╰─ user_id = 42
   at src/api.rs:89:5
      ╰─ in handle_request
   at myapp @ https://github.com/you/myapp/blob/a1b2c3d/src/main.rs#L23
```

## Installation

```toml
[dependencies]
whereat = "0.1"
```

Requires Rust 1.85+ (2024 edition). Works with `no_std` + `alloc` by default.

For cross-crate tracing with GitHub links, add this to your crate root:

```rust
whereat::define_at_crate_info!();
```

### Optional Features

| Feature | What It Does |
|---------|-------------|
| `std` | Adds `std::error::Error` impls (not needed on 1.85+, `core::error` works) |
| `_termcolor` | Terminal-colored output via `owo-colors` |
| `_html` | HTML-formatted output with Catppuccin Mocha theme |
| `_tinyvec-128-bytes` | Inline location storage (12 slots, reduces allocations) |
| `_smallvec-128-bytes` | Inline location storage via smallvec |

## Basic Usage

Replace `?` with `.at()?` anywhere you propagate errors:

```rust
use whereat::{at, At, ResultAtExt};

#[derive(Debug)]
enum AppError {
    NotFound,
    InvalidInput(String),
}

fn find_user(id: u64) -> Result<String, At<AppError>> {
    if id == 0 {
        return Err(at(AppError::InvalidInput("id cannot be zero".into())));
    }
    Err(at(AppError::NotFound))
}

fn handle_request(id: u64) -> Result<String, At<AppError>> {
    let user = find_user(id)
        .at_str("looking up user")?;
    Ok(user)
}
```

Output:

```
Error: NotFound
    at src/main.rs:10:5
       ╰─ looking up user
    at src/main.rs:5:9
```

`At<E>` derefs to `E`, so you can match on it directly:

```rust
match *err {
    AppError::NotFound => println!("not found"),
    AppError::InvalidInput(ref msg) => println!("bad input: {}", msg),
}
```

## How It Works

{% mermaid() %}
graph TB
    subgraph "At&lt;E&gt; — sizeof(E) + 8 bytes"
        E["error: E (inline)"]
        T["trace: AtTraceBoxed (8 bytes)"]
    end
    T -->|"Option&lt;Box&lt;…&gt;&gt;"| Trace["AtTrace (heap)"]
    subgraph "AtTrace"
        L["locations: InlineVec&lt;4&gt;"]
        C["contexts: Option&lt;Box&lt;Vec&lt;…&gt;&gt;&gt;"]
        CI["crate_info: Option&lt;&'static AtCrateInfo&gt;"]
    end
{% end %}

`At<E>` stores your error inline. The trace is a single `Option<Box<AtTrace>>` — 8 bytes on 64-bit thanks to null-pointer optimization. When no frames exist, the box is `None` and nothing is heap-allocated.

Every `.at()` and `.at_str()` method is annotated with `#[track_caller]`, which makes `Location::caller()` return the *call site's* file:line:col. This is baked into the binary as static data — no runtime stack walking, no debug symbols needed.

### Frames vs Contexts

{% mermaid() %}
graph TB
    subgraph "Frame 1 — at src/handler.rs:42:5"
        C1["╰─ 'processing request'"]
        C2["╰─ request_id = 7"]
    end
    subgraph "Frame 2 — at src/db.rs:89:9"
        C3["╰─ 'user lookup failed'"]
    end
    subgraph "Frame 3 — at src/db.rs:15:13"
        direction TB
        N["(no context)"]
    end
{% end %}

A **frame** is a location (file:line:col). A **context** is metadata attached to a frame.

- `.at()` creates a new frame
- `.at_str("msg")` adds context to the *last* frame (no new location)
- `.at_fn(|| {})` creates a new frame AND captures the function name
- `.at_data(|| val)` adds typed context via Display to the last frame

You can attach multiple contexts to one frame:

```rust
result
    .at_str("loading user profile")
    .at_data(|| user_id)?
```

Both contexts share one frame — no extra location captured.

## API Reference

**Starting a trace:**

| Function | Crate Info | Use When |
|----------|------------|----------|
| `at!(err)` | GitHub links | Default choice (needs `define_at_crate_info!()`) |
| `at(err)` | None | Simple usage, no links needed |
| `err.start_at()` | None | Chaining on `Error` types |

**Extending a trace** (on `Result<T, At<E>>`):

| Method | Effect |
|--------|--------|
| `.at()` | New frame at caller's location |
| `.at_str("msg")` | Context on last frame (no new location) |
| `.at_fn(\|\| {})` | New frame + captures function name |
| `.at_named("step")` | New frame + custom label |
| `.at_data(\|\| val)` | Context via Display (lazy) |
| `.at_debug(\|\| val)` | Context via Debug (lazy) |
| `.at_error(source)` | Attach a source error |
| `.at_string(\|\| format!(...))` | Dynamic string context (lazy) |
| `.map_err_at(\|e\| ...)` | Convert error type, preserve trace |

**Formatters:**

| Formatter | Shows |
|-----------|-------|
| `format!("{:?}", err)` | Debug — full trace with all contexts |
| `format!("{}", err)` | Display — just the error message |
| `err.full_trace()` | Message + locations + all contexts |
| `err.last_error_trace()` | Message + locations (no contexts) |
| `err.display_with_meta()` | Full trace with repository links |
| `err.display_color()` | Colored terminal output (feature `_termcolor`) |
| `err.display_html_styled()` | HTML with embedded CSS (feature `_html`) |

## Cross-Crate Tracing

When errors cross crate boundaries, file paths alone are ambiguous. `src/lib.rs:42` — which crate?

```rust
// In lib.rs or main.rs
whereat::define_at_crate_info!();

fn call_library() -> Result<(), At<LibError>> {
    at_crate!(lib::do_thing())?;  // marks boundary
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

`define_at_crate_info!()` captures crate name, repo URL, and commit hash at compile time. It auto-detects your forge from the repo URL:

| Forge | Link Pattern |
|-------|-------------|
| GitHub | `repo/blob/commit/path/file#Lline` |
| GitLab | `repo/-/blob/commit/path/file#Lline` |
| Gitea/Forgejo | `repo/src/commit/commit/path/file#Lline` |
| Bitbucket | `repo/src/commit/path/file#lines-line` |

You can override with `GITLAB_LINK_FORMAT`, `BITBUCKET_LINK_FORMAT`, etc., or define a custom format string with `{repo}`, `{commit}`, `{path}`, `{file}`, `{line}` placeholders.

In a workspace, each crate that uses `at!()` or `at_crate!()` needs its own `define_at_crate_info!()` call.

## Performance

{% mermaid() %}
graph LR
    subgraph "whereat"
        A["Location::caller()"] -->|"static ref"| B["Push to InlineVec"]
        B -->|"4 inline slots"| C["Done"]
    end
    subgraph "backtrace"
        D["Walk stack frames"] -->|"per frame"| E["Resolve symbols"]
        E -->|"read DWARF"| F["Format addresses"]
    end
{% end %}

`#[track_caller]` bakes file:line:col into the binary as static data. `Location::caller()` returns a pointer to data that already exists. No stack walking, no symbol resolution, no debug info parsing. The only runtime cost is pushing that pointer into a small vector.

```text
                                 Error creation time (lower is better)

Ok path (no error)      █ <1ns            ← ZERO overhead on success
plain enum error        █ <1ns
whereat (1 frame)       ███ 18ns          ← file:line:col captured
whereat (3 frames)      ███ 19ns
whereat (10 frames)     ██████████ 67ns

With RUST_BACKTRACE=1:
anyhow                  █████████████████████████████████████████████████ 2,500ns
backtrace crate         █████████████████████████████████████████████████████████████████ 6,300ns
panic + catch_unwind    ██████████████ 1,300ns
```

**Same-depth comparison (10 frames, 10k iterations):**

```text
whereat .at()           █ 1.2ms
panic + catch_unwind    ██████████████████████ 27ms
backtrace crate         ████████████████████████████████████████████████████████████████████████████ 119ms
```

Linux x86_64 (WSL2), 2026-01-18. See `cargo bench --bench overhead` and `cargo bench --bench nested_loops "fair_10fr"`.

### Allocation Behavior

**Default (no features):** 4 inline location slots. For ≤4 frames, one allocation total (the `Box<AtTrace>`).

**With inline features:** `_tinyvec-128-bytes` or `_smallvec-128-bytes` bumps to 12 inline slots.

**Contexts** are separately allocated only when you call `.at_str()`, `.at_data()`, etc. Plain `.at()` never allocates contexts.

**OOM handling:** Vec operations use `try_reserve` and silently skip on OOM. Your error `E` always propagates (stored inline), even if tracing fails.

### Hot Loops

Don't trace inside hot loops. Defer until you exit:

```rust
fn process_batch(items: &[Item]) -> Result<(), MyError> {
    for item in items {
        process_one(item)?;  // Plain Result, no At<>
    }
    Ok(())
}

fn caller() -> Result<(), At<MyError>> {
    process_batch(&items)
        .map_err(|e| at(e).at_skipped_frames())?;
    Ok(())
}
```

`.at_skipped_frames()` adds a `[...]` marker in the trace.

## Embedded Traces (AtTraceable)

The default approach wraps errors: `Result<T, At<E>>`. If you want the trace *inside* your error type — so callers see `Result<T, MyError>` — implement `AtTraceable`.

{% mermaid() %}
graph TD
    Q["Do you control the error type?"]
    Q -->|No| W["Use At&lt;E&gt; wrapper"]
    Q -->|Yes| Q2["Want callers to see your type directly?"]
    Q2 -->|"Yes — Result&lt;T, MyError&gt;"| T["Implement AtTraceable"]
    Q2 -->|"No — At&lt;E&gt; is fine"| W
{% end %}

```rust
use whereat::{AtTrace, AtTraceable, ResultAtTraceableExt};
use core::fmt;

struct MyError {
    kind: ErrorKind,
    trace: AtTrace,
}

impl AtTraceable for MyError {
    fn trace_mut(&mut self) -> &mut AtTrace {
        &mut self.trace
    }

    fn trace(&self) -> Option<&AtTrace> {
        Some(&self.trace)
    }

    fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}
```

Then use `ResultAtTraceableExt` instead of `ResultAtExt` — same API, works on `Result<T, MyError>`.

### Storage Options

| Field Type | Extra Size | Behavior |
|------------|------------|----------|
| `AtTrace` | ~40 bytes | Trace always present; captured at construction |
| `Box<AtTrace>` | 8 bytes | One heap allocation; trace always present |
| `Option<Box<AtTrace>>` | 8 bytes | Lazy — no allocation until first `.at_*()` call |

### Converting Between Approaches

```rust
// At<A> → At<B>: change error type, keep trace
let b: At<KindB> = a.map_error(|kind_a| convert(kind_a));

// At<E> → MyTraceable: transfer trace into embedded type
let my_err: MyError = at_err.into_traceable(|kind| MyError::from(kind));

// MyTraceable → At<B>: extract trace into wrapper
let at_b: At<KindB> = my_err.into_at(|e| convert(e.kind));
```
