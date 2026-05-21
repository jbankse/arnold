#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"
ARNOLD_DIR="${ARNOLD_DIR:-$HOME/.arnold}"

echo "[1/4] building 439 cpu binary..."
"$HERE/build-cpu.sh" "$ARNOLD_DIR/bin"

echo "[2/4] building arnoldd + arnold..."
cd "$ROOT"
cargo build --release -p arnoldd -p arnold-cli

echo "[3/4] installing binaries..."
mkdir -p "$BIN_DIR" "$ARNOLD_DIR/workspace" "$ARNOLD_DIR/memory" "$ARNOLD_DIR/jobs"
install -m 755 "$ROOT/target/release/arnoldd" "$BIN_DIR/arnoldd"
install -m 755 "$ROOT/target/release/arnold" "$BIN_DIR/arnold"

echo "[4/4] writing default config (if absent)..."
if [ ! -f "$ARNOLD_DIR/config.toml" ]; then
  cat > "$ARNOLD_DIR/config.toml" <<EOF
cpu_binary = "$ARNOLD_DIR/bin/cpu"
llm_provider = "anthropic"
model = "claude-sonnet-4-6"
allowed_dirs = []
EOF
fi

echo
echo "Installed. Start arnoldd manually for v0a:"
echo "  $BIN_DIR/arnoldd"
echo "Then in another terminal:"
echo "  $BIN_DIR/arnold"
echo
echo "launchd/systemd unit installation lands in v0b."
