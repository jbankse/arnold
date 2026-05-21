#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
VENDOR="$ROOT/vendor/439"
DEST_DIR="${1:-$HOME/.arnold/bin}"

mkdir -p "$DEST_DIR"
cd "$VENDOR"
cargo build --release -p bios
cp target/release/bios "$DEST_DIR/bios"
echo "Installed bios binary to $DEST_DIR/bios"
