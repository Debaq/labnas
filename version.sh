#!/bin/bash
set -e

# ── Colores ──
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
VERSION_FILE="$ROOT_DIR/VERSION"

if [ ! -f "$VERSION_FILE" ]; then
    echo -e "${RED}ERROR: No se encontró archivo VERSION${NC}"
    exit 1
fi

CURRENT=$(cat "$VERSION_FILE" | tr -d '[:space:]')

show_version() {
    echo -e "${BOLD}Versión actual:${NC} ${CYAN}v${CURRENT}${NC}"
}

sync_version() {
    local version="$1"

    # Cargo.toml
    sed -i "s/^version = \".*\"/version = \"${version}\"/" "$ROOT_DIR/backend/Cargo.toml"
    echo -e "  ${GREEN}✓${NC} backend/Cargo.toml → ${version}"

    # package.json
    sed -i "s/\"version\": \".*\"/\"version\": \"${version}\"/" "$ROOT_DIR/frontend/package.json"
    echo -e "  ${GREEN}✓${NC} frontend/package.json → ${version}"

    # labnas.sh (header del menú)
    sed -i "s/LabNAS v[0-9]\+\.[0-9]\+\.[0-9]\+/LabNAS v${version}/" "$ROOT_DIR/labnas.sh"
    echo -e "  ${GREEN}✓${NC} labnas.sh → ${version}"
}

bump() {
    local part="$1"
    IFS='.' read -r major minor patch <<< "$CURRENT"

    case "$part" in
        major) major=$((major + 1)); minor=0; patch=0 ;;
        minor) minor=$((minor + 1)); patch=0 ;;
        patch) patch=$((patch + 1)) ;;
        *)
            echo -e "${RED}Uso: $0 bump [major|minor|patch]${NC}"
            exit 1
            ;;
    esac

    local new_version="${major}.${minor}.${patch}"
    echo "$new_version" > "$VERSION_FILE"

    echo -e "${BOLD}Bump:${NC} ${YELLOW}v${CURRENT}${NC} → ${GREEN}v${new_version}${NC}\n"
    sync_version "$new_version"
    echo -e "\n${GREEN}${BOLD}Versión actualizada a v${new_version}${NC}"
}

set_version() {
    local new_version="$1"
    if ! echo "$new_version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
        echo -e "${RED}Formato inválido. Usa: X.Y.Z (ej: 2.0.0)${NC}"
        exit 1
    fi

    echo "$new_version" > "$VERSION_FILE"
    echo -e "${BOLD}Set:${NC} ${YELLOW}v${CURRENT}${NC} → ${GREEN}v${new_version}${NC}\n"
    sync_version "$new_version"
    echo -e "\n${GREEN}${BOLD}Versión actualizada a v${new_version}${NC}"
}

# ── CLI ──
case "${1:-}" in
    ""|show)
        show_version
        ;;
    bump)
        bump "${2:-patch}"
        ;;
    set)
        if [ -z "${2:-}" ]; then
            echo -e "${RED}Uso: $0 set X.Y.Z${NC}"
            exit 1
        fi
        set_version "$2"
        ;;
    sync)
        echo -e "${BOLD}Sincronizando v${CURRENT} en todos los archivos...${NC}\n"
        sync_version "$CURRENT"
        echo -e "\n${GREEN}${BOLD}Sincronización completa${NC}"
        ;;
    *)
        echo -e "${BOLD}version.sh${NC} — Gestión de versión de LabNAS\n"
        echo -e "  ${CYAN}$0${NC}              Muestra versión actual"
        echo -e "  ${CYAN}$0 bump patch${NC}   Incrementa patch (1.5.0 → 1.5.1)"
        echo -e "  ${CYAN}$0 bump minor${NC}   Incrementa minor (1.5.0 → 1.6.0)"
        echo -e "  ${CYAN}$0 bump major${NC}   Incrementa major (1.5.0 → 2.0.0)"
        echo -e "  ${CYAN}$0 set X.Y.Z${NC}   Establece versión específica"
        echo -e "  ${CYAN}$0 sync${NC}         Sincroniza VERSION a todos los archivos"
        ;;
esac
