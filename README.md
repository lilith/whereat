# whereat

[![CI](https://github.com/lilith/whereat/actions/workflows/ci.yml/badge.svg)](https://github.com/lilith/whereat/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/whereat.svg)](https://crates.io/crates/whereat)
[![Documentation](https://docs.rs/whereat/badge.svg)](https://docs.rs/whereat)
[![codecov](https://codecov.io/gh/lilith/whereat/branch/main/graph/badge.svg)](https://codecov.io/gh/lilith/whereat)
[![License](https://img.shields.io/crates/l/whereat.svg)](LICENSE)

# Why `whereat`? 

*After a decade of distributing server binaries, I'm finally extracting this approach into its own crate!*

In production, you need to immediately know 'err the bug is `at()` -- without panic!, debuginfo, or overhead. Just replace `?` with `.at()?` in your call tree to get beautiful build-time & async-friendly stacktraces. 
Compatible with plain enums, errors, structs, thiserror, anyhow, or any type with #[derive(Debug)]. No changes to your error type are required! 

Just `use whereat::*` and `Err(at!(MyEnum::Problem))` to get an `Err(At<MyEnum>)`. 

At<YourErr> is maximally minimal, and only adds 8 bytes to the stack plus a boxed 40-byte struct for traces. It's `no_std+alloc`, and offers `tinyvec` features to cut total allocs down from 2 to 1. 

Add arbitrary debug info at any time with `at_debug(|| impls_debug)`, `at_data(|| impls_display)`, `at_string(|| String)`, or `at_str("user_cursor_is_here")` on `Result` or `At<_>`, for one additional allocation each. 

`whereat` can (optionally, and with no runtime cost) have backtraces link to the exact commit and line number on github/etc! 

**DO: Keep your hot loops zero-alloc:**
* You do NOT need to use/add `At<>` inside hot loops. Defer wrapping until you exit the hot path and want to incur the allocation.

**DO: Halve allocations with tinyvec**

We suggest the features `tinyvec-128-bytes` or `tinyvec-256-bytes` to give you 12 or 28 inline stacktrace slots (8 bytes each on 64-bit systems) *without* a 2nd allocation.

**DO: Use `at_crate!()` when consuming a result or error from another crate**
This will ensure backtrace lines specify the crate name (no more confusing `src/lib.rs:305` lines!). Requires `whereat::define_at_crate_info!()` once in your crate root.

**DO: Feel free to add an ergomonic alias**, like `type MyError = At<MyInternalError>`


**Setup:** Add `whereat::define_at_crate_info!();` once in your lib.rs or main.rs. This defines a `at_crate_info()` getter that `at!()` and `at_crate!()` use to embed crate metadata (name, repo URL, commit) for GitHub-linked backtraces.

## Design Philosophy

**You define your own error types.** whereat doesn't impose any structure on your errors - use enums, structs, or whatever suits your domain. whereat just wraps them in `At<E>` to add location+context+crate tracking.

This means you can:
- Use `thiserror` for ergonomic `Display`/`From` impls, or `anyhow`
- Use any enum or struct that implements `Debug` / `Display`
- Define type aliases like `type MyError = At<BaseError>` (all methods delegate automatically)
- Access your error via `.error()`
- Support - or not support - nesting errors with `core::error::Error::source()`

## Features

- **Small sizeof**: `At<E>` is only `sizeof(E) + 8` bytes (one pointer for boxed trace)
- **Zero allocation on Ok path**: No heap allocation until an error occurs
- **Ergonomic API**: `.start_at()` on errors, `.at()` on Results, `.map_err_at()` for conversions
- **Context options**: `.at_str("msg")`, `.at_named("phase")`, `.at_fn(|| {})`, `.at_debug(|| data)`
- **Cross-crate tracing**: `at!()` macro captures crate info for GitHub links
- **Equality/Hashing**: `PartialEq`, `Eq`, `Hash` compare only the error, not the trace
- **no_std compatible**: Works with just `core` + `alloc`
- **Mostly fallible allocations**: Vec/String ops use `try_reserve` and silently fail on OOM

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
    find_user(id).at_str("looking up user")?;  // Adds context to existing frame
    Ok("done".into())
}
```

For workspace crates, set the path: `whereat::define_at_crate_info!(path = "crates/mylib/");`

## Crate Metadata Options

### Compile-time metadata

Add custom key-value pairs at compile time:

```rust
whereat::define_at_crate_info!(
    path = "crates/mylib/",
    meta = &[("team", "platform"), ("oncall", "platform@example.com")],
);
```

Access metadata via `at_crate_info().get_meta("team")`.

### Runtime metadata (escape hatch)

For runtime-determined values (instance IDs, environment config), define your own `at_crate_info()` getter instead of using the macro:

```rust
use std::sync::OnceLock;
use whereat::AtCrateInfo;

static CRATE_INFO: OnceLock<AtCrateInfo> = OnceLock::new();

// at!() and at_crate!() call this function
pub(crate) fn at_crate_info() -> &'static AtCrateInfo {
    CRATE_INFO.get_or_init(|| {
        AtCrateInfo::builder()
            .name(env!("CARGO_PKG_NAME"))
            .repo(option_env!("CARGO_PKG_REPOSITORY"))
            .module(module_path!())
            .meta_owned(vec![
                ("instance".into(), std::env::var("INSTANCE_ID").unwrap_or_default()),
                ("env".into(), std::env::var("ENV").unwrap_or("dev".into())),
            ])
            .build()
    })
}
```

The `_owned()` builder methods (`name_owned()`, `meta_owned()`, etc.) leak strings via `Box::leak` to get `'static` lifetime - appropriate for one-time initialization.

## Tinyvec Features

Enable inline storage for traces to avoid heap allocation for short traces:

- `tinyvec-64-bytes`: 4 inline slots before heap spill
- `tinyvec-128-bytes`: 12 inline slots before heap spill
- `tinyvec-256-bytes`: 28 inline slots before heap spill
- `tinyvec-512-bytes`: 60 inline slots before heap spill

```toml
[dependencies]
whereat = { version = "0.1", features = ["tinyvec-128-bytes"] }
```

## Embedded Traces (Advanced)

Instead of wrapping errors with `At<E>`, you can embed the trace directly in your error type using `AtTraceable`. This gives you full control over your error's layout.

```rust
use whereat::{AtTrace, AtTraceable, ResultAtTraceableExt};
use std::fmt;

struct MyError {
    kind: ErrorKind,
    trace: AtTrace,
}

enum ErrorKind { NotFound, InvalidInput }

impl AtTraceable for MyError {
    fn trace_mut(&mut self) -> &mut AtTrace {
        &mut self.trace
    }

    fn trace(&self) -> Option<&AtTrace> {
        Some(&self.trace)
    }

    fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ErrorKind::NotFound => write!(f, "not found"),
            ErrorKind::InvalidInput => write!(f, "invalid input"),
        }
    }
}

impl MyError {
    #[track_caller]
    fn not_found() -> Self {
        Self { kind: ErrorKind::NotFound, trace: AtTrace::capture() }
    }
}

fn find_user(id: u64) -> Result<String, MyError> {
    if id == 0 { return Err(MyError::not_found()); }
    Ok(format!("User {}", id))
}

fn process(id: u64) -> Result<String, MyError> {
    find_user(id).at_str("looking up user")?;  // Works on Result<T, impl AtTraceable>!
    Ok("done".into())
}
```

**Storage options:**

| Field Type | Size | When to use |
|------------|------|-------------|
| `AtTrace` | 40 bytes | Trace always captured at construction |
| `Box<AtTrace>` | 8 bytes | Smaller error, trace always allocated |
| `Option<Box<AtTrace>>` | 8 bytes | Lazy allocation on first `.at_*()` call |

For `Option<Box<AtTrace>>`, implement `trace_mut` with lazy init:

```rust
fn trace_mut(&mut self) -> &mut AtTrace {
    self.trace.get_or_insert_with(|| Box::new(AtTrace::new()))
}
```

## TODO

- [ ] **Use `Box::try_new` when stabilized**: Currently Box allocations can panic on OOM. Track [rust-lang/rust#32838](https://github.com/rust-lang/rust/issues/32838) for `allocator_api` stabilization.

## License

MIT OR Apache-2.0
