+++
title = "Installation"
description = "Add whereat to your Rust project"
weight = 1
+++

# Installation

Add whereat to your `Cargo.toml`:

```toml
[dependencies]
whereat = "0.1"
```

Requires Rust 1.85+ (2024 edition). Works with `no_std` + `alloc` by default — no `std` feature needed unless you want it.

## Optional Features

| Feature | What It Does |
|---------|-------------|
| `std` | Adds `std::error::Error` impls (not needed on Rust 1.85+, `core::error` works) |
| `_termcolor` | Terminal-colored error output via `owo-colors` |
| `_html` | HTML-formatted error output with Catppuccin Mocha theme |
| `_tinyvec-128-bytes` | Inline location storage (12 slots, reduces allocations) |
| `_smallvec-128-bytes` | Inline location storage via smallvec (best Linux perf) |

For cross-crate tracing with GitHub links, add this to your crate root (`lib.rs` or `main.rs`):

```rust
whereat::define_at_crate_info!();
```

This captures your crate name, repository URL, and commit hash at compile time. If your crate is in a subdirectory of the git repo:

```rust
whereat::define_at_crate_info!(path = "crates/mylib/");
```

## Workspace Setup

In a workspace, each crate that uses `at!()` or `at_crate!()` needs its own `define_at_crate_info!()` call. The macro reads `CARGO_PKG_NAME` and `CARGO_PKG_REPOSITORY` from the crate being compiled.

Crates that only use `at()` (without the macro) don't need `define_at_crate_info!()`.
