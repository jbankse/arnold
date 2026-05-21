#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"

if [[ "$(uname)" != "Darwin" ]]; then
    echo "arnold-tray is macOS-only in v0b; skipping on $(uname)"
    exit 0
fi

cd "$ROOT/arnold-tray"
go build -o "$BIN_DIR/arnold-tray" ./
echo "Installed arnold-tray to $BIN_DIR/arnold-tray"
