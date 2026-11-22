#!/usr/bin/env bash
set -euo pipefail

# Per-user install; no USBGuard, Polkit, or system service changes.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="io.github.tobiasguta.PlugWard"
BIN_DIR="${HOME}/.local/bin"
DATA_DIR="${XDG_DATA_HOME:-${HOME}/.local/share}"

cd "$ROOT"
cargo build --release
install -d "$BIN_DIR" "$DATA_DIR/applications" "$DATA_DIR/icons/hicolor/scalable/apps"
install -m 0755 "$ROOT/target/release/plugward" "$BIN_DIR/plugward"
install -m 0644 "$ROOT/data/${APP_ID}.desktop" "$DATA_DIR/applications/${APP_ID}.desktop"
install -m 0644 "$ROOT/data/icons/hicolor/scalable/apps/${APP_ID}.svg" "$DATA_DIR/icons/hicolor/scalable/apps/${APP_ID}.svg"

printf "Installed PlugWard and its GNOME launcher/icon for %s\n" "$USER"
printf "You may need to log out and back in to see the launcher.\n"
