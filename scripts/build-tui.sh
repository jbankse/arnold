#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"

cd "$ROOT/arnold-tui"
go build -o "$BIN_DIR/arnold" ./
echo "Installed arnold TUI to $BIN_DIR/arnold"
