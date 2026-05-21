#!/usr/bin/env bash
set -euo pipefail
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"
ARNOLD_DIR="${ARNOLD_DIR:-$HOME/.arnold}"
rm -f "$BIN_DIR/arnoldd" "$BIN_DIR/arnold"
echo "Removed binaries. To remove state, manually delete $ARNOLD_DIR"
