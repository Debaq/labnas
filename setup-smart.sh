#!/bin/bash
# setup-smart.sh - Permite a LabNAS leer la salud de los discos (SMART) sin correr como root
# Uso: sudo bash setup-smart.sh [usuario-del-servicio]
#
# Instala un wrapper de solo lectura (/usr/local/lib/labnas/smart-read) y una regla de
# sudo que permite al usuario del servicio ejecutar SOLO ese wrapper. Despues activa
# SMART en Configuracion > Sistema.

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

if [ "$(id -u)" -ne 0 ]; then
    echo -e "${RED}Ejecuta con sudo: sudo bash setup-smart.sh [usuario]${NC}"
    exit 1
fi

SERVICE_USER="${1:-${SUDO_USER:-}}"
if [ -z "$SERVICE_USER" ] || [ "$SERVICE_USER" = "root" ]; then
    echo -e "${RED}Indica el usuario que corre LabNAS: sudo bash setup-smart.sh <usuario>${NC}"
    exit 1
fi
id "$SERVICE_USER" > /dev/null

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WRAPPER_SRC="$SCRIPT_DIR/labnas-smart-read"
WRAPPER_DST="/usr/local/lib/labnas/smart-read"
SUDOERS_FILE="/etc/sudoers.d/labnas-smart"

echo "=== SMART para LabNAS (usuario: ${SERVICE_USER}) ==="

# 1. smartmontools
if ! command -v smartctl > /dev/null; then
    echo "[1/3] Instalando smartmontools..."
    if command -v pacman > /dev/null; then pacman -S --needed --noconfirm smartmontools
    elif command -v apt-get > /dev/null; then apt-get update && apt-get install -y smartmontools
    elif command -v dnf > /dev/null; then dnf install -y smartmontools
    else echo -e "${RED}Instala smartmontools manualmente${NC}"; exit 1; fi
else
    echo "[1/3] smartmontools ya instalado"
fi

# 2. wrapper de solo lectura (root:root, no editable por el usuario del servicio)
echo "[2/3] Instalando wrapper en ${WRAPPER_DST}"
install -D -o root -g root -m 0755 "$WRAPPER_SRC" "$WRAPPER_DST"

# 3. regla de sudo validada antes de instalarla
echo "[3/3] Regla de sudo en ${SUDOERS_FILE}"
TMP="$(mktemp)"
echo "${SERVICE_USER} ALL=(root) NOPASSWD: ${WRAPPER_DST}" > "$TMP"
if ! visudo -cf "$TMP" > /dev/null; then
    rm -f "$TMP"
    echo -e "${RED}La regla de sudo no es valida; no se instalo nada${NC}"
    exit 1
fi
install -o root -g root -m 0440 "$TMP" "$SUDOERS_FILE"
rm -f "$TMP"

echo -e "${GREEN}Listo.${NC} Activa SMART en ${YELLOW}Configuracion > Sistema${NC}."
echo "Para quitarlo: sudo rm ${SUDOERS_FILE} ${WRAPPER_DST}"
