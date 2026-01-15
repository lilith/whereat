# errat

Lightweight error location tracking with small sizeof and no_std support.

## Features

- **Small sizeof**: `Traced<E>` is only `sizeof(E) + 8` bytes (one pointer for boxed trace)
- **Zero allocation on Ok path**: No heap allocation until `.traced()` is called on an error
- **Ergonomic API**: `.traced()` on errors, `.at()` on `Result`s
- **Optional context**: Add context with `.at_msg("context")` or typed context with `.at_context(data)`
- **no_std compatible**: Works with just `core` + `alloc`, `std` is optional
- **Mostly fallible allocations**: Vec/String ops use `try_reserve` and silently fail on OOM; Box ops can still panic (waiting for `Box::try_new` stabilization)

## Usage

```rust
use errat::{Traced, Traceable, ResultExt};

#[derive(Debug)]
enum MyError {
    NotFound,
    InvalidInput(String),
}

fn inner() -> Result<(), Traced<MyError>> {
    Err(MyError::NotFound.traced())
}

fn outer() -> Result<(), Traced<MyError>> {
    inner().at_msg("while fetching user")?;
    Ok(())
}

let err = outer().unwrap_err();
println!("{:?}", err);
// Output includes error type and trace locations
```

## TODO

- [ ] **Use `Box::try_new` when stabilized**: Currently Box allocations can panic on OOM. Track [rust-lang/rust#32838](https://github.com/rust-lang/rust/issues/32838) for `allocator_api` stabilization.

- [ ] **thiserror-style derive macro**: Investigate adding a derive macro similar to `thiserror` that allows defining error variants with format strings:
  ```rust
  #[derive(Debug, Error)]
  enum MyError {
      #[error("file not found: {path}")]
      NotFound { path: String },
      #[error("invalid input: {0}")]
      InvalidInput(String),
  }
  ```
  This would auto-implement `Display` and potentially integrate with the tracing system.

- [ ] **Automatic context from format strings**: Consider whether format string context could be captured at trace points:
  ```rust
  result.at_fmt!("loading config from {}", path)?;
  ```

- [ ] **Integration with `#[track_caller]` spans**: Explore capturing more semantic information about the call site.

## License

MIT OR Apache-2.0
