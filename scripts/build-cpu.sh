#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR="$HERE/../vendor/439"
DEST_DIR="${1:-$HOME/.arnold/bin}"

mkdir -p "$DEST_DIR"
cd "$VENDOR"
cmake -G Ninja -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --target cpu
cp build/cpu/cpu "$DEST_DIR/cpu"
echo "Installed cpu binary to $DEST_DIR/cpu"
