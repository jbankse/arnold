#!/usr/bin/env bash
set -euo pipefail
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"
ARNOLD_DIR="${ARNOLD_DIR:-$HOME/.arnold}"
OS="$(uname)"

if [[ "$OS" == "Darwin" ]]; then
  for unit in arnoldd arnold-tray; do
    PLIST="$HOME/Library/LaunchAgents/com.arnold.${unit}.plist"
    if [ -f "$PLIST" ]; then
      launchctl unload "$PLIST" 2>/dev/null || true
      rm -f "$PLIST"
    fi
  done
elif [[ "$OS" == "Linux" ]]; then
  systemctl --user disable --now arnoldd.service 2>/dev/null || true
  rm -f "$HOME/.config/systemd/user/arnoldd.service"
  systemctl --user daemon-reload || true
fi

rm -f "$BIN_DIR/arnoldd" "$BIN_DIR/arnold" "$BIN_DIR/arnold-tray"
echo "Removed binaries + autostart units. To remove state, manually delete $ARNOLD_DIR"
