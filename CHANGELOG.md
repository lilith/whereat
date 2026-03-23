# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.4] - 2026-03-23

### Added

- `decompose()` method on `At<E>` — returns `(E, Option<AtTrace>)` so you can take apart an `At<E>` without silently losing the trace
- `at_aside_error()` method on `At<E>`, `ResultAtExt`, `ResultAtTraceableExt`, and `AtTraceable` — replacement for `at_error()` with a name that clarifies the attached error is diagnostic context, not part of the `.source()` chain
- `ErrorAtExt` added to `prelude` module

### Deprecated

- `into_inner()` on `At<E>` — use `decompose()` to preserve the trace, or `map_error()` / `map_err_at()` to convert error types while keeping the trace
- `at_error()` on `At<E>`, `ResultAtExt`, `ResultAtTraceableExt`, and `AtTraceable` — renamed to `at_aside_error()` to clarify that the attached error is NOT wired into `.source()` chain traversal

### Changed

- README rewritten with "Avoiding Trace Loss" section documenting anti-patterns that silently destroy traces, `no_std` guidance, `map_err_at` as a core pattern, and Result type alias convention

## [0.1.0] - 2026-01-16

Initial release.

### Added

- `At<E>` wrapper type for error location tracking
- `AtTrace` for embedded trace storage
- `AtTraceable` trait for custom error types with embedded traces
- Extension traits for ergonomic Result handling:
  - `ResultAtExt` - `.at()`, `.at_str("msg")`, `.at_fn(|| {})`, `.at_named("label")`, `.map_err_at(|e| ...)`, etc.
  - `ResultAtTraceableExt` - same methods for `AtTraceable` errors
  - `ErrorAtExt` - `.start_at()` on error values implementing `core::error::Error`
- Context attachment methods (attach to last frame, no new location):
  - `.at_str("msg")` - static string context
  - `.at_string(|| format!(...))` - dynamic string context
  - `.at_data(|| value)` - Display-formatted typed context
  - `.at_debug(|| value)` - Debug-formatted typed context
  - `.at_error(err)` - attach source errors
  - `.at_crate(info)` - crate boundary markers
- Location frame methods (add new frame):
  - `.at()` - add location frame
  - `.at_fn(|| {})` - add location + auto-detected function name
  - `.at_named("label")` - add location + explicit label
- `at!()` macro for crate-aware error creation with GitHub links
- `at_crate!()` macro for crate boundary marking
- `define_at_crate_info!()` macro for crate metadata setup
- `AtCrateInfo` and `AtCrateInfoBuilder` for runtime crate metadata
- `PartialEq`, `Eq`, `Hash` for `At<E>` (compares only inner error, not trace)
- `AsRef<E>` for `At<E>`
- Tinyvec feature flags for inline trace storage:
  - `_tinyvec-64-bytes` (4 inline slots)
  - `_tinyvec-128-bytes` (12 inline slots)
  - `_tinyvec-256-bytes` (28 inline slots)
  - `_tinyvec-512-bytes` (60 inline slots)
- Smallvec feature flags for comparison:
  - `_smallvec-128-bytes`
  - `_smallvec-256-bytes`
- `no_std` + `alloc` support
- Fallible allocations where stable APIs allow

### Notes

- `Box::try_new` not yet stable - Box allocations can panic on OOM
