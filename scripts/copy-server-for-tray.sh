#!/usr/bin/env bash
# Copy the locally-built julie-server into the tray sidecar directory
# so `npx tauri dev` or `npx tauri build` can find it.
#
# Usage: ./scripts/copy-server-for-tray.sh [--release]

set -euo pipefail

PROFILE="debug"
if [[ "${1:-}" == "--release" ]]; then
    PROFILE="release"
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="${REPO_ROOT}/target"
BINARIES_DIR="${REPO_ROOT}/tauri-app/src-tauri/binaries"

# Detect current platform triple
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)  TRIPLE="aarch64-apple-darwin" ;;
    Darwin-x86_64) TRIPLE="x86_64-apple-darwin" ;;
    Linux-x86_64)  TRIPLE="x86_64-unknown-linux-gnu" ;;
    *)             echo "Unsupported platform: $(uname -s)-$(uname -m)"; exit 1 ;;
esac

SRC="${TARGET_DIR}/${PROFILE}/julie-server"
if [[ ! -f "$SRC" ]]; then
    echo "Server binary not found at ${SRC}"
    if [[ "$PROFILE" == "release" ]]; then
        echo "Run: cargo build --release --bin julie-server"
    else
        echo "Run: cargo build --bin julie-server"
    fi
    exit 1
fi

DEST="${BINARIES_DIR}/julie-server-${TRIPLE}"
mkdir -p "$BINARIES_DIR"
cp "$SRC" "$DEST"
echo "Copied: ${SRC} → ${DEST}"
