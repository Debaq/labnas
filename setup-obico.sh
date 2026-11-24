#!/bin/bash
# setup-obico.sh - Obico (deteccion de impresion fallida) en el mismo servidor que LabNAS
# Uso: sudo bash setup-obico.sh            instala o actualiza (imagen publicada de Obico)
#      sudo bash setup-obico.sh --build    construye la imagen desde el codigo de Obico
#      sudo bash setup-obico.sh --quitar   detiene y borra el contenedor
#
# Levanta el ml_api de Obico (AGPL, por eso va aparte y no dentro de LabNAS) en Docker,
# escuchando solo en 127.0.0.1:3333: no queda expuesto a la red y no necesita token.
# Usa la red del servidor (--network host) para bajar los cuadros desde LabNAS en
# http://127.0.0.1:3001. Despues, en LabNAS: Camaras > Deteccion > regla «Impresion fallida».
# Necesita Docker (o Podman con el alias docker), x86_64 o arm64, y ~1 GB de disco.

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

NAME="labnas-obico"
IMAGE="${OBICO_IMAGE:-ghcr.io/thespaghettidetective/obico-ml-api-cpu:release}"
BUILT_IMAGE="labnas-obico-ml"
PORT=3333

if [ "$(id -u)" -ne 0 ]; then
    echo -e "${RED}Ejecuta con sudo: sudo bash setup-obico.sh${NC}"
    exit 1
fi
if ! command -v docker > /dev/null 2>&1; then
    echo -e "${RED}Falta Docker. Instalalo (p.ej. 'apt install docker.io' o 'pacman -S docker') y vuelve a correr este script.${NC}"
    exit 1
fi

case "${1:-}" in
    --quitar)
        docker rm -f "$NAME" > /dev/null 2>&1 || true
        echo -e "${GREEN}Contenedor $NAME quitado.${NC} La imagen sigue en Docker; para borrarla: docker rmi $IMAGE"
        exit 0
        ;;
    --build)
        TMP="$(mktemp -d)"
        trap 'rm -rf "$TMP"' EXIT
        echo "=== Construyendo el ml_api de Obico desde su codigo (tarda y baja ~1 GB) ==="
        git clone --depth 1 --branch release https://github.com/TheSpaghettiDetective/obico-server.git "$TMP/obico"
        docker build -t "$BUILT_IMAGE" "$TMP/obico/ml_api"
        IMAGE="$BUILT_IMAGE"
        ;;
    "")
        echo "=== Bajando la imagen de Obico ($IMAGE) ==="
        if ! docker pull "$IMAGE"; then
            echo -e "${RED}No se pudo bajar la imagen.${NC} Prueba construyendola: ${YELLOW}sudo bash setup-obico.sh --build${NC}"
            exit 1
        fi
        ;;
    *)
        echo -e "${RED}Opcion desconocida: $1${NC} (usa --build o --quitar)"
        exit 1
        ;;
esac

docker rm -f "$NAME" > /dev/null 2>&1 || true
docker run -d --name "$NAME" --restart unless-stopped --network host "$IMAGE" \
    gunicorn --bind "127.0.0.1:$PORT" --workers 1 --timeout 120 wsgi > /dev/null

echo -n "Esperando a que cargue el modelo (puede tardar un par de minutos)"
for _ in $(seq 1 90); do
    if [ "$(curl -s --max-time 2 "http://127.0.0.1:$PORT/hc/" || true)" = "ok" ]; then
        echo
        echo -e "${GREEN}Listo.${NC} Obico responde en http://127.0.0.1:$PORT"
        echo -e "En LabNAS: ${YELLOW}Camaras > Deteccion con IA > Nueva regla > Impresion fallida${NC}"
        echo "Para quitarlo: sudo bash setup-obico.sh --quitar"
        exit 0
    fi
    echo -n "."
    sleep 2
done
echo
echo -e "${RED}Obico no respondio a tiempo.${NC} Revisa: docker logs $NAME"
exit 1
