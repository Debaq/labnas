#!/bin/bash
# setup-snapshots.sh - Snapshots btrfs/ZFS para LabNAS sin correr como root
# Uso: sudo bash setup-snapshots.sh <usuario-del-servicio> <ruta> [<ruta> ...]
#
# Cada <ruta> debe ser un subvolumen btrfs o el punto de montaje de un dataset ZFS.
# Instala un wrapper (/usr/local/lib/labnas/snapshot) que solo actua sobre esas rutas
# y solo crea/lista/borra snapshots labnas-*, y una regla de sudo limitada a ese wrapper.
# Despues configura la frecuencia en Configuracion > Respaldos.

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

if [ "$(id -u)" -ne 0 ]; then
    echo -e "${RED}Ejecuta con sudo: sudo bash setup-snapshots.sh <usuario> <ruta>...${NC}"
    exit 1
fi

SERVICE_USER="${1:-}"
shift || true
if [ -z "$SERVICE_USER" ] || [ "$SERVICE_USER" = "root" ] || [ "$#" -eq 0 ]; then
    echo -e "${RED}Uso: sudo bash setup-snapshots.sh <usuario-del-servicio> <ruta> [<ruta>...]${NC}"
    exit 1
fi
id "$SERVICE_USER" > /dev/null

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WRAPPER_SRC="$SCRIPT_DIR/labnas-snapshot"
WRAPPER_DST="/usr/local/lib/labnas/snapshot"
ALLOW="/etc/labnas/snapshot-paths"
SUDOERS_FILE="/etc/sudoers.d/labnas-snapshot"

echo "=== Snapshots para LabNAS (usuario: ${SERVICE_USER}) ==="

TMP_ALLOW="$(mktemp)"
for p in "$@"; do
    real="$(realpath -e -- "$p")" || { echo -e "${RED}No existe: $p${NC}"; rm -f "$TMP_ALLOW"; exit 1; }
    case "$(stat -f -c %T "$real")" in
        btrfs)
            btrfs subvolume show -- "$real" > /dev/null 2>&1 || { echo -e "${RED}$real no es un subvolumen btrfs (crea uno con: btrfs subvolume create)${NC}"; rm -f "$TMP_ALLOW"; exit 1; }
            ;;
        zfs)
            [ -n "$(zfs list -H -o name -- "$real" 2> /dev/null)" ] || { echo -e "${RED}$real no es un dataset ZFS${NC}"; rm -f "$TMP_ALLOW"; exit 1; }
            ;;
        *)
            echo -e "${RED}$real no esta en btrfs ni ZFS${NC}"; rm -f "$TMP_ALLOW"; exit 1
            ;;
    esac
    echo "$real" >> "$TMP_ALLOW"
    echo "  + $real"
done

install -D -o root -g root -m 0755 "$WRAPPER_SRC" "$WRAPPER_DST"
install -D -o root -g root -m 0644 "$TMP_ALLOW" "$ALLOW"
rm -f "$TMP_ALLOW"

TMP="$(mktemp)"
echo "${SERVICE_USER} ALL=(root) NOPASSWD: ${WRAPPER_DST}" > "$TMP"
if ! visudo -cf "$TMP" > /dev/null; then
    rm -f "$TMP"
    echo -e "${RED}La regla de sudo no es valida; no se instalo${NC}"
    exit 1
fi
install -o root -g root -m 0440 "$TMP" "$SUDOERS_FILE"
rm -f "$TMP"

echo -e "${GREEN}Listo.${NC} Configura la frecuencia en ${YELLOW}Configuracion > Respaldos > Snapshots${NC}."
echo "Para quitarlo: sudo rm ${SUDOERS_FILE} ${WRAPPER_DST} ${ALLOW}"
