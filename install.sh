#!/bin/bash
# Instalador de LabNAS
#
#   curl -fsSL https://github.com/Debaq/labnas/releases/latest/download/install.sh | sudo bash
#   sudo bash install.sh [opciones]
#
# Opciones:
#   --user USUARIO    usuario que corre el servicio (por defecto quien usa sudo; nunca root)
#   --dir RUTA        carpeta de instalacion (por defecto /opt/labnas)
#   --version vX.Y.Z  version a instalar (por defecto la ultima)
#   --deps            instalar dependencias opcionales (rsync, restic, ffmpeg, mpv, yt-dlp, cups, sane, smartmontools)
#   --smart           configurar la lectura de salud de discos (setup-smart.sh)
#   --uninstall       quitar el servicio y la carpeta de instalacion (los datos quedan)
#   --purge           con --uninstall: borrar tambien los datos (~/.labnas del usuario; pide confirmacion)
#   --tarball ARCHIVO instalar desde un tarball local (sin descargar; requiere ARCHIVO.sha256)
#   --no-systemd      no crear el servicio (solo instalar archivos)
#   --no-hardening    servicio sin las restricciones de systemd (si algo de la terminal lo necesita)
#   -h, --help        esta ayuda

set -euo pipefail

REPO="Debaq/labnas"
SERVICE="labnas"
UNIT_FILE="/etc/systemd/system/${SERVICE}.service"
HTTP_PORT=3001
HTTPS_PORT=3443

SERVICE_USER="${SUDO_USER:-}"
INSTALL_DIR="/opt/labnas"
VERSION=""
WITH_DEPS=0
HARDENING=1
WITH_SMART=0
UNINSTALL=0
PURGE=0
TARBALL=""
USE_SYSTEMD=1
ASSUME_YES=0

if [ -t 1 ]; then
    RED=$'\033[0;31m' GREEN=$'\033[0;32m' YELLOW=$'\033[1;33m' CYAN=$'\033[0;36m' BOLD=$'\033[1m' NC=$'\033[0m'
else
    RED="" GREEN="" YELLOW="" CYAN="" BOLD="" NC=""
fi

info() { echo "${CYAN}==>${NC} $*"; }
ok() { echo "${GREEN}✓${NC} $*"; }
warn() { echo "${YELLOW}!${NC} $*"; }
die() { echo "${RED}✗ $*${NC}" >&2; exit 1; }

usage() {
    cat << 'EOF'
Instalador de LabNAS

  curl -fsSL https://github.com/Debaq/labnas/releases/latest/download/install.sh | sudo bash
  sudo bash install.sh [opciones]

  --user USUARIO    usuario que corre el servicio (por defecto quien usa sudo; nunca root)
  --dir RUTA        carpeta de instalacion (por defecto /opt/labnas)
  --version vX.Y.Z  version a instalar (por defecto la ultima)
  --deps            instalar dependencias opcionales (rsync, restic, ffmpeg, mpv, yt-dlp, cups, sane, smartmontools)
  --smart           configurar la lectura de salud de discos (setup-smart.sh)
  --uninstall       quitar el servicio y la carpeta de instalacion (los datos quedan)
  --purge           con --uninstall: borrar tambien los datos (~/.labnas del usuario; pide confirmacion)
  --tarball ARCHIVO instalar desde un tarball local (requiere ARCHIVO.sha256)
  --no-systemd      no crear el servicio (solo instalar archivos)
  --no-hardening    servicio sin las restricciones de systemd (si algo de la terminal lo necesita)
  --yes             no preguntar (para --purge sin terminal)
  -h, --help        esta ayuda
EOF
    exit 0
}

while [ $# -gt 0 ]; do
    case "$1" in
        --user) SERVICE_USER="${2:?falta el usuario}"; shift 2 ;;
        --dir) INSTALL_DIR="${2:?falta la ruta}"; shift 2 ;;
        --version) VERSION="${2:?falta la version}"; shift 2 ;;
        --deps) WITH_DEPS=1; shift ;;
        --smart) WITH_SMART=1; shift ;;
        --uninstall) UNINSTALL=1; shift ;;
        --purge) PURGE=1; shift ;;
        --tarball) TARBALL="${2:?falta el archivo}"; shift 2 ;;
        --no-systemd) USE_SYSTEMD=0; shift ;;
        --no-hardening) HARDENING=0; shift ;;
        --yes) ASSUME_YES=1; shift ;;
        -h | --help) usage ;;
        *) die "Opcion desconocida: $1 (usa --help)" ;;
    esac
done

# ─── Comprobaciones ───

if [ "$USE_SYSTEMD" -eq 1 ] && [ "$(id -u)" -ne 0 ]; then
    die "Ejecuta con sudo: sudo bash install.sh (o --no-systemd para solo copiar archivos)"
fi
if [ -z "$SERVICE_USER" ]; then
    die "No se pudo saber que usuario correra LabNAS. Usa --user <usuario>"
fi
if [ "$SERVICE_USER" = "root" ]; then
    die "LabNAS no debe correr como root. Usa --user <usuario-normal> (el del escritorio del laboratorio)"
fi
id "$SERVICE_USER" > /dev/null 2>&1 || die "El usuario '$SERVICE_USER' no existe"
USER_HOME="$(getent passwd "$SERVICE_USER" | cut -d: -f6)"
USER_GROUP="$(id -gn "$SERVICE_USER")"

run_as_root() {
    if [ "$(id -u)" -eq 0 ]; then "$@"; else "$@" 2> /dev/null || true; fi
}

# ─── Desinstalar ───

if [ "$UNINSTALL" -eq 1 ]; then
    info "Desinstalando LabNAS"
    if [ -f "$UNIT_FILE" ]; then
        systemctl disable --now "$SERVICE" 2> /dev/null || true
        rm -f "$UNIT_FILE"
        systemctl daemon-reload
        ok "Servicio quitado"
    fi
    if [ -d "$INSTALL_DIR" ] && [ -x "$INSTALL_DIR/labnas-backend" ]; then
        rm -rf "$INSTALL_DIR"
        ok "Quitado $INSTALL_DIR"
    fi
    if [ "$PURGE" -eq 1 ]; then
        # Irreversible: base de datos, secret.key, respaldos de la base
        if [ "$ASSUME_YES" -ne 1 ]; then
            [ -r /dev/tty ] || die "--purge sin terminal: agrega --yes para confirmar"
            printf '%s' "${RED}Se borraran ${USER_HOME}/.labnas (base de datos y secret.key). Escribe BORRAR para confirmar: ${NC}"
            read -r answer < /dev/tty
            [ "$answer" = "BORRAR" ] || die "Cancelado"
        fi
        rm -rf "${USER_HOME:?}/.labnas"
        ok "Datos borrados (${USER_HOME}/.labnas)"
    else
        warn "Los datos siguen en ${USER_HOME}/.labnas (base de datos y secret.key). Para borrarlos: --uninstall --purge"
    fi
    exit 0
fi

# ─── Arquitectura y version ───

case "$(uname -m)" in
    x86_64 | amd64) ARCH="x86_64" ;;
    aarch64 | arm64) ARCH="aarch64" ;;
    armv7l | armv7*) ARCH="armv7" ;;
    *) die "Arquitectura no soportada: $(uname -m)" ;;
esac

for cmd in tar sha256sum; do
    command -v "$cmd" > /dev/null || die "Falta el comando '$cmd'"
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Clave publica minisign de los releases (el workflow de release la escribe aqui antes de firmar
# este script). Con ella y el comando minisign, el tarball se verifica antes de ejecutar nada.
MINISIGN_PUBKEY="${LABNAS_MINISIGN_PUBKEY:-__MINISIGN_PUBKEY__}"
case "$MINISIGN_PUBKEY" in __*) MINISIGN_PUBKEY="" ;; esac

if [ -n "$TARBALL" ]; then
    [ -f "$TARBALL" ] || die "No existe $TARBALL"
    [ -f "$TARBALL.sha256" ] || die "Falta $TARBALL.sha256 (checksum obligatorio)"
    cp "$TARBALL" "$WORK/labnas.tar.gz"
    EXPECTED="$(cut -d' ' -f1 < "$TARBALL.sha256")"
    info "Instalando desde $TARBALL"
else
    command -v curl > /dev/null || die "Falta curl"
    if [ -z "$VERSION" ]; then
        VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep -m1 '"tag_name"' | cut -d'"' -f4)"
        [ -n "$VERSION" ] || die "No se pudo consultar la ultima version en GitHub"
    fi
    case "$VERSION" in v*) ;; *) VERSION="v$VERSION" ;; esac
    ASSET="labnas-${VERSION}-linux-${ARCH}.tar.gz"
    BASE="https://github.com/${REPO}/releases/download/${VERSION}"
    info "Descargando LabNAS ${VERSION} (${ARCH})"
    curl -fL --progress-bar -o "$WORK/labnas.tar.gz" "$BASE/$ASSET" || die "No se pudo descargar $ASSET"
    EXPECTED="$(curl -fsSL "$BASE/$ASSET.sha256" | cut -d' ' -f1)" || die "No se pudo descargar el checksum"
    if [ -n "$MINISIGN_PUBKEY" ]; then
        curl -fsSL -o "$WORK/labnas.tar.gz.minisig" "$BASE/$ASSET.minisig" || die "El release no esta firmado (falta $ASSET.minisig)"
    fi
fi

ACTUAL="$(sha256sum "$WORK/labnas.tar.gz" | cut -d' ' -f1)"
if [ -z "$EXPECTED" ] || [ "$ACTUAL" != "$EXPECTED" ]; then
    die "Checksum SHA-256 no coincide: descarga corrupta o alterada"
fi
ok "Checksum verificado"

# Firma: prueba que el paquete lo publico el dueño de la clave (no solo que llego entero)
if [ -n "$MINISIGN_PUBKEY" ] && [ -z "$TARBALL" ]; then
    if command -v minisign > /dev/null; then
        minisign -Vq -P "$MINISIGN_PUBKEY" -m "$WORK/labnas.tar.gz" -x "$WORK/labnas.tar.gz.minisig" \
            || die "Firma minisign invalida: el paquete no es del publicador esperado"
        ok "Firma verificada"
    else
        warn "Instala 'minisign' para verificar la firma del release (solo se verifico el checksum)"
    fi
fi

tar xzf "$WORK/labnas.tar.gz" -C "$WORK"
[ -x "$WORK/labnas/labnas-backend" ] || die "El tarball no contiene labnas/labnas-backend"
NEW_VERSION="$("$WORK/labnas/labnas-backend" --version 2> /dev/null)" || die "El binario no se ejecuta en esta maquina (¿arquitectura equivocada?)"

# ─── Instalar archivos ───

if [ "$USE_SYSTEMD" -eq 1 ] && systemctl is-active --quiet "$SERVICE" 2> /dev/null; then
    info "Deteniendo el servicio actual"
    systemctl stop "$SERVICE"
fi

mkdir -p "$INSTALL_DIR"
# Reemplazar binario y web; conservar cualquier otro archivo (p.ej. .rollback del updater)
rm -rf "$INSTALL_DIR/dist"
cp -a "$WORK/labnas/." "$INSTALL_DIR/"
run_as_root chown -R "$SERVICE_USER:$USER_GROUP" "$INSTALL_DIR"
ok "LabNAS ${NEW_VERSION} instalado en ${INSTALL_DIR} (dueño: ${SERVICE_USER}, para que se auto-actualice)"

# ─── Dependencias opcionales ───

OPTIONAL="rsync restic ffmpeg mpv yt-dlp lp scanimage smartctl"
if [ "$WITH_DEPS" -eq 1 ]; then
    info "Instalando dependencias opcionales"
    if command -v pacman > /dev/null; then
        pacman -S --needed --noconfirm rsync restic ffmpeg mpv yt-dlp cups sane alsa-utils smartmontools
    elif command -v apt-get > /dev/null; then
        apt-get update -qq && apt-get install -y rsync restic ffmpeg mpv yt-dlp cups sane-utils alsa-utils smartmontools
    elif command -v dnf > /dev/null; then
        dnf install -y rsync restic ffmpeg mpv yt-dlp cups sane-backends alsa-utils smartmontools
    else
        warn "Gestor de paquetes no reconocido: instala a mano $OPTIONAL"
    fi
else
    MISSING=""
    for c in $OPTIONAL; do command -v "$c" > /dev/null || MISSING="$MISSING $c"; done
    [ -z "$MISSING" ] || warn "Opcionales no instalados:${MISSING} (respaldos, timelapse, musica, impresion, escaner, SMART). Reintenta con --deps"
fi

# ─── Servicio systemd ───

# Restricciones que no rompen nada de LabNAS (terminal con sudo, SMART, musica, CUPS, respaldos).
# Se omiten a proposito NoNewPrivileges y CapabilityBoundingSet (romperian sudo en la terminal y
# el lector de SMART) y ProtectSystem/ProtectHome (la terminal del admin debe poder editar el sistema).
hardening_block() {
    [ "$HARDENING" -eq 1 ] || return 0
    cat << 'EOF'
# Endurecimiento (desactivar con install.sh --no-hardening)
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectKernelLogs=true
ProtectControlGroups=true
ProtectClock=true
ProtectHostname=true
RestrictRealtime=true
RestrictSUIDSGID=true
LockPersonality=true
RestrictNamespaces=true
SystemCallArchitectures=native
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK AF_PACKET
EOF
}

write_unit() {
    cat << EOF
[Unit]
Description=LabNAS - NAS de Laboratorio
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${SERVICE_USER}
ExecStart=${INSTALL_DIR}/labnas-backend
WorkingDirectory=${INSTALL_DIR}
Restart=on-failure
RestartSec=5
# Solo ping (escaner de red) y puertos 80/443; nada mas de root
AmbientCapabilities=CAP_NET_RAW CAP_NET_BIND_SERVICE
$(hardening_block)

[Install]
WantedBy=multi-user.target
EOF
}

if [ "$USE_SYSTEMD" -eq 1 ]; then
    write_unit > "$UNIT_FILE"
    systemctl daemon-reload
    systemctl enable --now "$SERVICE" > /dev/null
    ok "Servicio '${SERVICE}' activo (usuario ${SERVICE_USER})"
else
    write_unit > "$INSTALL_DIR/labnas.service.example"
    warn "Sin systemd: unidad de ejemplo en $INSTALL_DIR/labnas.service.example"
fi

# ─── Firewall ───

if [ "$USE_SYSTEMD" -eq 1 ] && command -v ufw > /dev/null && ufw status 2> /dev/null | grep -q "Status: active"; then
    for p in "$HTTP_PORT" "$HTTPS_PORT" 80 443 8631; do ufw allow "$p/tcp" > /dev/null; done
    ok "Firewall (ufw): abiertos ${HTTP_PORT}, ${HTTPS_PORT}, 80, 443 y 8631 (impresora de red)"
fi

# ─── SMART ───

if [ "$WITH_SMART" -eq 1 ]; then
    bash "$INSTALL_DIR/setup-smart.sh" "$SERVICE_USER"
fi

# ─── Listo ───

if [ "$USE_SYSTEMD" -eq 1 ]; then
    info "Esperando a que LabNAS responda"
    for _ in $(seq 1 30); do
        if curl -fs "http://localhost:${HTTP_PORT}/api/health" > /dev/null 2>&1; then break; fi
        sleep 1
    done
    curl -fs "http://localhost:${HTTP_PORT}/api/health" > /dev/null 2>&1 \
        || warn "LabNAS no respondio aun; revisa: journalctl -u ${SERVICE} -n 50"
fi

IP="$(hostname -I 2> /dev/null | awk '{print $1}')"
echo
echo "${BOLD}${GREEN}LabNAS ${NEW_VERSION} listo${NC}"
echo "  Web:    http://${IP:-localhost}:${HTTP_PORT}"
echo "  HTTPS:  https://${IP:-localhost}:${HTTPS_PORT} (certificado autofirmado: instalalo desde Configuracion > Sistema > HTTPS)"
echo
echo "  • La primera cuenta que se registre sera la de administrador: hazlo ahora"
echo "  • Guarda una copia de ${USER_HOME}/.labnas/secret.key fuera del NAS (cifra los secretos de la base)"
echo "  • Logs: journalctl -u ${SERVICE} -f   ·   Desinstalar: sudo bash install.sh --uninstall"
echo "  • Visor de escritorio: descarga labnas-viewer del release y ejecuta su install.sh"
