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
| `.at()` | Add new frame at caller's location |
| `.at_str("msg")` | Add context to last frame |
| `.map_err_at(\|e\| ...)` | Convert error, preserve trace |

See [Adding Context](#adding-context) for the full method list.

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

For runtime-determined values (instance IDs, environment config), define your own `at_crate_info()` getter:

```rust
use std::sync::OnceLock;
use whereat::AtCrateInfo;

static CRATE_INFO: OnceLock<AtCrateInfo> = OnceLock::new();

pub(crate) fn at_crate_info() -> &'static AtCrateInfo {
    CRATE_INFO.get_or_init(|| {
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

The `_owned()` builder methods (`name_owned()`, `meta_owned()`, etc.) leak strings via `Box::leak` to get `'static` lifetime — appropriate for one-time initialization.

### Link Formats

By default, links use GitHub format. For other forges:

```rust
use whereat::{AtCrateInfo, GITLAB_LINK_FORMAT};

static INFO: AtCrateInfo = AtCrateInfo::builder()
    .name("mylib")
    .repo(Some("https://gitlab.com/org/repo"))
    .link_format(GITLAB_LINK_FORMAT)  // or GITEA_LINK_FORMAT, BITBUCKET_LINK_FORMAT
    .build();
```

Or use `.link_format_auto()` to auto-detect from the repo URL.

## Embedded Traces

For full control, embed `AtTrace` directly in your error type:

```rust
use whereat::{AtTrace, AtTraceable, ResultAtTraceableExt};

struct MyError {
    kind: ErrorKind,
    trace: AtTrace,
}

impl AtTraceable for MyError {
    fn trace_mut(&mut self) -> &mut AtTrace { &mut self.trace }
    fn trace(&self) -> Option<&AtTrace> { Some(&self.trace) }
    fn fmt_message(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)
    }
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

## License

MIT OR Apache-2.0
