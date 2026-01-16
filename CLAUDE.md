# errat - Project Instructions

Lightweight error location tracking crate. See [FEEDBACK.md](FEEDBACK.md) for user feedback log.

## Quick Commands

```bash
just check   # fmt + clippy + test
just fmt     # format only
just clippy  # clippy only
just test    # test only
just outdated
just bench   # run benchmarks
just bench-group "happy_path"  # run specific benchmark group
just example-patterns  # run patterns example (good/bad/ugly usage)
```

## CI

GitHub Actions workflow at `.github/workflows/ci.yml`:
- Tests on: ubuntu-latest, windows-latest, macos-latest, windows-11-arm, ubuntu-24.04-arm
- Clippy, fmt check, code coverage (codecov)

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
