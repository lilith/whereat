<!-- GENERATED FROM README.md by zenutils gen-readme-crates.sh — DO NOT EDIT. -->

# whereat

Know where the bug is `at()` — **without panic!, debuginfo, or overhead.** Replace `?` with `.at()?` to get build-time, async-friendly stacktraces with clickable GitHub links.

```
Error: UserNotFound
   at src/db.rs:142:9
      ╰─ user_id = 42
   at src/api.rs:89:5
      ╰─ in handle_request
   at myapp @ https://github.com/you/myapp/blob/a1b2c3d/src/main.rs#L23
```

Compatible with `no_std`, plain enums, structs, thiserror, anyhow — any type with `Debug`. No changes to your error types required.

```toml
[dependencies]
whereat = "0.1.5"
```

## Quick Start

`at!()` creates a traced error. `.at()?` propagates it. That's it.

```rust
// once in lib.rs — enables clickable GitHub links in traces
// For workspace crates: whereat::define_at_crate_info!(path = "crates/mylib/");
whereat::define_at_crate_info!();

use whereat::prelude::*;

#[derive(Debug)]
enum DbError { NotFound, ConnectionFailed }

fn get_user(id: u64) -> Result<String, At<DbError>> {
    if id == 0 { return Err(at!(DbError::NotFound)); } // at!() starts the trace
    Ok("alice".into())
}

fn get_email(id: u64) -> Result<String, At<DbError>> {
    let name = get_user(id).at()?; // .at()? adds this call site to the trace
    Ok(format!("{}@example.com", name))
}
```

### Multiple error types

```rust
#[derive(Debug)]
enum ApiError { Db(DbError), BadRequest(String) }

fn handle_request(id: u64) -> Result<String, At<ApiError>> {
    let email = get_email(id)
        .at()                           // new frame at this call site
        .at_str("looking up recipient") // context on that frame
        .map_err_at(ApiError::Db)?;     // DbError → ApiError, trace preserved
    Ok(email)
}

fn api_endpoint(id: u64) -> Result<String, At<ApiError>> {
    let result = handle_request(id).at()?; // propagate with location tracking
    Ok(result)
}
```

See [Avoiding Trace Loss](#avoiding-trace-loss) for patterns that silently destroy traces. Full runnable version: [`examples/readme.rs`](https://github.com/lilith/whereat/blob/main/examples/readme.rs).

## How it works


`At<E>` is `sizeof(E) + 8` bytes. The trace pointer is null until an error occurs — zero heap allocation on the Ok path. Each `.at()` is `#[track_caller]`, so the compiler bakes file:line:col into the binary as static data. No stack walking, no debug symbols.

### Frames vs contexts

`.at()` creates a **frame** (a new location). `.at_str()` adds **context** to the last frame. Multiple contexts can attach to one frame:

```
   at src/db.rs:15:13           ← .at() created this frame
   at src/db.rs:89:9            ← .at() created this frame
      ╰─ user lookup failed        ╰─ .at_str() added context
   at src/handler.rs:42:5       ← .at() created this frame
      ╰─ processing request        ╰─ .at_str() added context
      ╰─ request_id = 7           ╰─ .at_data() added context
```

**150x faster than `backtrace`**, zero overhead on the Ok path, ~18ns per frame on error. See [PERFORMANCE.md](https://github.com/lilith/whereat/blob/main/PERFORMANCE.md) for benchmarks.

## API Reference

### Starting a trace

| Function | Works on | Crate info | Use when |
|----------|----------|------------|----------|
| `at!(err)` | Any type | ✅ GitHub links | Default — requires `define_at_crate_info!()` |
| `at(err)` | Any type | ❌ None | Quick prototyping, no setup |
| `err.start_at()` | `Error` types | ❌ None | Chaining on error values |

### The prelude

`use whereat::prelude::*;` glob-imports the **types and traits** you need for `.at()?`-style
propagation:

| Re-exported by `prelude` | What it is |
|--------------------------|------------|
| `At` | the `At<E>` wrapper type |
| `at` | the `at(err)` **function** (no crate info) |
| `ResultAtExt` | the extension trait powering `.at()` / `.at_str()` / `.map_err_at()` on `Result<T, At<E>>` |
| `ErrorAtExt` | `.start_at()` on values implementing `Error` |

The **macros are NOT in the prelude** — `at!`, `at_crate!`, and `define_at_crate_info!` are exported at
the crate root (via `#[macro_export]`), and the glob does not pull them in. Call them qualified:
`whereat::at!(...)`, `whereat::at_crate!(...)`, `whereat::define_at_crate_info!()` (the README's
Quick Start does exactly this for `define_at_crate_info!`). You can alternatively `use whereat::at;`
to import the `at!` macro by name — the macro and the same-named `at` *function* live in different
namespaces, so importing one doesn't remove the other.

The free function `at(err)` (lowercase, no `!`) *is* glob-imported, so `use whereat::prelude::*;` alone
is enough for the no-setup `at(err)` path; you only need to reach for `whereat::` qualification when you
want the crate-info macros (`at!` / `at_crate!`). `ResultAtTraceableExt` (for the embedded-trace
`AtTraceable` approach) is also not in the prelude — import it from `whereat::` directly when needed.

### Extending a trace

On `Result<T, At<E>>`:

| Method | Effect |
|--------|--------|
| `.at()` | **New frame** at caller's location |
| `.at_str("msg")` | Static string context on **last frame** (zero-cost) |
| `.at_string(\|\| format!(...))` | Dynamic string context (lazy) |
| `.at_fn(\|\| {})` | New frame + captures function name |
| `.at_named("label")` | New frame + custom label |
| `.at_data(\|\| value)` | Typed context via Display (lazy) |
| `.at_debug(\|\| value)` | Typed context via Debug (lazy) |
| `.at_aside_error(err)` | Attach a related error (diagnostic only, **not** in `.source()` chain) |
| `.map_err_at(\|e\| ...)` | Convert error type, **preserve trace** |

`.at()` creates a NEW frame. `.at_str()` and other context methods add to the LAST frame.

### Inspecting and decomposing

| Method | Returns / Effect |
|--------|------------------|
| `.error()` | `&E` — borrow the inner error |
| `.decompose()` | `(E, Option<AtTrace>)` — consume, preserving the trace |
| `.map_error(\|e\| ...)` | `At<E2>` — convert error type, preserving trace |
| `.frames()` | `impl Iterator<Item = AtFrame<'_>>` — each frame's location + contexts, **oldest first** |
| `.frame_count()` | `usize` — number of location frames |
| `.full_trace()` | Display formatter showing all frames + contexts |

Each `AtFrame` exposes `.location() -> Option<&'static std::panic::Location>` (it's `None` only for a
skipped-frames marker). A `Location` gives you `.file() -> &str`, `.line() -> u32`, and
`.column() -> u32` as plain values — that's how you get `file:line` as **structured data** (not a
rendered string) for a JSON/structured log. See [Structured logging](#structured-logging-extracting-fileline).

`At<E>` itself has no `.location()` — locations live on frames, because a trace can hold several. Use
`.frames().next().and_then(|f| f.location())` for the origin (oldest) frame.

### `At<E>` implements `std::error::Error` (when `E` does)

`At<E>` implements [`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html) whenever
`E: std::error::Error` — the impl is `impl<E: core::error::Error> core::error::Error for At<E>`, and its
`.source()` delegates to the inner error's `.source()`. So an `At<E>` propagates with `?` straight into
`Box<dyn std::error::Error>`, `anyhow::Error`, or `eyre::Report` — with the trace intact, and without
`.into_inner()` (which would discard it):

```rust
use whereat::{at, At};

#[derive(Debug)]
struct DbError;
impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "db error") }
}
impl std::error::Error for DbError {}

fn query() -> Result<(), At<DbError>> { Err(at(DbError)) }

// At<DbError>: Error, so `?` coerces it into Box<dyn Error>.
fn run() -> Result<(), Box<dyn std::error::Error>> {
    query()?; // At<DbError> → Box<dyn Error>
    Ok(())
}
# run().ok();
```

The bound is `E: Error`, so the inner error must implement `Display + Debug + Error` for this to apply.
A plain enum that only derives `Debug` is not an `Error` — keep returning `At<YourError>` for those and
convert at the boundary with `map_err_at`.

### Structured logging: extracting `file:line`

For a server log you usually want the error's **origin** as structured fields, not a human-rendered
multi-line trace. `frames()` yields oldest-first, so `frames().next()` is the origination site (where
`at()` / `at!()` first captured the location), and `.error()` borrows the inner error:

```rust
use whereat::{at, At};

#[derive(Debug)]
struct DbError { code: u32 }

fn query() -> Result<(), At<DbError>> {
    Err(at(DbError { code: 42 })) // origin frame captured here
}

fn log_error(e: &At<DbError>) {
    // First frame = where the trace started (oldest-first ordering).
    let origin = e.frames().next().and_then(|f| f.location());
    let file = origin.map(|l| l.file()).unwrap_or("<unknown>");
    let line = origin.map(|l| l.line()).unwrap_or(0);
    let column = origin.map(|l| l.column()).unwrap_or(0);

    // file / line / column are plain &str / u32 — feed them to any structured
    // logger (tracing, slog, serde_json, ...) as real fields, e.g.:
    //
    //   tracing::error!(
    //       error = ?e.error(),   // &DbError, the inner error
    //       file, line, column,
    //       "request failed",
    //   );
    let _ = (file, line, column, e.error());
}

# let e = query().unwrap_err();
# log_error(&e);
```

To record **every** propagation site (not just the origin), iterate all frames — each `Location` is a
`(file, line, column)` triple:

```rust
# use whereat::{at, At, ResultAtExt};
# #[derive(Debug)]
# struct DbError;
# let e: At<DbError> = at(DbError).at();
let trace: Vec<(&str, u32, u32)> = e
    .frames()
    .filter_map(|f| f.location())               // skip skipped-frames markers
    .map(|l| (l.file(), l.line(), l.column()))
    .collect(); // oldest frame first
```

### Cross-crate tracing

When consuming errors from other crates, use `at_crate!()` to mark the boundary:

```rust
whereat::define_at_crate_info!();

fn call_external() -> Result<(), At<ExternalError>> {
    at_crate!(external_crate::do_thing())?;  // Wraps Result, marks boundary
    Ok(())
}
```

This ensures traces show `myapp @ src/lib.rs:42` instead of confusing paths from dependencies. Desugars to `result.at_crate(crate::at_crate_info())`.

### Hot loops

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

```rust
// WRONG — inner trace is gone, you only get the adapter's location
.map_err(|e| Error::Other(format!("decode failed: {}", e.into_inner())).at())?;

// RIGHT — convert the error type, keep the trace
.map_err_at(|e| Error::Other(e.to_string()))?;
```

### `#[from]` doesn't work with `At<>` — don't reach for `.into_inner()`

thiserror's `#[from]` generates `From<SubError> for MyError`, but `?` on `Result<T, At<SubError>>` needs `From<At<SubError>> for At<MyError>`, which doesn't exist. The compiler will reject it. The temptation is to "fix" this with `.into_inner()` — **don't**. Use `map_err_at` instead:

```rust
// WON'T COMPILE — no From<At<SubError>> for At<MyError>
sub_call()?;

// WRONG — compiles but trace dies
sub_call().map_err(|e| MyError::Sub(e.into_inner()))?;

// RIGHT — trace preserved
sub_call().map_err_at(|e| MyError::Sub(e))?;
```

### Always trace at error origination

Every `Err(MyError::Variant)` should be `Err(at(MyError::Variant))` or `Err(at!(MyError::Variant))`. If you skip this, there's no trace to propagate.

## Design

**You define your own error types.** whereat wraps them in `At<E>` to add location + context + crate tracking. Works with plain enums, structs, thiserror, anyhow — anything with `Debug`.

| Situation | Use |
|-----------|-----|
| Existing struct/enum you don't want to modify | Wrap with `At<YourError>` |
| Want traces embedded inside your error type | Implement [`AtTraceable`](https://github.com/lilith/whereat/blob/main/ADVANCED.md#embedded-traces-attraceable) |

`At<E>` is `no_std` (`core` + `alloc`). The `std` feature exists for historical compatibility but is a no-op — `core::error::Error` is stable since Rust 1.81.

See also: [Inline storage features](https://github.com/lilith/whereat/blob/main/ADVANCED.md#allocation-behavior) | [Workspace layouts](https://github.com/lilith/whereat/blob/main/ADVANCED.md#complex-workspace-layouts) | [Link format customization](https://github.com/lilith/whereat/blob/main/ADVANCED.md#link-formats) | [Pretty output](https://github.com/lilith/whereat/blob/main/ADVANCED.md#pretty-output-formatters)

## License

Dual-licensed: [Apache-2.0](https://github.com/lilith/whereat/blob/main/LICENSE-APACHE) or [MIT](https://github.com/lilith/whereat/blob/main/LICENSE-MIT), at your option.

## Image tech I maintain

| | |
|:--|:--|
| **Codecs** ¹ | [zenjpeg] · [zenpng] · [zenwebp] · [zengif] · [zenavif] · [zenjxl] · [zenbitmaps] · [heic] · [zentiff] · [zenpdf] · [zensvg] · [zenjp2] · [zenraw] · [ultrahdr] |
| Codec internals | [zenjxl-decoder] · [jxl-encoder] · [zenrav1e] · [rav1d-safe] · [zenavif-parse] · [zenavif-serialize] |
| Compression | [zenflate] · [zenzop] · [zenzstd] |
| Processing | [zenresize] · [zenquant] · [zenblend] · [zenfilters] · [zensally] · [zentone] |
| Pixels & color | [zenpixels] · [zenpixels-convert] · [linear-srgb] · [garb] |
| Pipeline & framework | [zenpipe] · [zencodec] · [zencodecs] · [zenlayout] · [zennode] · [zenwasm] · [zentract] |
| Metrics | [zensim] · [fast-ssim2] · [butteraugli] · [zenmetrics] · [resamplescope-rs] |
| Pickers & ML | [zenanalyze] · [zenpredict] · [zenpicker] |
| Products | [Imageflow] image engine ([.NET][imageflow-dotnet] · [Node][imageflow-node] · [Go][imageflow-go]) · [Imageflow Server] · [ImageResizer] (C#) |

<sub>¹ pure-Rust, `#![forbid(unsafe_code)]` codecs, as of 2026</sub>

### General Rust awesomeness

[zenbench] · [archmage] · [magetypes] · [enough] · **whereat** · [cargo-copter]

[Open source](https://www.imazen.io/open-source) · [@imazen](https://github.com/imazen) · [@lilith](https://github.com/lilith) · [lib.rs/~lilith](https://lib.rs/~lilith)

[zenjpeg]: https://github.com/imazen/zenjpeg
[zenpng]: https://github.com/imazen/zenpng
[zenwebp]: https://github.com/imazen/zenwebp
[zengif]: https://github.com/imazen/zengif
[zenavif]: https://github.com/imazen/zenavif
[zenjxl]: https://github.com/imazen/zenjxl
[zenbitmaps]: https://github.com/imazen/zenbitmaps
[heic]: https://github.com/imazen/heic
[zentiff]: https://github.com/imazen/zentiff
[zenpdf]: https://github.com/imazen/zenpdf
[zensvg]: https://github.com/imazen/zenextras
[zenjp2]: https://github.com/imazen/zenextras
[zenraw]: https://github.com/imazen/zenraw
[ultrahdr]: https://github.com/imazen/ultrahdr
[zenjxl-decoder]: https://github.com/imazen/zenjxl-decoder
[jxl-encoder]: https://github.com/imazen/jxl-encoder
[zenrav1e]: https://github.com/imazen/zenrav1e
[rav1d-safe]: https://github.com/imazen/rav1d-safe
[zenavif-parse]: https://github.com/imazen/zenavif-parse
[zenavif-serialize]: https://github.com/imazen/zenavif-serialize
[zenflate]: https://github.com/imazen/zenflate
[zenzop]: https://github.com/imazen/zenzop
[zenzstd]: https://github.com/imazen/zenzstd
[zenresize]: https://github.com/imazen/zenresize
[zenquant]: https://github.com/imazen/zenquant
[zenblend]: https://github.com/imazen/zenblend
[zenfilters]: https://github.com/imazen/zenfilters
[zensally]: https://github.com/imazen/zensally
[zentone]: https://github.com/imazen/zentone
[zenpixels]: https://github.com/imazen/zenpixels
[zenpixels-convert]: https://github.com/imazen/zenpixels
[linear-srgb]: https://github.com/imazen/linear-srgb
[garb]: https://github.com/imazen/garb
[zenpipe]: https://github.com/imazen/zenpipe
[zencodec]: https://github.com/imazen/zencodec
[zencodecs]: https://github.com/imazen/zencodecs
[zenlayout]: https://github.com/imazen/zenlayout
[zennode]: https://github.com/imazen/zennode
[zenwasm]: https://github.com/imazen/zenwasm
[zentract]: https://github.com/imazen/zentract
[zensim]: https://github.com/imazen/zensim
[fast-ssim2]: https://github.com/imazen/fast-ssim2
[butteraugli]: https://github.com/imazen/butteraugli
[zenmetrics]: https://github.com/imazen/zenmetrics
[resamplescope-rs]: https://github.com/imazen/resamplescope-rs
[zenanalyze]: https://github.com/imazen/zenanalyze
[zenpredict]: https://github.com/imazen/zenanalyze
[zenpicker]: https://github.com/imazen/zenanalyze
[zenbench]: https://github.com/imazen/zenbench
[archmage]: https://github.com/imazen/archmage
[magetypes]: https://github.com/imazen/archmage
[enough]: https://github.com/imazen/enough
[cargo-copter]: https://github.com/imazen/cargo-copter
[Imageflow]: https://github.com/imazen/imageflow
[Imageflow Server]: https://github.com/imazen/imageflow-dotnet-server
[ImageResizer]: https://github.com/imazen/resizer
[imageflow-dotnet]: https://github.com/imazen/imageflow-dotnet
[imageflow-node]: https://github.com/imazen/imageflow-node
[imageflow-go]: https://github.com/imazen/imageflow-go
