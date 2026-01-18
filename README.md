# whereat

[![CI](https://github.com/lilith/whereat/actions/workflows/ci.yml/badge.svg)](https://github.com/lilith/whereat/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/whereat.svg)](https://crates.io/crates/whereat)
[![Documentation](https://docs.rs/whereat/badge.svg)](https://docs.rs/whereat)
[![codecov](https://codecov.io/gh/lilith/whereat/branch/main/graph/badge.svg)](https://codecov.io/gh/lilith/whereat)
[![License](https://img.shields.io/crates/l/whereat.svg)](LICENSE)

**Production error tracing without debuginfo, panic, or overhead.**

*After a decade of distributing server binaries, I'm finally extracting this approach into its own crate!*

In production, you need to immediately know where the bug is `at()` — without panic!, debuginfo, or overhead. Just replace `?` with `.at()?` in your call tree to get beautiful build-time & async-friendly stacktraces with GitHub links.

```
Error: UserNotFound
   at src/db.rs:142:9
      ╰─ user_id = 42
   at src/api.rs:89:5
      ╰─ in handle_request
   at myapp @ https://github.com/you/myapp/blob/a1b2c3d/src/main.rs#L23
```

Compatible with plain enums, errors, structs, thiserror, anyhow, or any type with `Debug`. No changes to your error types required!

## Performance

```text
                                 Error creation time (lower is better)

plain enum              ████ 27ns
thiserror               ████ 27ns
anyhow                  █████ 34ns         ← no location info
whereat (1 frame)       ██████ 40ns        ← file:line:col captured
whereat (3 frames)      ███████ 46ns

With RUST_BACKTRACE=1:
anyhow                  ██████████████████████████████████ 2,500ns (63x slower)
backtrace crate         █████████████████████████████████████████████████████ 6,300ns
panic + catch_unwind    █████████████████ 1,300ns
```

**Fair comparison (same 10-frame depth, 10k errors):**
```text
whereat .at()           ██ 656µs          ← 150x faster than backtrace
panic + catch_unwind    ██████████████████████ 22ms
backtrace crate         ██████████████████████████████████████████████████ 99ms
```

*anyhow/panic only capture backtraces when `RUST_BACKTRACE=1`. whereat always captures location.*

*Linux x86_64. See `cargo bench --bench nested_loops "fair_10fr"` for full results.*

## Quick Start

```rust
// In lib.rs or main.rs - required for at!() and at_crate!() macros
whereat::define_at_crate_info!();

use whereat::{At, ResultAtExt, at};

#[derive(Debug)]
enum MyError {
    NotFound,
    InvalidInput(String),
}

fn find_user(id: u64) -> Result<String, At<MyError>> {
    if id == 0 {
        return Err(at!(MyError::InvalidInput("id cannot be zero".into())));
    }
    Err(at!(MyError::NotFound))
}

fn process(id: u64) -> Result<String, At<MyError>> {
    find_user(id).at_str("looking up user")?;  // Adds context
    Ok("done".into())
}
```

For workspace crates: `whereat::define_at_crate_info!(path = "crates/mylib/");`

## API Overview

**Starting a trace:**

| Function | Works on | Crate info | Use when |
|----------|----------|------------|----------|
| `at!(err)` | Any type | ✅ GitHub links | Default choice with `define_at_crate_info!()` |
| `at(err)` | Any type | ❌ None | Simple usage, no links needed |
| `err.start_at()` | `Error` types | ❌ None | Chaining on error values |

**Extending a trace** (on `Result<T, At<E>>`):

| Method | Effect |
|--------|--------|
| `.at()` | **New frame** at caller's location |
| `.at_str("msg")` | Add context to **last frame** (no new location) |
| `.map_err_at(\|e\| ...)` | Convert error type, preserve trace |

**Key**: `.at()` creates a NEW frame. `.at_str()` adds to the LAST frame. See [Adding Context](#adding-context) for full list.

## Best Practices

**DO: Keep your hot loops zero-alloc**
- You do NOT need `At<>` inside hot loops. Defer tracing until you exit.
- `.at_skipped_frames()` adds a `[...]` marker to indicate frames were skipped.

**DO: Use `at_crate!()` at crate boundaries**
- When consuming errors from other crates, this ensures backtraces show `myapp @ src/lib.rs:42` instead of confusing paths.

**DO: Feel free to add ergonomic aliases**
- `type MyError = At<MyInternalError>` works perfectly.

## Design Philosophy

**You define your own error types.** whereat doesn't impose any structure on your errors — use enums, structs, or whatever suits your domain. whereat just wraps them in `At<E>` to add location+context+crate tracking.

### Which Approach?

| Situation | Use |
|-----------|-----|
| You have an existing struct/enum you don't want to modify | Wrap with `At<YourError>` |
| You want traces embedded inside your error type | Implement `AtTraceable` trait |

**Wrapper approach** (most common): Return `Result<T, At<YourError>>` from functions. The trace lives outside your error type.

**Embedded approach**: Implement `AtTraceable` on your error type and store an `AtTrace` (or `Box<AtTrace>`) field inside it. Return `Result<T, YourError>` directly. See [ADVANCED.md](ADVANCED.md) for details.

This means you can:
- Use `thiserror` for ergonomic `Display`/`From` impls, or `anyhow`
- Use any enum or struct that implements `Debug`
- Define type aliases like `type MyError = At<BaseError>`
- Access your error via `.error()` or deref
- Support nesting with `core::error::Error::source()`

## Features

- **Small sizeof**: `At<E>` is only `sizeof(E) + 8` bytes (one pointer for boxed trace)
- **Zero allocation on Ok path**: No heap allocation until an error occurs
- **Ergonomic API**: `.at()` on Results, `.start_at()` on errors, `.map_err_at()` for trace-preserving conversions
- **Context options**: `.at_str()`, `.at_string()`, `.at_fn()`, `.at_named()`, `.at_data()`, `.at_debug()`, `.at_error()`
- **Cross-crate tracing**: `at!()` and `at_crate!()` macros capture crate info for GitHub/GitLab/Gitea/Bitbucket links
- **Equality/Hashing**: `PartialEq`, `Eq`, `Hash` compare only the error, not the trace
- **no_std compatible**: Works with just `core` + `alloc`

## Adding Context

**Add a new location frame:**
```rust
result.at()?                    // New frame with just file:line:col
result.at_fn(|| {})?            // New frame + captures function name
result.at_named("validation")?  // New frame + custom label
```

**Add context to the last frame** (no new location):
```rust
result.at_str("loading config")?            // Static string (zero-cost)
result.at_string(|| format!("id={}", id))?  // Dynamic string (lazy)
result.at_data(|| path_context)?            // Typed via Display (lazy)
result.at_debug(|| request_info)?           // Typed via Debug (lazy)
result.at_error(io_err)?                    // Attach a source error
```

If the trace is empty, context methods create a frame first. Example:

```rust
// One frame with two contexts attached
let e = at!(MyError).at_str("a").at_str("b");
assert_eq!(e.frame_count(), 1);

// Two frames: at!() creates first, .at() creates second
let e = at!(MyError).at().at_str("on second frame");
assert_eq!(e.frame_count(), 2);
```

## Cross-Crate Tracing

When consuming errors from other crates, use `at_crate!()` to mark the boundary:

```rust
whereat::define_at_crate_info!();

fn call_external() -> Result<(), At<ExternalError>> {
    at_crate!(external_crate::do_thing())?;  // Wraps Result, marks boundary
    Ok(())
}
```

The `at_crate!()` macro takes a **Result** and desugars to:
```rust
result.at_crate(crate::at_crate_info())  // Adds your crate's info as boundary marker
```

This ensures traces show `myapp @ src/lib.rs:42` instead of confusing paths from dependencies.

## Hot Loops

Don't trace inside hot loops. Defer until you exit:

```rust
fn process_batch(items: &[Item]) -> Result<(), MyError> {
    for item in items {
        process_one(item)?;  // Plain Result here, no At<>
    }
    Ok(())
}

fn caller() -> Result<(), At<MyError>> {
    process_batch(&items)
        .map_err(|e| at!(e).at_skipped_frames())?;  // Wrap on exit, mark skipped
    Ok(())
}
```

## Advanced Usage

See [ADVANCED.md](ADVANCED.md) for:
- Embedded traces with `AtTraceable` trait
- Custom storage options (inline vs boxed)
- Complex workspace layouts
- Link format customization (GitLab, Gitea, Bitbucket)
- Inline storage features for reduced allocations

## License

MIT OR Apache-2.0
