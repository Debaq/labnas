#!/bin/bash
# Instala labnas-tray (control de musica de LabNAS en la bandeja) para el usuario actual.
# Requiere: gtk3 y webkit2gtk-4.1
#   Arch:          pacman -S webkit2gtk-4.1
#   Debian/Ubuntu: apt install libwebkit2gtk-4.1-0
# El icono usa StatusNotifierItem (KDE, XFCE, Cinnamon... lo traen). En GNOME hace
# falta la extension "AppIndicator and KStatusNotifierItem Support".
set -e

cd "$(dirname "$0")"
PREFIX="${PREFIX:-$HOME/.local}"

# El tarball del release trae el binario; desde el repo se compila
if [ -f ./labnas-tray ]; then
    BIN=./labnas-tray
else
    cargo build --release
    TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
    BIN="$TARGET_DIR/release/labnas-tray"
fi

install -Dm755 "$BIN" "$PREFIX/bin/labnas-tray"

# Direccion del LabNAS desde el que se descargo (la agrega LabNAS al tarball)
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/labnas-tray"
if [ -f ./labnas-tray.conf ] && ! grep -qs '^url=' "$CONFIG_DIR/config"; then
    install -d -m700 "$CONFIG_DIR"
    grep '^url=' ./labnas-tray.conf >> "$CONFIG_DIR/config"
    chmod 600 "$CONFIG_DIR/config"
    echo "✓ LabNAS: $(sed -n 's/^url=//p' ./labnas-tray.conf)"
fi

mkdir -p "$PREFIX/share/applications"
cat > "$PREFIX/share/applications/labnas-tray.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=LabNAS (bandeja)
Comment=Control de la música de LabNAS
Exec="$PREFIX/bin/labnas-tray" --abrir
Icon=io.github.debaq.LabNAS
Terminal=false
Categories=AudioVideo;Network;
DESKTOP
update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true

echo "✓ labnas-tray instalado en $PREFIX/bin"
echo "  Ábrelo desde el menú de aplicaciones (\"LabNAS (bandeja)\"): el asistente"
echo "  te guía para vincular tu cuenta. Para iniciarlo con la sesión, marca"
echo "  «Iniciar con el sistema» en el menú del icono."
echo "  Otra dirección: LABNAS_URL=https://IP:PUERTO labnas-tray  (o url= en ~/.config/labnas-tray/config)"
