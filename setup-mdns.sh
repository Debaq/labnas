#!/bin/bash
# setup-mdns.sh - Configura mDNS para que labnas.local funcione en este equipo
# Uso: sudo bash setup-mdns.sh

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=== Configuracion mDNS para LabNAS ==="
echo ""

# Detectar distro
if [ -f /etc/os-release ]; then
    . /etc/os-release
    DISTRO="$ID"
else
    DISTRO="unknown"
fi

echo -e "Distro detectada: ${YELLOW}${DISTRO}${NC}"
echo ""

# 1. Instalar avahi y nss-mdns
echo "[1/4] Instalando avahi y nss-mdns..."
case "$DISTRO" in
    arch|manjaro|endeavouros)
        pacman -S --needed --noconfirm avahi nss-mdns
        ;;
    ubuntu|debian|linuxmint|pop)
        apt-get update -qq
        apt-get install -y avahi-daemon libnss-mdns
        ;;
    fedora)
        dnf install -y avahi avahi-tools nss-mdns
        ;;
    opensuse*|sles)
        zypper install -y avahi nss-mdns
        ;;
    *)
        echo -e "${RED}Distro no soportada automaticamente.${NC}"
        echo "Instala manualmente: avahi-daemon y nss-mdns (o equivalentes)"
        exit 1
        ;;
esac
echo -e "${GREEN}OK${NC}"

# 2. Activar avahi-daemon
echo ""
echo "[2/4] Activando avahi-daemon..."
systemctl enable --now avahi-daemon
echo -e "${GREEN}OK${NC}"

# 3. Configurar nsswitch.conf
echo ""
echo "[3/4] Configurando /etc/nsswitch.conf..."
if grep -q "mdns4_minimal" /etc/nsswitch.conf; then
    echo -e "${GREEN}Ya configurado${NC}"
else
    # Backup
    cp /etc/nsswitch.conf /etc/nsswitch.conf.bak
    # Agregar mdns4_minimal antes de dns
    sed -i 's/^hosts:.*/hosts: mymachines mdns4_minimal [NOTFOUND=return] resolve [!UNAVAIL=return] files dns/' /etc/nsswitch.conf
    echo -e "${GREEN}OK (backup en nsswitch.conf.bak)${NC}"
fi

# 4. Verificar firewall
echo ""
echo "[4/4] Verificando firewall (UDP 5353)..."

if command -v ufw &>/dev/null; then
    if ufw status | grep -q "active"; then
        ufw allow 5353/udp comment "mDNS" 2>/dev/null
        echo -e "${GREEN}ufw: puerto 5353/udp abierto${NC}"
    else
        echo -e "${YELLOW}ufw inactivo, no se necesita regla${NC}"
    fi
elif command -v firewall-cmd &>/dev/null; then
    if firewall-cmd --state 2>/dev/null | grep -q "running"; then
        firewall-cmd --permanent --add-service=mdns
        firewall-cmd --reload
        echo -e "${GREEN}firewalld: servicio mdns habilitado${NC}"
    else
        echo -e "${YELLOW}firewalld inactivo${NC}"
    fi
elif command -v iptables &>/dev/null; then
    if iptables -C INPUT -p udp --dport 5353 -j ACCEPT 2>/dev/null; then
        echo -e "${GREEN}iptables: regla ya existe${NC}"
    else
        iptables -I INPUT -p udp --dport 5353 -j ACCEPT
        echo -e "${GREEN}iptables: regla agregada${NC}"
    fi
else
    echo -e "${YELLOW}No se detecto firewall activo${NC}"
fi

# Test
echo ""
echo "=== Verificacion ==="
if avahi-resolve -n labnas.local 2>/dev/null; then
    echo ""
    echo -e "${GREEN}labnas.local resuelve correctamente!${NC}"
else
    echo -e "${YELLOW}labnas.local no resuelve aun.${NC}"
    echo "Verifica que el NAS tenga mDNS activado en Configuracion."
    echo "Tambien verifica que ambos equipos esten en la misma red."
fi

echo ""
echo "Listo. Abre http://labnas.local:3001 en el navegador."
