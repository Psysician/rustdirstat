# Task runner. Cross-platform alternative to Makefile; no tab-sensitivity,
# simpler syntax, and native platform-specific recipe support. (ref: DL-003)

# List available recipes
default:
    @just --list

# Build the entire workspace
build:
    cargo build --workspace

# Run all tests
test:
    cargo test --workspace

# Run clippy with warnings as errors
lint:
    cargo clippy --workspace -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run the application
run:
    cargo run

# Type-check the workspace
check:
    cargo check --workspace

# Run benchmarks
bench:
    cargo bench -p rds-core
    cargo bench -p rds-gui --features bench-internals

# Open benchmark report in browser
bench-report:
    #!/usr/bin/env bash
    if command -v xdg-open &> /dev/null; then
        xdg-open target/criterion/report/index.html
    elif command -v open &> /dev/null; then
        open target/criterion/report/index.html
    else
        echo "Open target/criterion/report/index.html in your browser"
    fi

# Compare scan performance against dust and dua via hyperfine
bench-compare dir="/usr":
    ./scripts/benchmark-comparison.sh {{dir}}

# Remove build artifacts
clean:
    cargo clean
