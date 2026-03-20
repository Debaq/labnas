#!/bin/bash
echo "=== DIAGNOSTICO 2 ==="

echo "--- UFW status ---"
sudo ufw status verbose
echo ""

echo "--- Regla 3001 ---"
sudo ufw status numbered | grep 3001
echo ""

echo "--- Test local ---"
curl -s -w "\nHTTP: %{http_code}\n" http://localhost:3001/api/health 2>&1 || echo "No responde en localhost"
echo ""

echo "--- Test IP ---"
curl -s -w "\nHTTP: %{http_code}\n" http://192.168.50.114:3001/api/health 2>&1 || echo "No responde en IP"
echo ""

echo "--- Puerto abierto? ---"
ss -tlnp | grep 3001
echo ""

echo "--- Ping a si mismo ---"
ping -c 1 192.168.50.114 2>&1 | tail -1
echo ""

echo "=== FIN ==="
