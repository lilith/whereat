# ABLATION-whereat — Conservative Public-API Review

**Date:** 2026-06-10
**Snapshot commit:** 28ceab6e (main@origin)
**Snapshot file:** docs/public-api/whereat.txt (419 default / 419 all-features items; identical — no feature-gated additions)
**Grep template:** `grep -rn "<SYMBOL>" /home/lilith/work/ --include="*.rs" 2>/dev/null | grep -v "/whereat/" | grep -v "target/" | grep -v ".jj/" | grep -v "zen-arm-src/"`

## Summary

**0 items flagged.** The `whereat` crate is an error-location tracking library providing `At<E>`, `AtTrace`, and `AtTraceable` to attach source locations to errors. The surface is well-structured and actively consumed across the zen workspace.

Known consumers as of this scan:
- zengif (`.jplag/` and `cross/`): `whereat::At`, `whereat::at!`, `define_at_crate_info!`, `ResultAtExt`
- zenjpeg (`cross/`): `AtTrace`, `AtTraceBoxed`, `AtTraceable`, `ResultAtTraceableExt`, `define_at_crate_info!`
- zenpng (`pre-filter/`): `define_at_crate_info!`
- zenavif (`pre-filter/`): `define_at_crate_info!`

## Snapshot Structure Note

The 419-item count reflects the same type/trait set repeated at three path depths: top-level (`whereat::At`), `whereat::prelude::At`, and `whereat::prelude::at::At`. This is by design — the prelude module re-exports all primary types and the `at!` macro. No items are unique to the all-features build.

## Items Investigated

### `AtFrameOwned` and `AtTraceBoxed` — `#[doc(hidden)]`, correctly structured

Both are `pub struct` in `src/trace.rs` with `#[doc(hidden)]` on the struct definition:
- `AtFrameOwned` (line 598): advanced API for trace frame manipulation; used by consumers implementing `AtTraceable` (e.g., cross/zenjpeg embeds it indirectly via `at_push`/`at_pop`)
- `AtTraceBoxed` (line 772): Box-on-demand AtTrace helper; actively used by cross/zenjpeg's error types (`trace: AtTraceBoxed` field pattern)

These appear in the snapshot only in method signatures (not as `pub struct` top-level entries) because `#[doc(hidden)]` suppresses them from `cargo public-api --simplified`. The design is correct: accessible to power-user consumers implementing `AtTraceable`, hidden from the primary docs surface. **KEEP as-is.**

### `AT_MAX_CONTEXTS` and `AT_MAX_FRAMES` — public constants, KEEP

Used to reason about trace capacity. Correct to expose as public constants for consumers that need to reason about trace bounds.

### `AtCrateInfoBuilder` — builder for `AtCrateInfo`, KEEP

Used via `AtCrateInfo::builder()`. Necessary companion to `AtCrateInfo`. The `*_owned` variants (`name_owned`, `repo_owned`, etc.) are the `alloc`-owning variants for runtime-constructed crate info (vs `&'static str` compile-time variants). All load-bearing.

### `AtTrace` direct manipulation API — KEEP

`push`, `push_first`, `pop`, `pop_first`, `prepend`, `append` — needed by `AtTraceable` implementors for custom error types that embed `AtTrace` directly. Used by cross/zenjpeg.

### `ResultAtTraceableExt` trait — KEEP

Distinct from `ResultAtExt`: applies `at_*` chaining to `Result<T, E>` where `E: AtTraceable` (in-place, no wrapping in `At<E>`). Used by cross/zenjpeg.

## Flagged Items

None.

## Digest

- Snapshot: 419 (default) / 419 (all-features) items — feature-invariant
- Flagged A: 0
- Flagged B: 0
- 0% of surface flagged
- `AtFrameOwned` / `AtTraceBoxed` are already correctly `#[doc(hidden)]`; they appear in method signatures but not as primary doc items
- Active consumer use across zengif, zenjpeg, zenpng, zenavif
