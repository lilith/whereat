+++
title = "How At<E> Works"
description = "Memory layout, tracing mechanics, and design tradeoffs"
weight = 1
+++

# How At&lt;E&gt; Works

## Memory Layout

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

`At<E>` stores your error inline. The trace is a single `Option<Box<AtTrace>>` — 8 bytes on 64-bit platforms thanks to null-pointer optimization. When no frames have been captured (e.g., `At::wrap(err)` without calling `.at()`), the box is `None` and no heap allocation occurs.

`AtTrace` stores locations in an `InlineVec` with 4 inline slots (default). For most error paths (≤4 frames), the locations stay on the heap allocation that holds the `AtTrace` itself — no secondary allocation. Contexts (strings, typed data, source errors) are stored separately and only allocated when you actually add context.

## What #[track_caller] Does

Every `.at()`, `.at_str()`, and similar method is annotated with `#[track_caller]`. This is a compiler intrinsic that makes `Location::caller()` return the location of the *call site*, not the function body.

```rust
#[track_caller]
fn at(self) -> Self {
    // Location::caller() returns the file:line:col where .at() was called
    let loc = Location::caller();
    // Store it in the trace
}
```

This happens at compile time — the location is baked into the binary as static data. No runtime stack walking, no debug symbols needed.

## Frames vs Contexts

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
- `.at_str("msg")` adds a context to the *last* frame (or creates one if empty)
- `.at_fn(|| {})` creates a new frame AND captures the function name as context
- `.at_data(|| val)` adds typed context via Display to the last frame

This means you can attach multiple pieces of context to a single location:

```rust
result
    .at_str("loading user profile")  // context on same frame
    .at_data(|| user_id)?            // another context on same frame
```

Both contexts share one frame — no extra location captured.

## Performance

All cost is on the error path. The Ok path is free:

```rust
// This compiles to a branch + return. Zero allocation.
let user = find_user(id).at()?;
```

On the error path, each `.at()` call does:
1. `Location::caller()` — free, it's a static reference
2. Push location into the InlineVec — O(1), inline for first 4 frames
3. Box the trace on first frame if not already boxed — one allocation

Per-frame cost: ~18ns for the first frame (includes boxing), ~6ns for subsequent frames within inline capacity.

## Equality and Hashing

`At<E>` implements `PartialEq`, `Eq`, and `Hash` by delegating to `E`. The trace is ignored for comparison purposes. This means you can match, compare, and hash wrapped errors the same way you would unwrapped ones.
