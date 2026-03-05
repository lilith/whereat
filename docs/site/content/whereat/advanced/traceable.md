+++
title = "Embedded Traces (AtTraceable)"
description = "Store traces inside your error types instead of wrapping with At<E>"
weight = 1
+++

# Embedded Traces (AtTraceable)

The default approach wraps errors: `Result<T, At<E>>`. But if you want the trace *inside* your error type — so callers see `Result<T, MyError>` — implement `AtTraceable`.

## When to Use Each Approach

{% mermaid() %}
graph TD
    Q["Do you control the error type?"]
    Q -->|No| W["Use At&lt;E&gt; wrapper"]
    Q -->|Yes| Q2["Want callers to see your type directly?"]
    Q2 -->|"Yes — Result&lt;T, MyError&gt;"| T["Implement AtTraceable"]
    Q2 -->|"No — At&lt;E&gt; is fine"| W
{% end %}

| Approach | Return Type | Trace Location | When |
|----------|-------------|----------------|------|
| `At<E>` wrapper | `Result<T, At<E>>` | Outside error | You don't own the error type, or don't want to modify it |
| `AtTraceable` | `Result<T, MyError>` | Inside error | You own the type and want a clean public API |

## Implementing AtTraceable

```rust
use whereat::{AtTrace, AtTraceable, ResultAtTraceableExt};
use core::fmt;

struct MyError {
    kind: ErrorKind,
    trace: AtTrace,
}

impl AtTraceable for MyError {
    fn trace_mut(&mut self) -> &mut AtTrace {
        &mut self.trace
    }

    fn trace(&self) -> Option<&AtTrace> {
        Some(&self.trace)
    }

    fn fmt_message(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}

impl MyError {
    #[track_caller]
    fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            trace: AtTrace::capture(),  // captures caller's location
        }
    }
}
```

Then use `ResultAtTraceableExt` instead of `ResultAtExt`:

```rust
fn caller() -> Result<(), MyError> {
    inner()
        .at_str("loading config")?;  // same API, works on Result<T, MyError>
    Ok(())
}
```

## Storage Options

Choose based on your error type's size constraints:

| Field Type | Extra Size | Behavior |
|------------|------------|----------|
| `AtTrace` | ~40 bytes | Trace always present; captured at construction |
| `Box<AtTrace>` | 8 bytes | One heap allocation; trace always present |
| `Option<Box<AtTrace>>` | 8 bytes | Lazy — no allocation until first `.at_*()` call |

For lazy allocation:

```rust
struct MyError {
    kind: ErrorKind,
    trace: Option<Box<AtTrace>>,
}

impl AtTraceable for MyError {
    fn trace_mut(&mut self) -> &mut AtTrace {
        self.trace.get_or_insert_with(|| Box::new(AtTrace::new()))
    }

    fn trace(&self) -> Option<&AtTrace> {
        self.trace.as_deref()
    }

    // ...
}
```

## Converting Between Approaches

```rust
// At<A> → At<B>: change error type, keep trace
let b: At<KindB> = a.map_error(|kind_a| convert(kind_a));

// At<E> → MyTraceable: transfer trace into embedded type
let my_err: MyError = at_err.into_traceable(|kind| MyError::from(kind));

// MyTraceable → At<B>: extract trace into wrapper
let at_b: At<KindB> = my_err.into_at(|e| convert(e.kind));
```

## Formatters

`AtTraceable` provides the same formatters as `At<E>`:

```rust
println!("{}", err.full_trace());        // message + locations + contexts
println!("{}", err.last_error_trace());  // message + locations only
println!("{}", err.last_error());        // message only
```
