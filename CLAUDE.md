# errat - Project Instructions

Lightweight error location tracking crate. See [FEEDBACK.md](FEEDBACK.md) for user feedback log.

## Quick Commands

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Design Notes

- `Traced<E>` is `sizeof(E) + 8` bytes (error inline, trace boxed)
- All allocations are fallible where stable APIs allow (Vec/String use try_reserve)
- `Box::try_new` not yet stable - using `Box::new` which can panic on OOM
- Error `E` always propagates since stored inline
- No `wide`/`multiversed` needed - this is not SIMD code

## Known Bugs

(none currently)

## Investigation Notes

(none currently)
