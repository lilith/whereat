# errat TODO

## v0.1 Implementation

### API Methods

- [x] `at(err)` - function to wrap any type in `At<E>` (no Error trait required)
- [x] `at!(err)` - macro with crate info for GitHub links
- [x] `.start_at()` - method on `E: Error` types only (via ErrorAtExt trait)
- [x] `.at()` - add location to existing `At<E>` via ResultAtExt
- [x] `.at_crate(crate_info!())` - mark crate boundary
- [x] `source()` delegation - `At<E>` delegates to `E::source()` for error chains

### Context Methods

- [x] `at_str(&'static str)` - static strings (zero-cost)
- [x] `at_string(|| String)` - lazy computed strings (avoids alloc on Ok path)
- [x] `at_data(|| impl Display)` - typed context with Display, preserves type for downcast
- [x] `at_debug(|| impl Debug)` - typed context with Debug formatting

### Skip Markers

- [x] `at_skipped()` - adds `[...]` marker for skipped frames
- [x] `start_at_late!(err)` - starts trace with `[...]` marker for delayed tracing
- [x] `at_crate!(result)` - macro sugar for `result.at_crate(crate_info!())`

### Cleanup

- [x] Remove `ErrorMeta` trait (replaced by `CrateInfo` in trace)
- [x] Remove `errat-derive` crate (no longer needed)
- [x] Remove unused `std` feature
- [x] Require `E: Error` for `ErrorAtExt` blanket impl
- [x] Fix all doctests for new API
- [x] Update examples for new method names
- [x] Rename `ResultExt` → `ResultAtExt`, `Traceable` → `ErrorAtExt`

## Future Considerations

### Formatting (add if requested)

- [ ] ANSI color output (via feature flag)
- [ ] JSON output for structured logging (`to_json()`)
- [ ] Configurable trace order (newest-first vs oldest-first)
- [ ] Pretty-print with `{:#?}`

### Performance

- [ ] `Box::try_new` when stabilized (track rust-lang/rust#32838)
- [ ] Consider `first-location-free` feature to inline first location in `At<E>`

### API Extensions

- [ ] `at_source(|| impl Error)` - for wrapping source errors with context
- [ ] Integration with `tracing` crate spans
- [ ] `#[track_caller]` on more methods for better location capture

## Design Decisions

### Why `at_str` + `at_string` instead of `at_message`?

```rust
// Old: at_message with Cow - format! allocates on Ok path!
result.at_message(format!("x={}", x))?;  // BAD: allocates even on Ok

// New: at_string with closure - lazy, no alloc on Ok path
result.at_string(|| format!("x={}", x))?;  // GOOD: only allocates on Err

// New: at_str for static strings - zero overhead
result.at_str("loading config")?;  // GOOD: just a pointer
```

### Why `at_data` vs `at_string`?

- `at_string` stores as `String` - just text, no type info
- `at_data` stores as `Box<dyn DisplayAny>` - preserves type for `downcast_ref::<T>()`

### Why require `Error` for `.start_at()` method?

Blanket `impl<E: Error> ErrorAtExt for E` limits `.start_at()` to actual error types.
Use `at(err)` function for non-Error types.
