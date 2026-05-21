#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"
ARNOLD_DIR="${ARNOLD_DIR:-$HOME/.arnold}"

OS="$(uname)"

echo "[1/8] building 439 cpu binary..."
"$HERE/build-cpu.sh" "$ARNOLD_DIR/bin"

echo "[2/8] building 439 bios binary..."
"$HERE/build-bios.sh" "$ARNOLD_DIR/bin"

echo "[3/8] building arnoldd..."
cd "$ROOT"
cargo build --release -p arnoldd

echo "[4/8] building arnold TUI..."
ARNOLD_BIN_DIR="$BIN_DIR" "$HERE/build-tui.sh"

echo "[5/8] building arnold-tray (macOS only)..."
ARNOLD_BIN_DIR="$BIN_DIR" "$HERE/build-tray.sh"

echo "[6/8] installing arnoldd binary..."
mkdir -p "$BIN_DIR" "$ARNOLD_DIR/workspace" "$ARNOLD_DIR/memory" "$ARNOLD_DIR/jobs"
install -m 755 "$ROOT/target/release/arnoldd" "$BIN_DIR/arnoldd"

echo "[7/8] writing default config (if absent)..."
if [ ! -f "$ARNOLD_DIR/config.toml" ]; then
  cat > "$ARNOLD_DIR/config.toml" <<EOF
cpu_binary = "$ARNOLD_DIR/bin/cpu"
bios_binary = "$ARNOLD_DIR/bin/bios"
runtime_image = "runtime/os:local"
llm_provider = "anthropic"
model = "claude-sonnet-4-6"
allowed_dirs = []
EOF
fi

echo "[8/8] installing autostart service..."
if [[ "$OS" == "Darwin" ]]; then
  AGENTS_DIR="$HOME/Library/LaunchAgents"
  mkdir -p "$AGENTS_DIR"

  for unit in arnoldd arnold-tray; do
    SRC="$HERE/${unit}.plist.template"
    DST="$AGENTS_DIR/com.arnold.${unit}.plist"
    sed -e "s|__BIN_DIR__|$BIN_DIR|g" \
        -e "s|__ARNOLD_DIR__|$ARNOLD_DIR|g" \
        -e "s|__USER_PATH__|$PATH|g" \
        "$SRC" > "$DST"
    launchctl unload "$DST" 2>/dev/null || true
    launchctl load "$DST"
  done

  echo "Loaded com.arnold.arnoldd + com.arnold.arnold-tray via launchctl."
elif [[ "$OS" == "Linux" ]]; then
  UNIT_DIR="$HOME/.config/systemd/user"
  mkdir -p "$UNIT_DIR"
  SRC="$HERE/arnoldd.service.template"
  DST="$UNIT_DIR/arnoldd.service"
  sed -e "s|__BIN_DIR__|$BIN_DIR|g" \
      -e "s|__ARNOLD_DIR__|$ARNOLD_DIR|g" \
      "$SRC" > "$DST"
  systemctl --user daemon-reload
  systemctl --user enable --now arnoldd.service

  echo "Enabled arnoldd.service via systemctl --user."
else
  echo "WARNING: autostart not supported on $OS; you'll need to run arnoldd manually."
fi

# PATH check: make sure $BIN_DIR is reachable from new shells. Append to ~/.zshrc
# with a marker comment so re-installs don't duplicate the line.
ZSHRC="$HOME/.zshrc"
MARKER="# added by arnold install — puts ~/.local/bin (arnold's BIN_DIR) on PATH"
if [ ! -f "$ZSHRC" ] || ! grep -qF "$MARKER" "$ZSHRC"; then
  {
    echo ""
    echo "$MARKER"
    echo "export PATH=\"$BIN_DIR:\$PATH\""
  } >> "$ZSHRC"
  echo "Added $BIN_DIR to PATH in $ZSHRC. Open a new terminal or: source $ZSHRC"
fi

echo
echo "Installed. arnoldd is autostarting; arnold (TUI) is available at:"
echo "  $BIN_DIR/arnold"
if [[ "$OS" == "Darwin" ]]; then
  echo "Menu bar icon should appear within a few seconds."
fi
echo
echo "Once your shell PATH picks up $BIN_DIR, launch the TUI with: arnold"
