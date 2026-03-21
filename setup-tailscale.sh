#!/bin/bash
set -e

# Colores
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

echo -e "${BOLD}${CYAN}"
echo "  ╔═══════════════════════════════════════╗"
echo "  ║   LabNAS - Configurar Tailscale       ║"
echo "  ║   Acceso remoto seguro                ║"
echo "  ╚═══════════════════════════════════════╝"
echo -e "${NC}"

# Detectar distro
if command -v pacman &>/dev/null; then
    PKG_MANAGER="pacman"
elif command -v apt &>/dev/null; then
    PKG_MANAGER="apt"
elif command -v dnf &>/dev/null; then
    PKG_MANAGER="dnf"
else
    echo -e "${RED}No se detecto gestor de paquetes (pacman/apt/dnf)${NC}"
    exit 1
fi

# Verificar si Tailscale ya esta instalado
if command -v tailscale &>/dev/null; then
    echo -e "  ${GREEN}✓ Tailscale ya esta instalado${NC}"
else
    echo -e "  ${CYAN}Instalando Tailscale...${NC}"
    case $PKG_MANAGER in
        pacman) sudo pacman -S --noconfirm tailscale ;;
        apt) curl -fsSL https://tailscale.com/install.sh | sh ;;
        dnf) sudo dnf install -y tailscale ;;
    esac
    echo -e "  ${GREEN}✓ Tailscale instalado${NC}"
fi

# Habilitar y arrancar el servicio
echo -e "  ${CYAN}Habilitando servicio...${NC}"
sudo systemctl enable --now tailscaled
echo -e "  ${GREEN}✓ Servicio activo${NC}"

# Verificar estado
STATUS=$(tailscale status 2>&1 || true)
if echo "$STATUS" | grep -q "Logged out" || echo "$STATUS" | grep -q "NeedsLogin"; then
    echo ""
    echo -e "  ${YELLOW}Necesitas iniciar sesion en Tailscale.${NC}"
    echo -e "  ${YELLOW}Se abrira un link en el navegador:${NC}"
    echo ""
    sudo tailscale up
    echo ""
fi

# Mostrar IP de Tailscale
TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "?")

echo ""
echo -e "  ${GREEN}${BOLD}Tailscale configurado!${NC}"
echo ""
echo -e "  ${CYAN}Tu IP Tailscale:${NC} ${BOLD}${TAILSCALE_IP}${NC}"
echo -e "  ${CYAN}LabNAS remoto:${NC}  ${BOLD}http://${TAILSCALE_IP}:3001${NC}"
echo ""
echo -e "  ${YELLOW}Ahora instala Tailscale en tus otros dispositivos:${NC}"
echo -e "  ${YELLOW}  - PC/Mac: https://tailscale.com/download${NC}"
echo -e "  ${YELLOW}  - iPhone: App Store → Tailscale${NC}"
echo -e "  ${YELLOW}  - Android: Play Store → Tailscale${NC}"
echo ""
echo -e "  ${YELLOW}Usa la misma cuenta en todos los dispositivos.${NC}"
echo -e "  ${YELLOW}Luego accede a http://${TAILSCALE_IP}:3001 desde cualquier parte.${NC}"
echo ""
