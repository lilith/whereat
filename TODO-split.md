# lib.rs Module Split TODO

Split the large `src/lib.rs` (~2400 lines) into smaller modules for maintainability.

## Target Structure

```
src/
├── lib.rs          # Re-exports, crate docs, LocationVec helpers, macros, tests
├── context.rs      # AtContext, AtDebugAny, AtDisplayAny
├── crate_info.rs   # AtCrateInfo, AtCrateInfoBuilder
├── trace.rs        # AtTrace, AtTraceable trait
├── at.rs           # At<E> struct and its methods
└── ext.rs          # Extension traits (ResultAtExt, ErrorAtExt, etc.)
```

## Completed

- [x] `context.rs` - Created with AtContext, AtDebugAny, AtDisplayAny

## Remaining

### 1. Create `crate_info.rs`
- Move `AtCrateInfo` struct and impl
- Move `AtCrateInfoBuilder` struct and impl
- Move `const_str_eq()` helper
- **Keep macros in lib.rs** (they use `$crate` which must be in crate root)

### 2. Create `trace.rs`
- Move `AtTrace` struct and impl
- Move `AtTraceable` trait
- Import helpers from lib.rs: `LocationVec`, `try_push_location`, `try_location_vec_with_capacity`, `unwrap_location`, `try_box`

### 3. Create `at.rs`
- Move `At<E>` struct definition
- Move all `impl<E> At<E>` blocks
- Move `DisplayWithMeta` struct and impl
- Move `write_location_meta()` helper
- Move `impl fmt::Debug/Display/Error for At<E>`

### 4. Create `ext.rs`
- Move `ErrorAtExt` trait and impl
- Move `ResultAtExt` trait and impl
- Move `ResultStartAtExt` trait and impl
- Move `ResultAtTraceableExt` trait and impl
- Move `impl From<E> for At<E>`

### 5. Update `lib.rs`
- Add module declarations: `mod context; mod crate_info; mod trace; mod at; mod ext;`
- Keep: crate docs, `LocationVec` type aliases and helpers, `try_box()`, macros, tests
- Re-export everything: `pub use context::*; pub use crate_info::*;` etc.
- Keep `__ERRAT_CRATE_INFO` static and `at_crate_info()` getter
- Keep all macros (`define_at_crate_info!`, `at!`, `at_crate!`, `__errat_detect_commit!`)
- Keep `pub fn at<E>(err: E) -> At<E>` function

## Dependencies Between Modules

```
context.rs → crate_info.rs (AtContext::Crate holds &AtCrateInfo)
trace.rs → context.rs, crate_info.rs, lib.rs helpers
at.rs → trace.rs, context.rs, crate_info.rs, lib.rs helpers
ext.rs → at.rs, trace.rs, crate_info.rs
```

## Notes

- Macros must stay in lib.rs because they use `$crate` path resolution
- `LocationVec` helpers should stay in lib.rs (they're cfg-gated for tinyvec)
- `try_box()` can stay in lib.rs or move to a helpers module
- All pub items get re-exported from lib.rs for `use errat::*` to work
- Run `cargo test` after each module to verify no breakage
