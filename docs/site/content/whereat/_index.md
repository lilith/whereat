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

In production, you strip debuginfo and disable backtraces. When something fails, you get an error type and nothing else. whereat fixes this: replace `?` with `.at()?` in your call tree, and every error carries a trace of file:line:col locations through your code — captured at compile time, zero cost on the happy path.

```
Error: UserNotFound
   at src/db.rs:142:9
      ╰─ user_id = 42
   at src/api.rs:89:5
      ╰─ in handle_request
   at myapp @ https://github.com/you/myapp/blob/a1b2c3d/src/main.rs#L23
```

## How It Works

{% mermaid() %}
graph LR
    A["at(err)"] -->|"sizeof(E) + 8 bytes"| B["At&lt;E&gt;"]
    B -->|".at()?"| C["New frame added"]
    C -->|".at_str('context')?"| D["Context on last frame"]
    D -->|"propagates up"| E["Full trace at handler"]
{% end %}

`At<E>` wraps your error inline and stores a boxed trace. Each `.at()` call adds a frame with the caller's `file:line:col`. Context methods (`.at_str()`, `.at_data()`, etc.) attach metadata to the last frame without adding a new location.

The trace is heap-allocated on first error — the Ok path does zero work.

## API at a Glance

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

**Key distinction**: `.at()` creates a NEW frame. `.at_str()` and other context methods add to the LAST frame.

## Next Steps

- [Installation](@/whereat/getting-started/installation.md) — Add whereat to your project
- [Basic Usage](@/whereat/getting-started/basic-usage.md) — Your first traced error
- [How At&lt;E&gt; Works](@/whereat/concepts/how-at-works.md) — Understand the wrapper type
- [Cross-Crate Tracing](@/whereat/concepts/cross-crate.md) — GitHub links and crate boundaries
- [Advanced: AtTraceable](@/whereat/advanced/traceable.md) — Embed traces in your error types
