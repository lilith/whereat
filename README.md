# errat

[![CI](https://github.com/imazen/errat/actions/workflows/ci.yml/badge.svg)](https://github.com/imazen/errat/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/errat.svg)](https://crates.io/crates/errat)
[![Documentation](https://docs.rs/errat/badge.svg)](https://docs.rs/errat)
[![codecov](https://codecov.io/gh/imazen/errat/branch/main/graph/badge.svg)](https://codecov.io/gh/imazen/errat)
[![License](https://img.shields.io/crates/l/errat.svg)](LICENSE)

# Why `errat`? 

*After a decade of distributing server binaries, I'm finally extracting this approach into its own crate!*

In production, you need to immediately know 'err the bug is `at()` -- without panic!, debuginfo, or overhead. Just replace `?` with `.at()?` in your call tree to get beautiful build-time & async-friendly stacktraces. 
Compatible with plain enums, errors, structs, thiserror, anyhow, or any type with #[derive(Debug)]. No changes to your error type are required! 

Just `use errat::*` and `Err(at!(MyEnum::Problem))` to get an `Err(At<MyEnum>)`. 

At<YourErr> is maximally minimal, and only adds 8 bytes to the stack plus a boxed (under 64 byte) struct for traces. It's `no_std+alloc`, and offers `tinyvec` features to cut total allocs down from 2 to 1. 

Add arbitrary debug info at any time with `at_debug(|| impls_debug)`, `at_data(|| impls_display)`, `at_string(|| String)`, or `at_str("user_cursor_is_here")` on `Result` or `At<_>`, for one additional allocation each. 

`errat` can (optionally, and with no runtime cost) have backtraces link to the exact commit and line number on github/etc! 

**DO: Keep your hot loops zero-alloc:**
* You do NOT need to use/add At<> inside hot loops or until you want to incur that allocation. `start_at_late!()` will start the stacktrace with `[...]` to avoid confusion. Skipping frames? `.at_skipped()` will do the same.

**DO: Halve allocations with tinyvec**

We suggest the features `tinyvec-128-bytes` or `tinyvec-256-bytes` to give you 11 or 27  stacktrace lines (8 bytes each on 64-bit systems) *without* a 2nd allocation.

**DO: Call `.at_crate(crate_info!())` when consuming a result or error from anther crate**
This will ensure backtrace lines specify the crate name (no more confusing `src/lib.rs:305` lines!). at! and at_crate! are syntactic sugar for `start_at().at_crate(crate_info!())`.

**DO: Feel free to add an ergomonic alias**, like `type MyError = At<MyInternalError>`


NOTE: `at!(err)` is syntax sugar for `start_at(err).at_crate(crate_info!())`. and `at_crate!(result)` is syntax sugar for `result.at_crate(crate_info!())`. Like `.at()`, `crate_info!()` is generated at build time, and is just a &'static reference.

## Design Philosophy

**You define your own error types.** errat doesn't impose any structure on your errors - use enums, structs, or whatever suits your domain. errat just wraps them in `At<E>` to add location+context+crate tracking.

This means you can:
- Use `thiserror` for ergonomic `Display`/`From` impls, or `anyhow`
- Use any enum or struct that implements `Debug` / `Display`
- Define type aliases like `type MyError = At<BaseError>` (all methods delegate automatically)
- Access your error via `.error()`
- Support - or not support - nesting errors with `core::error::Error::source()`

## Features

- **Small sizeof**: `At<E>` is only `sizeof(E) + 8` bytes (one pointer for boxed trace)
- **Zero allocation on Ok path**: No heap allocation until an error occurs
- **Ergonomic API**: `.start_at()` on errors, `.at()` on Results
- **Context options**: `.at_str("context")` for strings, `.at_debug(|| data)` for typed context
- **Cross-crate tracing**: `at!()` macro captures crate info for GitHub links
- **no_std compatible**: Works with just `core` + `alloc`, `std` is optional
- **Mostly fallible allocations**: Vec/String ops use `try_reserve` and silently fail on OOM

## Quick Start

[..]

## Tinyvec Features

Enable inline storage for traces to avoid heap allocation for short traces:

- `tinyvec-64-bytes`: 3 inline slots before heap spill
- `tinyvec-128-bytes`: 11 inline slots before heap spill
- `tinyvec-256-bytes`: 27 inline slots before heap spill

```toml
[dependencies]
errat = { version = "0.1", features = ["tinyvec-128-bytes"] }
```

## TODO

- [ ] **Use `Box::try_new` when stabilized**: Currently Box allocations can panic on OOM. Track [rust-lang/rust#32838](https://github.com/rust-lang/rust/issues/32838) for `allocator_api` stabilization.

## License

MIT OR Apache-2.0
