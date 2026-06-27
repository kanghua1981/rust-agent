#!/bin/bash
set -euo pipefail
# Build script for rust-agent
#
# Usage:
#   ./build.sh                              → gnu, 无 browser (9.2 MB)
#   ./build.sh x86_64-unknown-linux-musl    → musl, 无 browser
#   ./build.sh x86_64-unknown-linux-musl browser → musl + browser (13 MB)

TARGET="${1:-x86_64-unknown-linux-gnu}"
FEATURES="${2:-}"
OUTPUT_DIR="dist"

echo "=== Building agent ==="
echo "  Target:   $TARGET"
echo "  Features: ${FEATURES:-default}"

if [ -n "$FEATURES" ]; then
    cargo build --release --target "$TARGET" --features "$FEATURES"
else
    cargo build --release --target "$TARGET"
fi

# 收集产物
mkdir -p "$OUTPUT_DIR"
ARCH=$(echo "$TARGET" | grep -o 'musl\|gnu' || echo "$TARGET")
cp "target/$TARGET/release/agent" "$OUTPUT_DIR/agent-${ARCH}"
ls -lh "$OUTPUT_DIR/agent-${ARCH}"
echo "✅ Done: $OUTPUT_DIR/agent-${ARCH}"
