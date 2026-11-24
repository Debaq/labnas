#!/bin/bash
# Instala labnas-tray (control de musica de LabNAS en la bandeja) para el usuario actual.
# Requiere: gtk3 y libayatana-appindicator (Arch: pacman -S libayatana-appindicator;
# Debian/Ubuntu: apt install libayatana-appindicator3-1). En GNOME hace falta la
# extension "AppIndicator and KStatusNotifierItem Support" para ver el icono.
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

mkdir -p "$PREFIX/share/applications"
cat > "$PREFIX/share/applications/labnas-tray.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=LabNAS (bandeja)
Comment=Control de la música de LabNAS
Exec="$PREFIX/bin/labnas-tray"
Icon=io.github.debaq.LabNAS
Terminal=false
Categories=AudioVideo;Network;
DESKTOP
update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true

echo "✓ labnas-tray instalado en $PREFIX/bin"
echo "  Ábrelo desde el menú de aplicaciones (\"LabNAS (bandeja)\") y elige"
echo "  «Vincular con mi cuenta» en el icono. Para iniciarlo con la sesión, marca"
echo "  «Iniciar con el sistema» en el mismo menú."
echo "  Otra dirección: LABNAS_URL=https://IP:PUERTO labnas-tray  (o url= en ~/.config/labnas-tray/config)"
