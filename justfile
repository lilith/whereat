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

# Run CrateInfo example with GitHub links
example-meta:
    cargo run --example error_meta

# Run patterns example (good/bad/ugly usage patterns)
example-patterns:
    cargo run --example patterns

# Run benchmarks
bench:
    cargo bench --bench overhead

# Run specific benchmark group
bench-group group:
    cargo bench --bench overhead -- "{{group}}"

# Windows PowerShell for WSL->Windows execution
pwsh := "pwsh.exe"
wsl_path := "\\\\wsl.localhost\\Ubuntu-22.04"

# Run nested_loops benchmark (all variants comparison)
bench-nested:
    cargo bench --bench nested_loops

# Run frame benchmarks (quick comparison)
bench-frames frames="40":
    @echo "=== Default (heap) ===" && cargo run --release --example frames_{{frames}}
    @echo "=== tinyvec-128 (12 slots) ===" && cargo run --release --example frames_{{frames}} --features tinyvec-128-bytes
    @echo "=== smallvec-128 (12 slots) ===" && cargo run --release --example frames_{{frames}} --features smallvec-128-bytes
    @echo "=== tinyvec-256 (28 slots) ===" && cargo run --release --example frames_{{frames}} --features tinyvec-256-bytes
    @echo "=== smallvec-256 (28 slots) ===" && cargo run --release --example frames_{{frames}} --features smallvec-256-bytes

# Run 40-frame benchmark (shortcut)
bench-40: (bench-frames "40")

# Run benchmarks on Windows host (from WSL)
bench-win:
    {{pwsh}} -NoProfile -Command "Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo bench --bench nested_loops"

# Run frame benchmarks on Windows host (from WSL)
bench-frames-win frames="40":
    @echo "=== Default (heap) ==="
    {{pwsh}} -NoProfile -Command "Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo run --release --example frames_{{frames}}"
    @echo "=== tinyvec-128 (12 slots) ==="
    {{pwsh}} -NoProfile -Command "Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo run --release --example frames_{{frames}} --features tinyvec-128-bytes"
    @echo "=== smallvec-128 (12 slots) ==="
    {{pwsh}} -NoProfile -Command "Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo run --release --example frames_{{frames}} --features smallvec-128-bytes"
    @echo "=== tinyvec-256 (28 slots) ==="
    {{pwsh}} -NoProfile -Command "Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo run --release --example frames_{{frames}} --features tinyvec-256-bytes"
    @echo "=== smallvec-256 (28 slots) ==="
    {{pwsh}} -NoProfile -Command "Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo run --release --example frames_{{frames}} --features smallvec-256-bytes"

# Run 40-frame benchmark on Windows (shortcut)
bench-40-win: (bench-frames-win "40")

# Run specific benchmark on Windows host
bench-win-group group:
    {{pwsh}} -NoProfile -Command "Set-Location '{{wsl_path}}{{justfile_directory()}}'; cargo bench --bench nested_loops -- '{{group}}'"
