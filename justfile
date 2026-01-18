# whereat development commands

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

# Run tests with all feature combinations (internal features)
test-all:
    cargo test
    cargo test --features _tinyvec-64-bytes
    cargo test --features _tinyvec-128-bytes
    cargo test --features _tinyvec-256-bytes

# Check for outdated dependencies
outdated:
    cargo outdated

# Run stack trace example
example-trace:
    cargo run --example stack_trace

# Run anyhow/thiserror integration example
example-anyhow:
    cargo run --example anyhow_thiserror --features std

# Run CrateInfo example with GitHub links
example-meta:
    cargo run --example error_meta

# Run patterns example (good/bad/ugly usage patterns)
example-patterns:
    cargo run --example patterns

# Run pretty output example (terminal colors and HTML)
example-pretty:
    cargo run --example pretty_output --features "_termcolor,_html"

# Run benchmarks
bench:
    cargo bench --bench overhead

# Run specific benchmark group
bench-group group:
    cargo bench --bench overhead -- "{{group}}"

# Run focused at_depth benchmark (~10s, tests 1/4/8/16 at() calls)
bench-depth:
    cargo bench --bench at_depth

# Windows PowerShell for WSL->Windows execution
pwsh := "pwsh.exe"
wsl_path := "\\\\wsl.localhost\\Ubuntu-22.04"

# Run nested_loops benchmark (all variants comparison)
bench-nested:
    cargo bench --bench nested_loops

# Run tests on Windows host (from WSL)
test-win:
    {{pwsh}} -NoProfile -Command "\$env:CARGO_INCREMENTAL=0; Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo test"

# Run benchmarks on Windows host (from WSL)
bench-win:
    {{pwsh}} -NoProfile -Command "\$env:CARGO_INCREMENTAL=0; Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo bench --bench nested_loops"

# Run specific benchmark on Windows host
bench-win-group group:
    {{pwsh}} -NoProfile -Command "\$env:CARGO_INCREMENTAL=0; Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo bench --bench nested_loops -- '{{group}}'"

# Run single_error benchmarks with RUST_BACKTRACE=1 (for accurate anyhow/panic comparison)
bench-backtrace:
    @echo "=== Linux (RUST_BACKTRACE=0) ==="
    RUST_BACKTRACE=0 cargo bench --bench nested_loops "single_error" 2>&1 | grep -E "^single_error|time:"
    @echo ""
    @echo "=== Linux (RUST_BACKTRACE=1) ==="
    RUST_BACKTRACE=1 cargo bench --bench nested_loops "single_error" 2>&1 | grep -E "^single_error|time:"

# Run single_error benchmarks on Windows with RUST_BACKTRACE variants
bench-backtrace-win:
    @echo "=== Windows (RUST_BACKTRACE=0) ==="
    {{pwsh}} -NoProfile -Command "\$env:CARGO_INCREMENTAL=0; \$env:RUST_BACKTRACE=0; Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo bench --bench nested_loops 'single_error'"
    @echo ""
    @echo "=== Windows (RUST_BACKTRACE=1) ==="
    {{pwsh}} -NoProfile -Command "\$env:CARGO_INCREMENTAL=0; \$env:RUST_BACKTRACE=1; Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo bench --bench nested_loops 'single_error'"
