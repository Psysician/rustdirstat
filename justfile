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

# Remove build artifacts
clean:
    cargo clean
