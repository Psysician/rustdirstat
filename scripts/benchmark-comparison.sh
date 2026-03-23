#!/usr/bin/env bash
# Benchmark comparison: rustdirstat vs dust vs dua using hyperfine.
# Outputs results as a markdown table.
#
# Usage: ./scripts/benchmark-comparison.sh [directory]
#   directory  Path to scan (default: /usr)

set -euo pipefail

DIR="${1:-/usr}"

missing=()

if ! command -v hyperfine &>/dev/null; then
    missing+=("hyperfine  — install: cargo install hyperfine  (or: apt install hyperfine)")
fi
if ! command -v dust &>/dev/null; then
    missing+=("dust       — install: cargo install du-dust")
fi
if ! command -v dua &>/dev/null; then
    missing+=("dua        — install: cargo install dua-cli")
fi

if [ ${#missing[@]} -gt 0 ]; then
    echo "Missing required tools:"
    for tool in "${missing[@]}"; do
        echo "  $tool"
    done
    exit 1
fi

# Build rustdirstat in release mode first so compile time is not measured.
echo "Building rustdirstat in release mode..."
cargo build --release --quiet

RUSTDIRSTAT="./target/release/rustdirstat"

echo ""
echo "Benchmarking scan of: $DIR"
echo ""

hyperfine \
    --warmup 1 \
    --min-runs 3 \
    --export-markdown /dev/stdout \
    --command-name "rustdirstat" "$RUSTDIRSTAT --scan-only $DIR" \
    --command-name "dust"        "dust -d0 $DIR" \
    --command-name "dua"         "dua $DIR"
