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

## Best Practices

**DO: Keep your hot loops zero-alloc**
- You do NOT need At<> inside hot loops. Defer tracing until you exit.
- `.at_skipped_frames()` adds a `[...]` marker to indicate frames were skipped.

**DO: Halve allocations with tinyvec**
- Features `_tinyvec-128-bytes` or `_tinyvec-256-bytes` give you 12 or 28 inline stacktrace slots without a 2nd allocation.

**DO: Use `at_crate!()` at crate boundaries**
- When consuming errors from other crates, this ensures backtraces show `myapp @ src/lib.rs:42` instead of confusing paths.

**DO: Feel free to add ergonomic aliases**
- `type MyError = At<MyInternalError>` works perfectly.

## Design Philosophy

**You define your own error types.** whereat doesn't impose any structure on your errors — use enums, structs, or whatever suits your domain. whereat just wraps them in `At<E>` to add location+context+crate tracking.

This means you can:
- Use `thiserror` for ergonomic `Display`/`From` impls, or `anyhow`
- Use any enum or struct that implements `Debug`
- Define type aliases like `type MyError = At<BaseError>`
- Access your error via `.error()` or deref
- Support nesting with `core::error::Error::source()`

## Features

- **Small sizeof**: `At<E>` is only `sizeof(E) + 8` bytes (one pointer for boxed trace)
- **Zero allocation on Ok path**: No heap allocation until an error occurs
- **Ergonomic API**: `.at()` on Results, `.start_at()` on errors, `.map_err(at)` for conversions
- **Context options**: `.at_str("msg")`, `.at_fn(|| {})`, `.at_debug(|| data)`, `.at_string(|| format!(...))`
- **Cross-crate tracing**: `at!()` and `at_crate!()` macros capture crate info for GitHub links
- **Equality/Hashing**: `PartialEq`, `Eq`, `Hash` compare only the error, not the trace
- **no_std compatible**: Works with just `core` + `alloc`
- **Fallible allocations**: Vec/String ops use `try_reserve` and silently fail on OOM

## Adding Context

```rust
result.at()?                           // Just add location
result.at_str("loading config")?       // Static message
result.at_string(|| format!("id={}", id))?  // Dynamic message (lazy)
result.at_debug(|| request_info)?      // Attach debug data (lazy)
result.at_fn(|| {})?                   // Capture function name
```

## Cross-Crate Tracing

When consuming errors from other crates, use `at_crate!()` to mark the boundary:

```rust
whereat::define_at_crate_info!();

fn call_external() -> Result<(), At<ExternalError>> {
    external_crate::do_thing().map_err(|e| at_crate!(e))?;
    Ok(())
}
```

This ensures traces show `myapp @ src/lib.rs:42` instead of confusing paths from dependencies.

## Crate Metadata

### Compile-time

```rust
whereat::define_at_crate_info!(
    path = "crates/mylib/",  // For workspace crates
    meta = &[("team", "platform"), ("oncall", "team@example.com")],
);
```

### Runtime

```rust
use std::sync::OnceLock;
use whereat::AtCrateInfo;

static INFO: OnceLock<AtCrateInfo> = OnceLock::new();

pub(crate) fn at_crate_info() -> &'static AtCrateInfo {
    INFO.get_or_init(|| {
        AtCrateInfo::builder()
            .name(env!("CARGO_PKG_NAME"))
            .repo(option_env!("CARGO_PKG_REPOSITORY"))
            .meta_owned(vec![
                ("instance".into(), std::env::var("INSTANCE_ID").unwrap_or_default()),
            ])
            .build()
    })
}
```

## Embedded Traces

For full control, embed `AtTrace` directly in your error type:

```rust
use whereat::{AtTrace, AtTraceable, ResultAtTraceableExt};

struct MyError {
    kind: ErrorKind,
    trace: AtTrace,  // 40 bytes, or Box<AtTrace> for 8 bytes
}

impl AtTraceable for MyError {
    fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
    fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
    fn fmt_message(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}
```

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
    process_batch(&items).map_err(|e| at!(e))?;  // Wrap on exit
    Ok(())
}
```

Use `.at_skipped_frames()` to add a `[...]` marker when you know frames were skipped.

## License

MIT OR Apache-2.0
