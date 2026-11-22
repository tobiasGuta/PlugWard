#!/usr/bin/env bash
set -euo pipefail

# User-only launcher installation: no USBGuard or Polkit settings are changed.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="io.github.tobiasguta.PlugWard"
BIN_DIR="${HOME}/.local/bin"
DATA_DIR="${XDG_DATA_HOME:-${HOME}/.local/share}"
ICON_DIR="$DATA_DIR/icons/hicolor"
SVG_SOURCE="$ROOT/data/icons/hicolor/scalable/apps/${APP_ID}.svg"
SVG_DEST="$ICON_DIR/scalable/apps/${APP_ID}.svg"
PNG_DEST="$ICON_DIR/256x256/apps/${APP_ID}.png"
DESKTOP_DEST="$DATA_DIR/applications/${APP_ID}.desktop"

cd "$ROOT"
cargo build --release

install -d "$BIN_DIR" "$DATA_DIR/applications" \
  "$ICON_DIR/scalable/apps" "$ICON_DIR/256x256/apps"
install -m 0755 "$ROOT/target/release/plugward" "$BIN_DIR/plugward"
install -m 0644 "$SVG_SOURCE" "$SVG_DEST"

# A rasterized icon plus an absolute Icon path prevents stale/missing user
# icon-theme caches from producing a generic image placeholder in GNOME.
ICON_PATH="$SVG_DEST"
if command -v rsvg-convert >/dev/null 2>&1; then
  rsvg-convert --width 256 --height 256 --output "$PNG_DEST" "$SVG_DEST"
  ICON_PATH="$PNG_DEST"
elif command -v inkscape >/dev/null 2>&1; then
  inkscape "$SVG_DEST" --export-type=png --export-width=256 \
    --export-filename="$PNG_DEST" >/dev/null
  ICON_PATH="$PNG_DEST"
else
  printf "No SVG-to-PNG renderer found; using the SVG directly.\n" >&2
  printf "For PNG fallback on Fedora, install librsvg2-tools or Inkscape.\n" >&2
fi

if [[ ! -s "$ICON_PATH" ]]; then
  printf "Error: generated PlugWard icon is missing or empty: %s\n" "$ICON_PATH" >&2
  exit 1
fi

# Freedesktop desktop entries support absolute paths for Icon. The user-local
# launcher also uses the full executable path rather than relying on GNOME PATH.
cat > "$DESKTOP_DEST" <<EOF
[Desktop Entry]
Type=Application
Name=PlugWard
Comment=GNOME USB security and authorization manager
Exec="$BIN_DIR/plugward"
Icon=$ICON_PATH
Terminal=false
Categories=System;Security;GTK;
StartupNotify=true
EOF
chmod 0644 "$DESKTOP_DEST"

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$DESKTOP_DEST"
fi

printf "Installed PlugWard to %s\n" "$BIN_DIR/plugward"
printf "Launcher: %s\n" "$DESKTOP_DEST"
printf "Icon: %s\n" "$ICON_PATH"
printf "If GNOME still shows an old icon, sign out and back in once.\n"
