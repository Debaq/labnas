#!/bin/bash
set -e

# Colores
PURPLE='\033[0;35m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
BACKEND_DIR="$ROOT_DIR/backend"
FRONTEND_DIR="$ROOT_DIR/frontend"
DIST_DIR="$FRONTEND_DIR/dist"

header() {
    clear
    echo -e "${PURPLE}${BOLD}"
    echo "  ╔═══════════════════════════════════════╗"
    echo "  ║             LabNAS v0.3.0             ║"
    echo "  ║   NAS de Laboratorio + Scanner Red    ║"
    echo "  ╚═══════════════════════════════════════╝"
    echo -e "${NC}"
}

check_deps() {
    local missing=0
    for cmd in cargo node npm; do
        if ! command -v $cmd &>/dev/null; then
            echo -e "  ${RED}✗ $cmd no encontrado${NC}"
            missing=1
        fi
    done
    if [ $missing -eq 1 ]; then
        echo -e "\n  ${RED}Instala las dependencias faltantes antes de continuar.${NC}"
        exit 1
    fi
}

install_frontend_deps() {
    if [ ! -d "$FRONTEND_DIR/node_modules" ]; then
        echo -e "  ${CYAN}Instalando dependencias del frontend...${NC}"
        cd "$FRONTEND_DIR" && npm install --silent
    fi
}

# --- Opciones del menu ---

dev_mode() {
    header
    echo -e "  ${GREEN}▶ Modo Desarrollo${NC}\n"
    install_frontend_deps

    echo -e "  ${CYAN}Iniciando backend (cargo run)...${NC}"
    echo -e "  ${CYAN}Iniciando frontend (vite dev)...${NC}"
    echo -e "  ${YELLOW}Backend: http://localhost:3001${NC}"
    echo -e "  ${YELLOW}Frontend: http://localhost:5173${NC}"
    echo -e "  ${YELLOW}(Ctrl+C para detener ambos)${NC}\n"

    # Lanzar backend y frontend en paralelo
    trap 'kill 0; exit' SIGINT SIGTERM
    (cd "$BACKEND_DIR" && cargo run 2>&1 | sed 's/^/  [backend] /') &
    (cd "$FRONTEND_DIR" && npx vite --host 2>&1 | sed 's/^/  [frontend] /') &
    wait
}

build_all() {
    header
    echo -e "  ${GREEN}▶ Compilando proyecto completo${NC}\n"
    install_frontend_deps

    # Build frontend
    echo -e "  ${CYAN}[1/2] Compilando frontend...${NC}"
    cd "$FRONTEND_DIR" && npx vite build --emptyOutDir
    echo -e "  ${GREEN}  ✓ Frontend compilado en $DIST_DIR${NC}\n"

    # Build backend (release)
    echo -e "  ${CYAN}[2/2] Compilando backend (release)...${NC}"
    cd "$BACKEND_DIR" && cargo build --release
    echo -e "  ${GREEN}  ✓ Backend compilado${NC}\n"

    # Copiar dist al lado del binario
    local bin_dir="$BACKEND_DIR/target/release"
    if [ -d "$DIST_DIR" ]; then
        cp -r "$DIST_DIR" "$bin_dir/dist"
        echo -e "  ${GREEN}  ✓ Frontend copiado junto al binario${NC}"
    fi

    echo -e "\n  ${BOLD}${GREEN}Build completo!${NC}"
    echo -e "  ${CYAN}Binario:${NC} $bin_dir/labnas-backend"
    echo -e "  ${CYAN}Frontend:${NC} $bin_dir/dist/"
    echo -e "\n  ${YELLOW}Para ejecutar:${NC}"
    echo -e "  sudo $bin_dir/labnas-backend"
    echo ""
}

run_production() {
    header
    echo -e "  ${GREEN}▶ Modo Produccion${NC}\n"

    local bin="$BACKEND_DIR/target/release/labnas-backend"

    if [ ! -f "$bin" ]; then
        echo -e "  ${RED}No se encontro el binario compilado.${NC}"
        echo -e "  ${YELLOW}Ejecuta primero la opcion 2 (Compilar).${NC}"
        echo ""
        read -p "  Presiona Enter para volver..."
        return
    fi

    if [ ! -d "$BACKEND_DIR/target/release/dist" ]; then
        # Intentar compilar frontend y copiar
        if [ -d "$DIST_DIR" ]; then
            cp -r "$DIST_DIR" "$BACKEND_DIR/target/release/dist"
        else
            echo -e "  ${YELLOW}Frontend no compilado. Compilando...${NC}"
            install_frontend_deps
            cd "$FRONTEND_DIR" && npx vite build --emptyOutDir
            cp -r "$DIST_DIR" "$BACKEND_DIR/target/release/dist"
        fi
    fi

    echo -e "  ${CYAN}LabNAS corriendo en: ${BOLD}http://localhost:3001${NC}"
    echo -e "  ${YELLOW}(Ctrl+C para detener)${NC}\n"

    sudo "$bin"
}

dev_backend_only() {
    header
    echo -e "  ${GREEN}▶ Solo Backend (dev)${NC}\n"
    echo -e "  ${YELLOW}http://localhost:3001${NC}"
    echo -e "  ${YELLOW}(Ctrl+C para detener)${NC}\n"

    cd "$BACKEND_DIR" && cargo run
}

dev_frontend_only() {
    header
    echo -e "  ${GREEN}▶ Solo Frontend (dev)${NC}\n"
    install_frontend_deps
    echo -e "  ${YELLOW}http://localhost:5173${NC}"
    echo -e "  ${YELLOW}(Ctrl+C para detener)${NC}\n"

    cd "$FRONTEND_DIR" && npx vite --host
}

# --- Menu principal ---

main_menu() {
    while true; do
        header
        check_deps
        echo -e "  ${BOLD}Selecciona una opcion:${NC}\n"
        echo -e "  ${CYAN}1)${NC} Desarrollo       ${PURPLE}— Backend + Frontend con hot reload${NC}"
        echo -e "  ${CYAN}2)${NC} Compilar          ${PURPLE}— Build de produccion (binario unico)${NC}"
        echo -e "  ${CYAN}3)${NC} Produccion        ${PURPLE}— Ejecutar build compilado${NC}"
        echo -e "  ${CYAN}4)${NC} Solo Backend      ${PURPLE}— cargo run (dev)${NC}"
        echo -e "  ${CYAN}5)${NC} Solo Frontend     ${PURPLE}— vite dev (dev)${NC}"
        echo -e "  ${CYAN}0)${NC} Salir"
        echo ""
        read -p "  > " choice

        case $choice in
            1) dev_mode ;;
            2) build_all; read -p "  Presiona Enter para volver..." ;;
            3) run_production ;;
            4) dev_backend_only ;;
            5) dev_frontend_only ;;
            0) echo -e "\n  ${PURPLE}Hasta luego!${NC}\n"; exit 0 ;;
            *) echo -e "\n  ${RED}Opcion invalida${NC}"; sleep 1 ;;
        esac
    done
}

# Si se pasa argumento directo
case "${1:-}" in
    dev)   dev_mode ;;
    build) build_all ;;
    run)   run_production ;;
    *)     main_menu ;;
esac
