#!/bin/bash
# Cross-compile the Remote Agent Host for Linux targets.
# Requires: rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
# On non-Linux hosts, also install a cross-linker (e.g. gcc-x86-64-linux-gnu).

set -euo pipefail
cd "$(dirname "$0")/.."

TARGETS=("x86_64-unknown-linux-gnu" "aarch64-unknown-linux-gnu")
BINARY_NAME="remote-agent-host"
OUTPUT_DIR="apps/frontend/src-tauri/binaries"

mkdir -p "$OUTPUT_DIR"

for target in "${TARGETS[@]}"; do
    echo "=== Building for $target ==="
    cargo build -p remote-agent-host --target "$target" --release

    # Copy to embeddings directory
    cp "target/$target/release/$BINARY_NAME" "$OUTPUT_DIR/$BINARY_NAME-${target%%-*}"
    echo "Copied to $OUTPUT_DIR/$BINARY_NAME-${target%%-*}"
done

echo "=== Done ==="
ls -lh "$OUTPUT_DIR/"
