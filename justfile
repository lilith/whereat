# errat development commands

# Run all checks (fmt, clippy, test)
check:
    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test

# Format code
fmt:
    cargo fmt

# Run clippy
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Run tests
test:
    cargo test

# Run tests with all feature combinations
test-all:
    cargo test
    cargo test --features tinyvec-64-bytes
    cargo test --features tinyvec-128-bytes
    cargo test --features tinyvec-256-bytes

# Check for outdated dependencies
outdated:
    cargo outdated

# Run stack trace example
example-trace:
    cargo run --example stack_trace

# Run anyhow/thiserror integration example
example-anyhow:
    cargo run --example anyhow_thiserror --features std

# Run ErrorMeta example with GitHub links
example-meta:
    cargo run --example error_meta
