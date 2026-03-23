# whereat

[![CI](https://github.com/lilith/whereat/actions/workflows/ci.yml/badge.svg)](https://github.com/lilith/whereat/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/whereat.svg)](https://crates.io/crates/whereat)
[![Documentation](https://docs.rs/whereat/badge.svg)](https://docs.rs/whereat)
[![codecov](https://codecov.io/gh/lilith/whereat/branch/main/graph/badge.svg)](https://codecov.io/gh/lilith/whereat)
[![License](https://img.shields.io/crates/l/whereat.svg)](LICENSE)
[![Docs Site](https://img.shields.io/badge/docs-site-blue)](https://lilith.github.io/whereat/)

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

Ok path (no error)      █ <1ns            ← ZERO overhead on success
plain enum error        █ <1ns
whereat (1 frame)       ███ 18ns          ← file:line:col captured
whereat (3 frames)      ███ 19ns
whereat (10 frames)     ██████████ 67ns

With RUST_BACKTRACE=1:
anyhow                  █████████████████████████████████████████████████ 2,500ns
backtrace crate         ████████████████████████████████████████████████████████████████████████████████████████████████████ 6,300ns
panic + catch_unwind    ██████████████████████████ 1,300ns
```

**Fair comparison (same 10-frame depth, 10k iterations):**
```text
whereat .at()           █ 1.2ms           ← 100x faster than backtrace
panic + catch_unwind    ██████████████████████ 27ms
backtrace crate         ████████████████████████████████████████████████████████████████████████████████████████████████████ 119ms
```

*anyhow/panic only capture backtraces when `RUST_BACKTRACE=1`. whereat always captures location.*

*Linux x86_64 (WSL2), 2026-01-18. See `cargo bench --bench overhead` and `cargo bench --bench nested_loops "fair_10fr"`.*

## Quick Start

Two things to remember: `at!()` creates a traced error, `.at()?` propagates it.

```rust
whereat::define_at_crate_info!();  // once in lib.rs — enables GitHub links in traces

use whereat::prelude::*;  // At, at, ResultAtExt, ErrorAtExt

#[derive(Debug)]
enum DbError { NotFound, ConnectionFailed }

fn get_user(id: u64) -> Result<String, At<DbError>> {
    if id == 0 { return Err(at!(DbError::NotFound)); }  // at!() starts the trace
    Ok("alice".into())
}

fn get_email(id: u64) -> Result<String, At<DbError>> {
    let name = get_user(id).at()?;  // .at()? adds this call site to the trace
    Ok(format!("{}@example.com", name))
}
```

That's it. If `get_user` fails, the trace shows every `.at()` call site it passed through — with clickable GitHub links.

For workspace crates: `whereat::define_at_crate_info!(path = "crates/mylib/");`

### Context and error type conversion

Real code has multiple error types. Use `.at_str()` to add context and `.map_err_at()` to convert between error types without losing the trace:

```rust
#[derive(Debug)]
enum ApiError { Db(DbError), BadRequest(String) }

type ApiResult<T> = Result<T, At<ApiError>>;  // Result alias — recommended for every crate

fn handle_request(id: u64) -> ApiResult<()> {
    let email = get_email(id)
        .map_err_at(ApiError::Db)?;          // DbError → ApiError, trace preserved
    send_welcome(&email)
        .at()                                 // new frame at this call site
        .at_str("sending welcome email")?;    // context on that frame
    Ok(())
}
```

`map_err_at` transforms the inner error while keeping the trace. This is the #1 pattern to get right — see [Avoiding Trace Loss](#avoiding-trace-loss).

## API Overview

**Starting a trace:**

| Function | Works on | Crate info | Use when |
|----------|----------|------------|----------|
| `at(err)` | Any type | ❌ None | General use, no setup |
| `at!(err)` | Any type | ✅ GitHub links | Production — requires `define_at_crate_info!()` |
| `err.start_at()` | `Error` types | ❌ None | Chaining on error values |

**Extending a trace** (on `Result<T, At<E>>`):

| Method | Effect |
|--------|--------|
| `.at()` | **New frame** at caller's location |
| `.at_str("msg")` | Add context to **last frame** (no new location) |
| `.map_err_at(\|e\| ...)` | Convert error type, **preserve trace** |

**Inspecting / decomposing:**

| Method | Effect |
|--------|--------|
| `.error()` | Borrow the inner `&E` |
| `.decompose()` | Consume into `(E, Option<AtTrace>)` — preserves the trace |
| `.map_error(\|e\| ...)` | Convert error type inside `At<E>`, preserving trace |

**Key**: `.at()` creates a NEW frame. `.at_str()` adds to the LAST frame. See [Adding Context](#adding-context) for full list.

## Avoiding Trace Loss

whereat only works if you keep the trace alive as errors propagate. These patterns silently destroy traces — **avoid them**.

### Never use `.into_inner()` during error propagation

`into_inner()` is deprecated since 0.1.4 because it discards the trace. Use `decompose()` to get both error and trace, or `map_error()` / `map_err_at()` to convert types while preserving the trace.

```rust
// WRONG — trace is gone
let bare_err = at_err.into_inner();
return Err(at(MyError::Sub(bare_err)));

// RIGHT — trace preserved
return inner_call().map_err_at(|e| MyError::Sub(e));
```

### Never implement `From<At<X>> for Y`

This gets invoked by `?` and discards the `At<>` wrapper (and its trace):

```rust
// WRONG — ? uses this From impl, trace dies
impl From<At<BufferError>> for TiffError {
    fn from(e: At<BufferError>) -> Self {
        TiffError::Buffer(e.into_inner())  // trace lost!
    }
}

// RIGHT — implement From on the bare types, convert with map_err_at
impl From<BufferError> for TiffError {
    fn from(e: BufferError) -> Self { TiffError::Buffer(e) }
}

fn decode() -> Result<(), At<TiffError>> {
    pixel_call().map_err_at(TiffError::from)?;  // trace preserved
    Ok(())
}
```

### Never format-then-rewrap

Formatting the error into a string and wrapping it in a new `At<>` destroys the original trace:

```rust
// WRONG — inner trace is gone, you only get the adapter's location
.map_err(|e| Error::Other(format!("decode failed: {}", e.into_inner())).at())?;

// RIGHT — convert the error type, keep the trace
.map_err_at(|e| Error::Other(e.to_string()))?;
```

### Avoid thiserror `#[from]` on variants that receive traced errors

thiserror's `#[from]` generates `From<X> for MyError`, but when the caller has `At<X>`, the `?` operator strips the `At<>` wrapper before the `From` conversion runs. Use explicit `map_err_at` instead.

### Always trace at error origination

Every `Err(MyError::Variant)` should be `Err(at(MyError::Variant))` or `Err(at!(MyError::Variant))`. If you skip this, there's no trace to propagate.

## `no_std` Support

whereat works with `core` + `alloc` (no `std`). Many crates use it this way:

```toml
[dependencies]
whereat = { version = "0.1", default-features = false }

[features]
std = ["whereat/std"]
```

The `std` feature currently only affects `Error` trait impls. All core functionality works without it.

## Best Practices

**Define a Result alias** in every crate that uses whereat:
```rust
pub type Result<T> = core::result::Result<T, At<MyError>>;
```

**Use `map_err_at` at every error type boundary.** This is the trace-preserving equivalent of `map_err`. If you're calling `.map_err()` on a `Result<T, At<E>>`, you probably want `.map_err_at()` instead.

**Use `at()` or `at!()` at every error origination.** An error without a trace is invisible.

**Keep hot loops zero-alloc.** You don't need `At<>` inside hot loops. Defer tracing until you exit. `.at_skipped_frames()` adds a `[...]` marker to indicate frames were skipped.

**Use `at_crate!()` at crate boundaries** when consuming errors from dependencies — ensures backtraces show your crate's GitHub links instead of confusing paths.

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
- Define type aliases like `type Result<T> = core::result::Result<T, At<MyError>>`
- Access your error via `.error()` or deref
- Support nesting with `core::error::Error::source()`

## Features

- **Small sizeof**: `At<E>` is only `sizeof(E) + 8` bytes (one pointer for boxed trace)
- **Zero allocation on Ok path**: No heap allocation until an error occurs
- **Ergonomic API**: `.at()` on Results, `.start_at()` on errors, `.map_err_at()` for trace-preserving conversions
- **Context options**: `.at_str()`, `.at_string()`, `.at_fn()`, `.at_named()`, `.at_data()`, `.at_debug()`, `.at_aside_error()`
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
result.at_aside_error(io_err)?              // Attach a related error (diagnostic only,
                                            // NOT in the .source() chain)
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
