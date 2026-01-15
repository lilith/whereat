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

# Check for outdated dependencies
outdated:
    cargo outdated
