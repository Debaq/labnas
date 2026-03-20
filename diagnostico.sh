#!/bin/bash
echo "=== DIAGNOSTICO LABNAS ==="
echo ""

echo "--- Sistema ---"
uname -a
echo ""

echo "--- IP local ---"
ip addr show | grep "inet " | grep -v 127.0.0.1
echo ""

echo "--- Puerto 3001 ---"
ss -tlnp | grep 3001 || echo "NADA escuchando en 3001"
echo ""

echo "--- Servicio systemd ---"
systemctl is-active labnas 2>/dev/null || echo "Servicio labnas no existe"
systemctl status labnas 2>/dev/null | head -15
echo ""

echo "--- Procesos labnas ---"
ps aux | grep labnas | grep -v grep || echo "No hay procesos labnas corriendo"
echo ""

echo "--- Config ---"
echo "Root:"
ls -la /root/.labnas/config.json 2>/dev/null || echo "  No existe en /root"
cat /root/.labnas/config.json 2>/dev/null | head -5
echo ""
echo "Usuario actual ($USER):"
ls -la ~/.labnas/config.json 2>/dev/null || echo "  No existe en ~"
echo ""

echo "--- Firewall ---"
sudo iptables -L -n 2>/dev/null | head -10 || echo "Sin acceso a iptables"
sudo ufw status 2>/dev/null || echo "ufw no instalado"
echo ""

echo "--- Binario ---"
which labnas-backend 2>/dev/null || echo "No esta en PATH"
find / -name "labnas-backend" -type f 2>/dev/null | head -5
echo ""

echo "--- Disco ---"
df -h / | tail -1
echo ""

echo "=== FIN DIAGNOSTICO ==="
