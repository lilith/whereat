# Ergonomics Improvements for errat

This document captures ideas for improving pattern matching and error handling ergonomics.

## Problem Statement

`At<E>` wraps an error with a trace, but accessing the inner error requires indirection:

```rust
// Current: must use .error()
match result {
    Err(ref e) => match e.error() {
        ErrorKind::NotFound => { /* handle */ }
        _ => return Err(e.clone()),
    }
    Ok(v) => v,
}

// Current: match guards are verbose
match result {
    Err(ref e) if matches!(e.error(), ErrorKind::NotFound) => { /* handle */ }
    Err(e) => return Err(e),
    Ok(v) => v,
}
```

## Proposed: Add Deref for At<E>

```rust
impl<E> core::ops::Deref for At<E> {
    type Target = E;

    #[inline]
    fn deref(&self) -> &E {
        &self.error
    }
}
```

### Benefits

```rust
// Clean match via &*err
match result {
    Err(ref e) => match &**e {
        ErrorKind::NotFound => { /* handle */ }
        _ => return Err(e.clone()),
    }
    Ok(v) => v,
}

// Field access works naturally
let err: At<MyError> = ...;
println!("{}", err.message);  // Deref to MyError, access .message

// Pattern matching in if-let
if let ErrorKind::NotFound = &*err {
    // handle
}
```

### Concerns to Explore

1. **Method resolution ambiguity**: If `E` has methods that conflict with `At<E>` methods,
   Deref could cause surprises. Current `At<E>` methods:
   - `error()`, `error_mut()`, `into_inner()`
   - `trace_len()`, `trace_iter()`, `frames()`
   - `at()`, `at_str()`, `at_string()`, `at_data()`, `at_debug()`
   - `map_error()`, `into_traceable()`
   - `take_trace()`, `set_trace()`

   If `E` has any of these methods, calling `err.method()` would call `At<E>::method()`,
   not `E::method()`. User would need `(*err).method()` to reach inner method.

2. **DerefMut**: Should we also add DerefMut? Allows `err.field = value` but also
   allows mutation that could break invariants. Probably safe since E is just data.

3. **Interaction with AsRef/Borrow**: Should we also impl `AsRef<E>` and `Borrow<E>`?

## Proposed: Add matches() helper

```rust
impl<E> At<E> {
    /// Check if inner error matches a predicate
    #[inline]
    pub fn matches<F>(&self, f: F) -> bool
    where
        F: FnOnce(&E) -> bool
    {
        f(&self.error)
    }
}
```

### Usage

```rust
match result {
    Err(ref e) if e.matches(|k| matches!(k, ErrorKind::NotFound)) => break,
    Err(e) => return Err(e),
    Ok(v) => v,
}
```

Not much better than `matches!(e.error(), ...)` - probably skip this.

## Proposed: Add map_err_at() extension

```rust
pub trait ResultAtMapExt<T, E> {
    /// Map error type while preserving trace
    fn map_err_at<E2, F>(self, f: F) -> Result<T, At<E2>>
    where
        F: FnOnce(E) -> E2;
}

impl<T, E> ResultAtMapExt<T, E> for Result<T, At<E>> {
    fn map_err_at<E2, F>(self, f: F) -> Result<T, At<E2>>
    where
        F: FnOnce(E) -> E2,
    {
        self.map_err(|e| e.map_error(f))
    }
}
```

### Usage

```rust
// Clean conversion that preserves trace
internal_fn().map_err_at(|kind| convert_to_public(kind))?;

// Instead of verbose:
internal_fn().map_err(|e| e.map_error(|kind| convert_to_public(kind)))?;
```

This is a clear win - add it.

## Style Guide: At<E> vs Custom Struct

### Option A: Use At<ErrorKind> directly (RECOMMENDED)

```rust
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    InvalidDimensions { width: u32, height: u32, reason: &'static str },
    IoError { reason: String },
}

pub type Error = errat::At<ErrorKind>;
pub type Result<T> = core::result::Result<T, Error>;
```

**Pros:**
- `map_error()` automatically preserves trace
- No boilerplate constructor functions
- Single canonical way to do things

**Cons:**
- Must use `.error()` or `&*err` to access kind
- Can't add custom methods to Error (only to ErrorKind)

### Option B: Custom struct implementing AtTraceable

```rust
pub struct Error {
    kind: ErrorKind,
    trace: AtTraceBoxed,
}

impl AtTraceable for Error {
    fn trace_mut(&mut self) -> &mut AtTrace { ... }
    fn trace(&self) -> Option<&AtTrace> { ... }
    fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { ... }
}
```

**Pros:**
- Can add custom methods like `is_not_found()`
- Can impl Deref to ErrorKind yourself

**Cons:**
- Must remember to use `into_at()` or manual trace transfer for conversions
- More boilerplate

### Interoperability

Both approaches should work together:

```rust
// At<A> -> At<B>: use map_error()
let b: At<KindB> = a.map_error(|kind_a| convert(kind_a));

// At<A> -> CustomError: use into_traceable()
let custom: CustomError = at_err.into_traceable(|kind| CustomError::from(kind));

// CustomError -> At<B>: use into_at()
let at_b: At<KindB> = custom.into_at(|e| convert(e.kind));
```

## Control Flow vs Errors

Don't model expected conditions as errors:

```rust
// BAD: EndOfScanData is expected during progressive decode
enum ErrorKind {
    EndOfScanData,  // Not really an error!
    ActualError { ... },
}

// GOOD: Separate type for expected outcomes
enum DecodeStatus {
    Continue,
    EndOfScan,
}

fn decode_block() -> Result<DecodeStatus, Error> {
    if found_marker {
        return Ok(DecodeStatus::EndOfScan);
    }
    Ok(DecodeStatus::Continue)
}

// Clean usage:
match decode_block()? {
    DecodeStatus::EndOfScan => break,
    DecodeStatus::Continue => { /* process */ }
}
```

## Testing Patterns

```rust
// GOOD: Unwrap then match on .error()
let err = result.unwrap_err();
assert!(matches!(err.error(), ErrorKind::NotFound { .. }));

// GOOD: With Deref
let err = result.unwrap_err();
assert!(matches!(&*err, ErrorKind::NotFound { .. }));

// GOOD: Check specific fields
match err.error() {
    ErrorKind::InvalidDimensions { width, .. } => assert_eq!(*width, 0),
    other => panic!("expected InvalidDimensions, got {:?}", other),
}

// AVOID: Nested maps in assertion
assert!(matches!(
    result.as_ref().map_err(|e| e.error()),
    Err(ErrorKind::NotFound { .. })
));
```

## Implementation Priority

1. **Add `Deref<Target=E>` for `At<E>`** - biggest ergonomic win
2. **Add `map_err_at()` extension** - cleaner conversions
3. **Add `AsRef<E>` and `Borrow<E>`** - consistency
4. **Consider `DerefMut`** - if use cases emerge

## Open Questions

1. Should there be a `#[derive(AtError)]` macro that generates both ErrorKind enum
   and convenience constructors?

2. Should `At<E>` implement `PartialEq` based only on `E`, ignoring trace?
   (Currently does not impl PartialEq)

3. Should there be a way to "freeze" a trace to prevent further additions?
   Use case: crossing API boundaries where internal locations shouldn't leak.
