# whereat

[![CI](https://github.com/lilith/whereat/actions/workflows/ci.yml/badge.svg)](https://github.com/lilith/whereat/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/whereat.svg)](https://crates.io/crates/whereat)
[![Documentation](https://docs.rs/whereat/badge.svg)](https://docs.rs/whereat)
[![codecov](https://codecov.io/gh/lilith/whereat/branch/main/graph/badge.svg)](https://codecov.io/gh/lilith/whereat)
[![License](https://img.shields.io/crates/l/whereat.svg)](LICENSE)

**Production error tracing without debuginfo, panic, or overhead.**

Replace `?` with `.at()?` and get async-friendly stacktraces with GitHub links—in 40 nanoseconds.

```
Error: UserNotFound
   at src/db.rs:142:9
      ╰─ user_id = 42
   at src/api.rs:89:5
      ╰─ in handle_request
   at myapp @ https://github.com/you/myapp/blob/a1b2c3d/src/main.rs#L23
```

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

*anyhow/panic only capture backtraces when `RUST_BACKTRACE=1`. whereat always captures location.*

*Linux x86_64. See `cargo bench --bench nested_loops` for full results.*

## Quick Start

```rust
whereat::define_at_crate_info!();  // Once in lib.rs/main.rs

use whereat::{At, ResultAtExt, at};

#[derive(Debug)]
enum MyError { NotFound, InvalidInput }

fn find_user(id: u64) -> Result<User, At<MyError>> {
    if id == 0 {
        return Err(at!(MyError::InvalidInput));
    }
    Err(at!(MyError::NotFound))
}

fn handle(id: u64) -> Result<User, At<MyError>> {
    find_user(id).at_str("looking up user")?  // Add context
}
```

## Why whereat?

- **40ns per error** — 63x faster than anyhow with `RUST_BACKTRACE=1`
- **Zero cost on Ok** — No allocation until an error occurs
- **Tiny footprint** — `At<E>` is `sizeof(E) + 8` bytes
- **Async-friendly** — Uses `#[track_caller]`, works across `.await`
- **GitHub links** — Traces link to exact commit and line number
- **Works with anything** — thiserror, anyhow, plain enums, any `Debug` type
- **no_std + alloc** — Works everywhere

## Adding Context

```rust
result.at()?                           // Just add location
result.at_str("loading config")?       // Static message
result.at_string(|| format!("id={}", id))?  // Dynamic message
result.at_debug(|| request_info)?      // Attach debug data
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

This ensures traces show `myapp @ src/lib.rs:42` instead of confusing paths.

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

## Design

**You define your error types.** whereat just wraps them in `At<E>` to add tracing.

- Works with `thiserror`, `anyhow`, plain enums, or any `Debug` type
- `At<E>` dereferences to `E` — use `.error()` for explicit access
- `PartialEq`/`Hash` compare only the error, not the trace
- Type alias friendly: `type MyError = At<InternalError>`

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
fn process_batch(items: &[Item]) -> Result<(), At<MyError>> {
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

## License

MIT OR Apache-2.0
