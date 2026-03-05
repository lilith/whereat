+++
title = "Performance"
description = "Benchmarks and allocation behavior"
weight = 3
+++

# Performance

## Benchmarks

Measured on Linux x86_64 (WSL2), 2026-01-18. Run `cargo bench` to reproduce.

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
backtrace crate         ████████████████████████████████████████████████████████████████████████████████████████████████ 119ms
```

See `cargo bench --bench overhead` and `cargo bench --bench nested_loops "fair_10fr"` for full results.

## Why It's Fast

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

`#[track_caller]` bakes file:line:col into the binary as static data. `Location::caller()` returns a `&'static Location` — a pointer to data that already exists. There's no stack walking, no symbol resolution, no debug info parsing.

The only runtime cost is pushing that pointer into a small vector.

## Allocation Behavior

**Default (no features):** AtTrace uses 4 inline location slots. For traces with ≤4 frames, the only allocation is the `Box<AtTrace>` itself — one allocation total on the error path.

**With inline features:** `_tinyvec-128-bytes` or `_smallvec-128-bytes` bumps inline capacity to 12 slots.

**Contexts** (strings, typed data) are separately allocated only when you call `.at_str()`, `.at_data()`, etc. Plain `.at()` never allocates contexts.

**OOM handling:** Vec operations use `try_reserve` and silently skip on OOM. The error `E` is always stored inline in `At<E>`, so your error always propagates even if tracing fails. `Box::new` can still panic (waiting for `Box::try_new` stabilization).

## Hot Loops

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

`.at_skipped_frames()` adds a `[...]` marker in the trace to indicate frames were omitted.
