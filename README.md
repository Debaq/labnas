<p align="center">
  <img src="frontend/public/favicon.svg" width="80" alt="LabNAS" />
</p>

<h1 align="center">LabNAS</h1>

<p align="center">
  <strong>Servidor de laboratorio self-hosted: NAS + escáner de red + impresoras 3D + streaming + IA de correo + tareas + inventario + sensores IoT + bot de Telegram — un solo binario, una sola web UI.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-2.8.0-success?style=flat-square" alt="Version" />
  <img src="https://img.shields.io/badge/Rust-Axum_0.8-orange?style=flat-square" alt="Rust" />
  <img src="https://img.shields.io/badge/React-19-blue?style=flat-square" alt="React" />
  <img src="https://img.shields.io/badge/TypeScript-5.9-blue?style=flat-square" alt="TypeScript" />
  <img src="https://img.shields.io/badge/SQLite-WAL-003B57?style=flat-square" alt="SQLite" />
  <img src="https://img.shields.io/badge/Telegram-40%2B_cmds-26A5E4?style=flat-square" alt="Telegram" />
  <img src="https://img.shields.io/badge/AI-Groq_Llama_3.3-orange?style=flat-square" alt="Groq" />
  <img src="https://img.shields.io/badge/Tested_on-Arch_Linux-1793D1?style=flat-square" alt="Arch" />
  <img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="License" />
</p>

---

## Qué es LabNAS

Plataforma self-hosted pensada para laboratorios (docencia, investigación, makerspaces). Integra gestión de archivos, terminal web, bot de Telegram bidireccional, correo con IA, tareas/proyectos, impresoras 3D multi-protocolo, streaming de música/video, impresión CUPS, inventario, reportes académicos, portafolio, sensores IoT y escaneo de red — todo en una única interfaz web moderna.

Se distribuye como **binario estático** (musl) con la web UI embebida. Sin Docker, sin dependencias de runtime obligatorias. Persistencia en **SQLite** (WAL).

## Inicio rápido

```bash
# Descargar último release
TAG=$(curl -s https://api.github.com/repos/Debaq/labnas/releases/latest | grep tag_name | cut -d'"' -f4)
curl -sL "https://github.com/Debaq/labnas/releases/download/${TAG}/labnas-${TAG}-linux-x86_64.tar.gz" | tar xz

# Ejecutar (como usuario normal, NO con sudo)
cd labnas
./labnas-backend
```

Abre `http://localhost:3001` — la primera cuenta creada se convierte en admin. Las siguientes quedan **pendientes** hasta que un admin las apruebe.

## Sistema de módulos activables

Desde **v2.7.0**, las funcionalidades son **módulos independientes** que se activan/desactivan desde el panel de Administración. Cada rol de usuario puede tener acceso distinto a cada módulo. Esto permite desplegar LabNAS como solo-NAS, solo-gestor-impresoras, lab completo, etc.

Módulos disponibles:

| Módulo | Descripción |
|--------|-------------|
| Archivos | Explorador de archivos, compartir, descargar URL |
| Música/Video | Reproductor YouTube + radio Last.fm |
| Correo | IMAP/POP3 con clasificación IA |
| Impresoras 3D | Moonraker / OctoPrint / Creality / FlashForge |
| Impresión | CUPS + wizard doble cara manual |
| Red | Escáner ICMP + detección de dispositivos |
| Tareas/Proyectos | Kanban + calendario + notificaciones |
| Inventario | Stock de insumos con movimientos y costos |
| Portafolio académico | Evidencias de trabajo por usuario |
| Reportes | Generación de informes por periodo |
| Sensores IoT | Dashboard de sensores ESP32/MQTT/HTTP |
| Notas | Markdown colaborativo |
| Servicios | Links a otros servicios del lab |

## Funcionalidades

### Archivos
- Explorador completo: subir, descargar, borrar, crear directorios
- Accesos directos a rutas comunes
- Compartir archivos con links temporales (24h)
- Descargar desde URL directo al NAS
- Imprimir archivos directo a CUPS
- Dedup por subvolúmenes BTRFS

### Música y Video
- Búsqueda y reproducción de YouTube vía `yt-dlp` + `mpv`
- **Dos modos**: parlantes del NAS o streaming al navegador
- **Radio Last.fm**: genera playlist automática con canciones similares
- **Lucky Play**: una canción similar al azar
- **Playlists persistentes**: crear, editar, reproducir; editor a pantalla completa
- Reproducir directo desde resultados de búsqueda
- Controles completos: play, pause, prev, next, stop, shuffle, repeat (off/all/one)
- Cola: reordenar, play next, eliminar, limpiar
- Volumen con popup vertical compacto
- Recomendaciones IA multi-semilla diversificadas por artista (YouTube Mix)
- **Salida de video**: fullscreen en display conectado con multi-monitor (X11)
- Panel lateral persistente en todas las páginas
- Control por Telegram: `/play`, `/next`, `/stop`, `/pause`, `/mix`, `/vol`

### Correo (IMAP y POP3)
- IMAP y POP3 con selector de protocolo por cuenta
- Compatible con Gmail (IMAP), Outlook (POP3 + app password), y cualquier servidor estándar
- Clasificación IA vía Groq LLM: urgente, tarea, informativo, spam
- Resumen y acciones sugeridas por correo
- Filtros por remitente: prioridad, normal, silencioso, ignorar
- Convertir correo a tarea en un click
- Chequeo en segundo plano cada 5 min + alertas Telegram para urgentes
- Comandos Telegram: `/correos`, `/leer UID`, `/correo2tarea UID`

### Impresoras 3D
- **4 protocolos**: Moonraker (Klipper), OctoPrint, Creality stock (WebSocket), FlashForge stock (TCP)
- Control unificado de flotas mixtas en un solo dashboard
- Link directo a UI web de cada impresora (Fluidd, Mainsail, OctoPrint, Creality)
- Temperaturas en tiempo real con barras (hotend + cama)
- Control de job: iniciar, pausar, reanudar, cancelar
- Jog pad por ejes (0.1, 1, 10, 100mm)
- Home ejes + G-code manual
- Gestión de archivos en impresora: listar, subir, imprimir, borrar
- Snapshots de webcam en UI y vía Telegram (`/camara`)
- Drag & drop de `.gcode`
- Auto-monitor: notificación Telegram al finalizar o al error
- Autodetección de impresoras en red (los 4 protocolos)
- Secciones/agrupaciones de impresoras por área del lab

### Impresión de documentos
- Integración CUPS con opciones dinámicas por impresora
- Tamaño, calidad, color, dúplex
- Cola con cancelación
- Drag & drop
- **Wizard doble cara manual**: para impresoras sin dúplex automático, guía al usuario paso a paso con lotes multi-impresora, intercalado correcto y configuración avanzada por trabajo
- Conteo real de páginas PDF y límite de subida configurable

### Tareas y Proyectos
- Crear tareas con asignación (`@user`, `@all`), fecha límite, proyecto
- Chips clickeables para asignar — lista todos los usuarios del sistema
- Notificación Telegram al crear (asignados) y completar (creador)
- Alertas de vencimiento: "vence hoy" y "vencida" en ciclo de 12h
- `@all` sólo para operadores y admins (no observadores); `@user` funciona para cualquier rol
- Flujo de confirmación: accept/reject explícito
- Recordatorios insistentes Telegram (default 8 min, configurable)
- Progreso de proyecto con barras visuales
- Calendario con eventos, invitaciones (RSVP), recordatorios
- **Recurrencia por días específicos** en eventos (v2.6.9)
- Comandos Telegram: `/tarea`, `/tareas`, `/hecho`, `/confirmar`, `/rechazar`, `/proyecto`, `/eventos`

### Inventario
- Stock de insumos del lab (filamentos, reactivos, material)
- Movimientos de entrada/salida con usuario y timestamp
- Costos por usuario y por proyecto
- Alerta de stock bajo

### Reportes
- Generación de informes por periodo (día, semana, mes, rango custom)
- Incluye actividad del sistema, impresiones 3D, impresiones papel, uso de recursos
- Exportables desde la UI

### Portafolio académico
- Evidencias de trabajo por usuario (archivos, notas, piezas impresas)
- Vista pública opcional por usuario
- Útil para seguimiento docente

### Sensores IoT
- Dashboard de sensores con ingesta HTTP/MQTT
- Compatibilidad con ESP32 y similares
- Gráficos de series temporales
- Alertas por umbrales
- **Token por dispositivo** (Sensores > Seguridad de sensores, admin): el sensor lo envía en el header `X-Sensor-Token` o en el campo `token` del JSON de `POST /api/sensors/data`. En la base se guarda solo su hash. Con "exigir token" activo se rechazan lecturas sin token válido y dispositivos desconocidos; sin él (por defecto) los sensores sin token siguen funcionando

  ```json
  { "mac": "AA:BB:CC:DD:EE:FF", "token": "<token>", "readings": [{ "key": "temp", "val": 21.5 }] }
  ```

### Escáner de red
- Scan ICMP con descubrimiento automático
- **Wake-on-LAN**: los dispositivos etiquetados siguen listados aunque estén apagados (con su última IP) y operadores/admins pueden encenderlos con un clic
- Detección de MAC y fabricante
- Tracking de dispositivos conocidos vs desconocidos con iconos (24 tipos)
- Alertas Telegram ante nuevos dispositivos desconocidos
- Etiquetas e iconos personalizables
- Escaneo periódico en segundo plano (cada 5 min)

### Bot de Telegram (40+ comandos)

| Categoría | Comandos |
|-----------|----------|
| Sistema | `/estado` `/discos` `/ram` `/cpu` `/uptime` `/red` `/ip` `/actividad` |
| Impresoras 3D | `/impresoras` `/temp` `/camara` `/imprimir` `/pausar` `/cancelar3d` |
| Terminal | `/cmd <comando>` — shell remoto interactivo (corre como el usuario del servicio, nunca root) |
| Tareas | `/tarea` `/tareas` `/hecho` `/confirmar` `/rechazar` `/avance` |
| Proyectos | `/proyecto` `/proyectos` |
| Calendario | `/evento` `/eventos` `/aceptar` `/declinar` |
| Correo | `/correos` `/leer` `/correo2tarea` |
| Música | `/musica` `/play` `/next` `/stop` `/pause` `/mix` `/vol` |
| Usuario | `/vincular` `/mirol` `/horario` `/ayuda` |

### Terminal web
- PTY completa sobre WebSocket (xterm.js)
- Corre como el usuario logueado (no root) por seguridad
- Resize, 256 colores, programas interactivos
- Terminal interactiva también por Telegram (`/cmd`)

### Notas
- Editor Markdown con preview en vivo split
- Compartir con `@user` — destinatarios reciben notificación Telegram
- Flag public: visible a todos los usuarios
- Colaborativas: todos ven y editan

### Servicios del laboratorio
- Registrar servicios en otros puertos (CVAT, Label Studio, CUPS, Jupyter, etc.)
- Cards de acceso rápido en dashboard
- Administración desde Configuración
- Muestran IP real del host

### Notificaciones
- Bot Telegram con long polling
- Reportes diarios programados (estado sistema + actividad) con horario por usuario
- Alertas en tiempo real: dispositivos nuevos, correos urgentes, impresiones 3D terminadas/errores, asignación/completar tareas, notas compartidas
- Filtrado por rol

### Sistema y Administración
- Dashboard en tiempo real: CPU, RAM, disco, hosts, impresoras, uptime
- **Configuración con pestañas** (v2.8.0): separa cada área en tabs
- **Panel Administración** como tab: gestión de usuarios, roles, módulos activos, permisos por rol
- 4 temas: Dracula (default), Light, Nord, Solarized + modo auto
- Branding: nombre del lab, logo, color de acento, institución, misión/visión
- Título de la web muestra `[NombreLab] - LabNAS`
- **Auto-actualización** desde GitHub Releases — chequea cada 6h, update en un click
- **Reinstalar versión actual** (útil ante archivos corruptos)
- Servicio systemd con auto-restart
- mDNS/Bonjour (`labnas.local`)
- Apagado desde la UI (sólo admin)

### Autenticación y Seguridad
- Multiusuario con 4 roles: Admin, Operador, Observador, Pendiente
- Hash bcrypt + tokens de sesión (24h)
- Permisos granulares por rol (terminal, impresión, archivos, módulos)
- Middleware de permisos a nivel de ruta
- Rate limiting en login (2s de delay por intento fallido)
- Token del bot nunca se expone en respuestas de la API
- Vinculación Telegram ↔ cuenta web con código de 8 chars (5 min expiry)

## Stack técnico

| Componente | Tecnología |
|------------|-----------|
| Backend | Rust, Axum 0.8, Tokio |
| Frontend | React 19, TypeScript 5.9, Vite 8, TailwindCSS 4 |
| Persistencia | SQLite (WAL) |
| Terminal | portable-pty + xterm.js 6 (WebSocket) |
| Música/Video | yt-dlp + mpv (X11 multi-monitor), Last.fm API |
| Correo | IMAP (nativo), POP3 (TLS custom) |
| IA | Groq API (Llama 3.3 70B Versatile) |
| Impresoras 3D | Moonraker, OctoPrint, Creality WS, FlashForge TCP |
| Impresión | CUPS CLI (lp, lpstat, cancel) |
| Red | ICMP ping (surge-ping), DNS lookup |
| mDNS | mdns-sd |
| Notificaciones | Telegram Bot API (long polling) |
| Auth | bcrypt + UUID session tokens |
| Build | Binario estático vía musl + OpenSSL vendored |

## Requisitos

**Probado en Arch Linux.** Debería funcionar en cualquier distro Linux moderna.

- **SO**: Linux x86_64
- **Dependencias opcionales** (para funcionalidad completa):

| Dependencia | Para qué | Instalar (Arch) |
|-------------|----------|-----------------|
| `mpv` | Reproducción música/video | `pacman -S mpv` |
| `yt-dlp` | Búsqueda y streaming YouTube | `pacman -S yt-dlp` |
| `alsa-utils` | Control de volumen | `pacman -S alsa-utils` |
| `cups` | Impresión de documentos | `pacman -S cups` |
| `avahi` + `nss-mdns` | Resolución mDNS | `pacman -S avahi nss-mdns` |

```bash
# Instalar todas las deps opcionales de una (Arch)
sudo pacman -S mpv yt-dlp alsa-utils cups avahi nss-mdns
```

## Instalación

### Binario pre-compilado (recomendado)

```bash
TAG=$(curl -s https://api.github.com/repos/Debaq/labnas/releases/latest | grep tag_name | cut -d'"' -f4)
curl -sL "https://github.com/Debaq/labnas/releases/download/${TAG}/labnas-${TAG}-linux-x86_64.tar.gz" | tar xz

sudo mv labnas /opt/labnas
sudo chown -R "$USER": /opt/labnas   # el servicio corre como tu usuario y se auto-actualiza
/opt/labnas/labnas-backend
```

### Servicio systemd (producción)

LabNAS **no debe correr como root**. El servicio corre como un usuario normal (el del escritorio del laboratorio, para que música, terminal y archivos del home funcionen) y solo recibe dos capacidades: `CAP_NET_RAW` (ping del escáner de red) y `CAP_NET_BIND_SERVICE` (puerto 80).

**Configuración > Sistema > Inicio automático** muestra el comando listo para copiar. O a mano (reemplaza `TU_USUARIO`):

```bash
sudo tee /etc/systemd/system/labnas.service > /dev/null << 'EOF'
[Unit]
Description=LabNAS - NAS de Laboratorio
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=TU_USUARIO
ExecStart=/opt/labnas/labnas-backend
WorkingDirectory=/opt/labnas
Restart=on-failure
RestartSec=5
AmbientCapabilities=CAP_NET_RAW CAP_NET_BIND_SERVICE

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now labnas
```

La actualización desde la web reemplaza el binario y se re-ejecuta sola (no necesita `systemctl` ni root).

### Carpetas accesibles (raíces de almacenamiento)

El explorador, compartir, descargar URL e imprimir archivo solo acceden a las **raíces de almacenamiento**. Por defecto: el home del usuario del servicio, `/media`, `/mnt` y `/run/media`. Un admin las cambia en **Configuración > Sistema > Carpetas accesibles**. El directorio de datos (`~/.labnas`, con la base de datos) nunca es accesible.

**Permisos por carpeta**: en cada carpeta se elige quién **lee** y quién **escribe** (escribir incluye leer): todos, los usuarios con permiso de escritura, un rol (operador / observador) o usuarios puntuales. El admin siempre tiene acceso. Por defecto lee cualquiera y escriben los usuarios con permiso de escritura. Un usuario solo ve en el explorador las carpetas que puede leer, y en la papelera solo lo que puede restaurar.

### mDNS

Activar mDNS desde **Configuración > mDNS** en la web.

Para que los clientes resuelvan `labnas.local`:

```bash
# Helper incluido
sudo bash /opt/labnas/setup-mdns.sh

# O manual (Arch):
sudo pacman -S avahi nss-mdns
sudo systemctl enable --now avahi-daemon
# /etc/nsswitch.conf debe tener: hosts: ... mdns4_minimal [NOTFOUND=return] ... dns
```

**Clientes Windows**: instalar [Bonjour Print Services](https://support.apple.com/kb/DL999) o abrir UDP 5353.

### Build desde código

```bash
git clone https://github.com/Debaq/labnas.git
cd labnas

# Menú interactivo
./labnas.sh

# O directo:
./labnas.sh build    # Binario producción + frontend
./labnas.sh run      # Correr build producción
./labnas.sh dev      # Dev con hot reload
```

**Requisitos de build**: toolchain Rust (stable), Node.js 20+, npm.

Script `version.sh` para bump de versión antes de commit release.

## Visor de escritorio (opcional)

`viewer/` contiene **labnas-viewer**, una ventana nativa (Rust + [wry](https://github.com/tauri-apps/wry), WebKitGTK del sistema) que abre la web UI sin depender de un navegador. Binario de ~800 KB.

```bash
# Requiere webkit2gtk-4.1 y gtk3 (Arch: sudo pacman -S webkit2gtk-4.1)

# Desde el release (x86_64 / aarch64)
TAG=$(curl -s https://api.github.com/repos/Debaq/labnas/releases/latest | grep tag_name | cut -d'"' -f4)
curl -sL "https://github.com/Debaq/labnas/releases/download/${TAG}/labnas-viewer-${TAG}-linux-x86_64.tar.gz" | tar xz
./labnas-viewer/install.sh   # instala en ~/.local (binario + .desktop + icono)

# O desde el repo (compila)
./viewer/install.sh
labnas-viewer                # abre http://localhost:3001
labnas-viewer http://192.168.1.10:3001
```

- URL: argumento CLI → `LABNAS_URL` → `~/.config/labnas-viewer/url` → `http://localhost:3001`
- Si el servidor aún no responde, muestra una pantalla de espera y carga sola cuando levanta
- Sesión persistente (localStorage/cookies en `~/.local/share/labnas-viewer`)
- Descargas a `~/Descargas` con notificación; enlaces externos se abren con `xdg-open`
- Atajos: `F5` recargar, `Ctrl+Q` salir

## Configuración

Config persiste en `~/.labnas/` (SQLite + assets). Override con `LABNAS_CONFIG`.

### Primera vez
1. Abrir `http://localhost:3001` (o `http://labnas.local:3001`)
2. Crear cuenta admin
3. Ir a **Configuración** (ahora con pestañas):
   - Token del bot Telegram
   - Branding (nombre, logo, colores, institución)
   - Impresoras 3D
   - Last.fm API key (Radio y Lucky Play)
   - Cuentas de correo
   - Servicios del lab (links a puertos)
   - mDNS
   - **Administración**: gestión usuarios, módulos, roles

### Bot Telegram
1. Crear bot con [@BotFather](https://t.me/botfather)
2. Pegar token en **Configuración > Notificaciones**
3. `/start` al bot
4. Vincular cuenta web: generar código en Configuración, luego `/vincular CODIGO` en Telegram

### Correo con IA
1. API key gratis en [console.groq.com](https://console.groq.com)
2. Admin pega la key en **Configuración > Correo > Groq API Key**
3. Cada usuario configura su cuenta:
   - **Gmail**: IMAP, `imap.gmail.com:993`, app password
   - **Outlook**: POP3, `outlook.office365.com:995`, app password

### Video en pantallas externas
Corriendo como servicio systemd, LabNAS:
1. Detecta usuario con sesión X activa
2. Otorga acceso X11 vía `xhost`
3. Detecta displays conectados por DRM

Elegir display desde el menú del panel de música (tres puntos) para fullscreen.

## Puertos

| Servicio | Puerto | Protocolo |
|----------|--------|-----------|
| Web UI + API | 3001 | HTTP |
| mDNS | 5353 | UDP multicast |

## Estructura del proyecto

```
labnas/
  backend/src/
    main.rs              # Router, server, tareas background
    state.rs             # Estado compartido (sesiones, terminales, música)
    config.rs            # Persistencia SQLite
    middleware.rs        # Permisos por rol
    handlers/
      auth.rs            # Login, registro, roles, vinculación
      files.rs           # Explorador + sharing
      music.rs           # Reproductor + YouTube + playlists
      email.rs           # IMAP/POP3 + IA
      network.rs         # Escáner + MAC
      system.rs          # Info, update, branding, servicios
      terminal.rs        # WebSocket PTY
      notifications.rs   # Bot Telegram (40+ cmds)
      tasks.rs           # Tareas, proyectos, calendario
      printers3d.rs      # Impresoras 3D
      printing.rs        # CUPS + wizard doble cara
      inventory.rs       # Inventario y movimientos
      portfolio.rs       # Portafolio académico
      reports.rs         # Reportes por periodo
      sensors.rs         # Sensores IoT
      modules.rs         # Activación de módulos
      extras.rs          # Temp links, download URL, notas
    models/              # Estructuras de datos
  frontend/src/
    pages/               # Páginas
    components/
      Layout.tsx         # Shell con sidebar
      MusicPanel.tsx     # Panel lateral persistente
    themes/              # 4 temas + auto
    auth/                # Contexto auth + hooks permisos
    api/                 # Cliente API tipado
  viewer/                # Visor de escritorio nativo (wry/WebKitGTK)
  labnas.sh              # Script interactivo dev/build/run
  version.sh             # Bump de versión
  setup-mdns.sh          # Helper mDNS
```

## Auto-actualización

LabNAS chequea GitHub cada 6h. Cuando hay update:
- Admin recibe notificación Telegram
- **Configuración** muestra botón "Actualizar"
- Un click descarga el tarball de su arquitectura y **verifica su SHA-256** (sin checksum no instala)
- Comprueba que el binario nuevo arranque en la máquina (`labnas-backend --version`), guarda la versión actual en `.rollback/`, reemplaza los archivos y se re-ejecuta
- **Rollback automático**: si la versión nueva cae 2 veces al arrancar, el siguiente arranque restaura la anterior. Si se mantiene viva 30 s, se da por buena
- **Rollback manual**: botón "Volver a vX" en Configuración (repetirlo vuelve a la nueva)
- También permite **reinstalar la versión actual** (archivos corruptos)

## Tiempo real

Cada pestaña mantiene un WebSocket (`/api/live`) con el servidor, que empuja los cambios en vez de que la web consulte cada pocos segundos:

- **Impresoras 3D**: el servidor consulta todas las impresoras en paralelo una sola vez y reparte el estado a todas las pestañas (cada 5 s solo si alguien está mirando esa página; si no, cada 30 s para detectar fin de impresión)
- **Música**, **sensores** y **respaldos** se actualizan al instante
- **Notificaciones** en la web y **nativas en el visor de escritorio**: impresión terminada o con error, alerta de sensor, respaldo fallido, usuario pendiente de aprobación, actualización disponible
- Al aprobar una cuenta pendiente, la sesión del usuario se actualiza sola

Cada conexión recibe solo lo de su rol y de módulos activos. El WebSocket se abre con un ticket de un solo uso (30 s), así el token de sesión nunca viaja en una URL (la terminal web usa el mismo mecanismo). Si la conexión se cae, cada vista vuelve a consultar periódicamente hasta reconectar.

## Vista previa

Click en un archivo del explorador abre la vista previa: imágenes, video y audio (con adelantar/retroceder), PDF y texto/código (primeros 512 KB). Se sirven con un **link temporal de un solo archivo** (30 min), no con el token de sesión. HTML/XML/JS se muestran como texto y los SVG sin permitir scripts, para que un archivo subido no pueda ejecutar código en LabNAS. Las descargas usan el mismo mecanismo (en streaming, sin cargar el archivo en memoria del navegador).

## WebDAV (montar como unidad)

Desactivado por defecto; se activa en **Configuración > Sistema > Carpetas accesibles**. Dirección: `http://<servidor>:3001/dav/`, con el mismo usuario y contraseña de la web.

- Cada carpeta accesible que el usuario puede leer aparece como `/dav/<nombre>/`; se aplican los **mismos permisos por carpeta** (leer / escribir)
- Borrar desde el explorador del sistema envía a la **papelera** de LabNAS
- Rutas canonicalizadas (sin `..` ni symlinks que escapen) y `~/.labnas` bloqueado; mismo bloqueo por intentos fallidos que el login
- Linux: `dav://<servidor>:3001/dav/` en el gestor de archivos (o `davfs2`); macOS: Finder > Ir > Conectarse al servidor
- Windows sobre HTTP (sin TLS) requiere habilitar Basic en el cliente WebDAV (`HKLM\SYSTEM\CurrentControlSet\Services\WebClient\Parameters\BasicAuthLevel = 2`) o usar WinSCP / Cyberduck. Sin HTTPS la contraseña viaja en claro: úsalo solo en la LAN o sobre Tailscale

## Papelera

Borrar desde el explorador **mueve a la papelera** (`.labnas-trash/`, oculta, en el mismo disco que el archivo: mover es instantáneo). Desde **Archivos > Papelera** se restaura a la ruta original (con sufijo si ya existe algo con ese nombre) o se borra definitivamente; un admin puede vaciarla entera. Lo que lleva más de 30 días se borra solo (setting `trash_retention_days`). La papelera no se incluye en los respaldos.

## Respaldos

**Configuración > Respaldos** (admin). Cada tarea copia una carpeta (dentro de las carpetas accesibles) a un destino local —un disco externo o una carpeta de red montada (NFS/SMB)— todos los días a la hora elegida.

- **Snapshots incrementales** con `rsync --link-dest`: cada copia (`<destino>/AAAA-MM-DD_HHMMSS/`) es completa y navegable, pero los archivos sin cambios son hardlinks a la anterior y no ocupan espacio de nuevo. `latest` apunta a la última
- Se conservan las últimas N copias; opcionalmente incluye la base de datos de LabNAS (`_labnas/labnas.db`)
- Si el equipo estaba apagado a la hora programada, la copia se hace al volver
- Fallos y advertencias se notifican a los admins por Telegram y quedan en la auditoría
- Además, `labnas.db` se copia sola cada día en `~/.labnas/backups/` (se conservan 7)

Requiere `rsync` en el servidor.

> **Importante:** los secretos de la base van cifrados con `~/.labnas/secret.key`, que **no** se incluye en los respaldos (así una copia de la base sola no expone contraseñas ni tokens). Guarda ese archivo aparte: para restaurar la base en otra máquina hay que copiar también `secret.key` a `~/.labnas/`.

## Auditoría

Todas las acciones registradas (subidas, borrados, cambios de rol, actualizaciones, rollbacks...) se guardan en SQLite y se consultan en **Configuración > Administración > Auditoría**, con filtro por usuario y texto. Se conservan 180 días (setting `audit_retention_days`). El bot de Telegram las muestra con `/actividad`.

## Seguridad

- El servicio corre como usuario normal (no root), con solo `CAP_NET_RAW` y `CAP_NET_BIND_SERVICE`
- Todas las rutas API requieren auth (excepto login/registro/branding/health, ingesta de sensores y links compartidos)
- Permisos **denegados por defecto**: toda ruta no declarada en `middleware.rs` es solo admin (con tests)
- Registro abierto, pero las cuentas nuevas quedan **pendientes** hasta que un admin las apruebe
- Acceso a archivos limitado a las raíces de almacenamiento (canonicalizadas: sin `..` ni symlinks que escapen); `~/.labnas` bloqueado
- Passwords con bcrypt (cost 10, mínimo 8 caracteres); bloqueo de 5 min tras 5 intentos fallidos
- Sesiones de 24h persistidas en SQLite (sobreviven reinicios); cambiar la contraseña cierra las demás
- `labnas.db` con permisos 600
- Sin CORS por defecto (la web y el visor son mismo origen); para autorizar un origen externo: `LABNAS_CORS_ORIGINS=https://dashboard.lab.cl`
- Secretos cifrados en la base (token del bot, API keys de impresoras/Groq/Last.fm, contraseñas de correo) con XChaCha20-Poly1305; la clave está en `~/.labnas/secret.key` (600)
- Token del bot jamás expuesto en la API
- Comandos CUPS sanitizados contra inyección
- Terminal web y `/cmd` de Telegram nunca corren como root

## Licencia

MIT

---

<p align="center">
  Hecho por <a href="https://github.com/Debaq">TecMedHub</a>
</p>
