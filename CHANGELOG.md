# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-01-16

Initial release.

### Added

- `At<E>` wrapper type for error location tracking
- `AtTrace` for embedded trace storage
- `AtTraceable` trait for custom error types with embedded traces
- Extension traits for ergonomic Result handling:
  - `ResultAtExt` - `.at()`, `.at_str("msg")`, `.at_fn(|| {})`, `.at_named("label")`, `.map_err_at(|e| ...)`, etc.
  - `ResultStartAtExt` - `.start_at()`, `.start_at_late()`
  - `ResultAtTraceableExt` - same methods for `AtTraceable` errors
  - `ErrorAtExt` - `.start_at()` on error values
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
  - `tinyvec-64-bytes` (3 inline slots)
  - `tinyvec-128-bytes` (11 inline slots)
  - `tinyvec-256-bytes` (27 inline slots)
  - `tinyvec-512-bytes` (60 inline slots)
- Smallvec feature flags for comparison:
  - `smallvec-128-bytes`
  - `smallvec-256-bytes`
- `no_std` + `alloc` support
- Fallible allocations where stable APIs allow

### Notes

- `Box::try_new` not yet stable - Box allocations can panic on OOM
