#!/bin/bash
# Compila e instala labnas-viewer para el usuario actual (~/.local)
# Requiere: webkit2gtk-4.1 y gtk3 (Arch: pacman -S webkit2gtk-4.1)
set -e

cd "$(dirname "$0")"
PREFIX="${PREFIX:-$HOME/.local}"
APP_ID="io.github.debaq.LabNAS"

cargo build --release
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"

install -Dm755 "$TARGET_DIR/release/labnas-viewer" "$PREFIX/bin/labnas-viewer"
install -Dm644 "assets/$APP_ID.desktop" "$PREFIX/share/applications/$APP_ID.desktop"
install -Dm644 "assets/$APP_ID.svg" "$PREFIX/share/icons/hicolor/scalable/apps/$APP_ID.svg"

update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$PREFIX/share/icons/hicolor" 2>/dev/null || true

echo "✓ labnas-viewer instalado en $PREFIX/bin"
echo "  URL por defecto: http://localhost:3001"
echo "  Para otra URL:   echo 'http://IP:3001' > ~/.config/labnas-viewer/url"
