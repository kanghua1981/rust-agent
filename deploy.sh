#!/bin/bash
# deploy.sh — Build the Docker image and save it as a .tar.gz for manual deployment.
#
# Usage:
#   ./deploy.sh              # build + save to ./dist/rust-agent.tar.gz
#   ./deploy.sh --skip-build # re-package the last local image without rebuilding

set -euo pipefail

SKIP_BUILD=false
IMAGE_NAME="rust-agent"
IMAGE_TAG="latest"
OUTPUT_DIR="./dist"
OUTPUT_FILE="${OUTPUT_DIR}/${IMAGE_NAME}.tar.gz"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build) SKIP_BUILD=true; shift ;;
        --tag)        IMAGE_TAG="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

FULL_IMAGE="${IMAGE_NAME}:${IMAGE_TAG}"

# ── Build ──────────────────────────────────────────────────────────────────────
if [[ "$SKIP_BUILD" == false ]]; then
    echo "▶ Compiling musl binary..."
    ./build.sh
fi

# Stage the binary into dist/ so it isn't blocked by .dockerignore (target/ is excluded).
mkdir -p "$OUTPUT_DIR"
cp target/x86_64-unknown-linux-musl/release/agent "${OUTPUT_DIR}/agent"

if [[ "$SKIP_BUILD" == false ]]; then
    echo "▶ Building Docker image..."
    docker build -t "$FULL_IMAGE" .
else
    echo "▶ Skipping build (--skip-build)"
fi

# ── Save ───────────────────────────────────────────────────────────────────────
mkdir -p "$OUTPUT_DIR"
echo "▶ Saving image to ${OUTPUT_FILE} ..."
docker save "$FULL_IMAGE" | gzip > "$OUTPUT_FILE"

# Also copy the compose + env files so everything is in one place.
cp docker-compose.yml "$OUTPUT_DIR/"
cp .env.example       "$OUTPUT_DIR/"

SIZE=$(du -sh "$OUTPUT_FILE" | cut -f1)
echo ""
echo "✓ Done! Package saved to: ${OUTPUT_DIR}/"
echo "  $(ls -1 "$OUTPUT_DIR/")"
echo "  Image size: ${SIZE}"
echo ""
echo "── On the server ─────────────────────────────────────────────────────────"
echo "  # 1. Upload the dist/ directory to the server, then:"
echo "  docker load < rust-agent.tar.gz"
echo "  cp .env.example .env && nano .env   # fill in API keys"
echo "  docker compose up -d"

