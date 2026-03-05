+++
title = "Pretty Formatters"
description = "Terminal color and HTML output"
weight = 2
+++

# Pretty Formatters

whereat includes optional formatters for colored terminal output and HTML rendering.

## Terminal Colors

Enable with:

```toml
[dependencies]
whereat = { version = "0.1", features = ["_termcolor"] }
```

```rust
let err: At<MyError> = at(MyError).at_str("loading config");

// Colored output (uses owo-colors)
println!("{}", err.display_color());

// Colored output with repository links
println!("{}", err.display_color_meta());
```

Colors follow the terminal's color scheme:
- Error type in **red**
- File paths in **cyan**
- Line numbers in **yellow**
- Context strings in **green**
- Crate boundaries in **blue**

## HTML Output

Enable with:

```toml
[dependencies]
whereat = { version = "0.1", features = ["_html"] }
```

```rust
// HTML without styles (bring your own CSS)
println!("{}", err.display_html());

// HTML with embedded <style> block (Catppuccin Mocha theme)
println!("{}", err.display_html_styled());
```

The HTML output uses semantic class names: `whereat-error`, `error-header`, `location`, `context`, `context-text`, `context-fn`, `context-data`, `context-error`, `crate-boundary`, `skip-marker`. Override any of them in your own CSS.

All string content is HTML-escaped to prevent XSS.

## Standard Formatters

These work without any feature flags:

| Formatter | Shows |
|-----------|-------|
| `format!("{:?}", err)` | Debug — full trace with all contexts |
| `format!("{}", err)` | Display — just the error message |
| `err.full_trace()` | Message + locations + all contexts |
| `err.last_error_trace()` | Message + locations (no contexts) |
| `err.last_error()` | Message only |
| `err.display_with_meta()` | Full trace with repository links |
