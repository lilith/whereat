+++
title = "Basic Usage"
description = "Your first traced error with whereat"
weight = 2
+++

# Basic Usage

## The Simple Version

Replace `?` with `.at()?` anywhere you propagate errors:

```rust
use whereat::{at, At, ResultAtExt};

#[derive(Debug)]
enum AppError {
    NotFound,
    InvalidInput(String),
}

fn find_user(id: u64) -> Result<String, At<AppError>> {
    if id == 0 {
        return Err(at(AppError::InvalidInput("id cannot be zero".into())));
    }
    Err(at(AppError::NotFound))
}

fn handle_request(id: u64) -> Result<String, At<AppError>> {
    let user = find_user(id)
        .at_str("looking up user")?;   // context on the error
    Ok(user)
}
```

That's it. When `find_user` returns an error, `.at_str("looking up user")?` adds a context message and a new location frame, then propagates.

## What You Get

```
Error: NotFound

    at src/main.rs:10:5
       ╰─ looking up user
    at src/main.rs:5:9
```

Each `.at()` or `.at_str()` call records the file, line, and column of the caller. The trace builds up as errors propagate through your call stack.

## With GitHub Links

For clickable source links in your traces:

```rust
// In lib.rs or main.rs
whereat::define_at_crate_info!();

use whereat::{at, At, ResultAtExt};

fn find_user(id: u64) -> Result<String, At<AppError>> {
    Err(at!(AppError::NotFound))  // at!() instead of at()
}
```

The `at!()` macro captures crate metadata so traces include links:

```
Error: NotFound
    at src/main.rs:5:9
       https://github.com/you/repo/blob/abc123/src/main.rs#L5
```

## Displaying Errors

`At<E>` provides several formatters:

```rust
let err: At<AppError> = at(AppError::NotFound).at_str("context");

// Debug — full trace with all contexts
println!("{:?}", err);

// Display — just the error message (delegates to E's Display/Debug)
println!("{}", err);

// Full trace via Display (message + locations + contexts)
println!("{}", err.full_trace());

// Just error + locations (no contexts)
println!("{}", err.last_error_trace());

// With repository links from AtCrateInfo
println!("{}", err.display_with_meta());
```

## Accessing the Inner Error

`At<E>` derefs to `E`, so you can match on it directly:

```rust
match *err {
    AppError::NotFound => println!("not found"),
    AppError::InvalidInput(ref msg) => println!("bad input: {}", msg),
}

// Or explicitly
let inner: &AppError = err.error();
```
