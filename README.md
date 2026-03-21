<p align="center">
  <img src="frontend/public/favicon.svg" width="80" alt="LabNAS" />
</p>

<h1 align="center">LabNAS</h1>

<p align="center">
  <strong>NAS inteligente para laboratorio con bot Telegram, correo con IA, gestion de proyectos y acceso remoto</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-Axum-orange?style=flat-square" alt="Rust" />
  <img src="https://img.shields.io/badge/React-19-blue?style=flat-square" alt="React" />
  <img src="https://img.shields.io/badge/TypeScript-5.9-blue?style=flat-square" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Telegram-Bot-26A5E4?style=flat-square" alt="Telegram" />
  <img src="https://img.shields.io/badge/Groq-IA-orange?style=flat-square" alt="Groq" />
  <img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="License" />
</p>

---

## Que es LabNAS?

Una plataforma self-hosted para gestionar un servidor NAS de laboratorio. Combina explorador de archivos, terminal remota, bot Telegram bidireccional, correo inteligente con IA, gestion de proyectos, impresoras 3D, impresion CUPS, sistema de seguridad de red y acceso remoto en una sola interfaz.

## Instalacion rapida

```bash
# Descargar ultimo release
wget https://github.com/Debaq/labnas/releases/latest/download/labnas-v1.6.0-linux-x86_64.tar.gz
tar xzf labnas-v1.6.0-linux-x86_64.tar.gz
cd labnas
sudo ./labnas-backend
```

La primera cuenta que crees en la web sera administrador.

## Funcionalidades

### Bot Telegram (40+ comandos)
Bot bidireccional que funciona como asistente personal del laboratorio:

| Comando | Descripcion |
|---|---|
| `/estado` `/discos` `/ram` `/cpu` `/uptime` | Info del sistema |
| `/red` | Dispositivos en la red |
| `/impresoras` | Estado impresoras 3D |
| `/camara` `/foto` | Snapshot de webcam de impresora 3D |
| `/temp` | Temperaturas de todas las impresoras |
| `/imprimir` `/pausar` `/cancelar3d` | Control de impresion 3D |
| `/cmd <comando>` | Terminal remota interactiva (soporta sudo) |
| `/tarea Titulo @persona !insistente` | Crear tarea con recordatorios |
| `/tareas` `/hecho ID` `/confirmar ID` | Gestionar tareas |
| `/proyecto Nombre` `/proyectos` `/avance` | Proyectos con progreso |
| `/evento 2026-03-25 10:00 Reunion @all` | Crear evento + invitar |
| `/eventos` `/aceptar ID` `/declinar ID` | Calendario |
| `/correos` | Resumen de bandeja con IA |
| `/leer UID` | Resumen IA de un correo |
| `/correo2tarea UID` | Convertir correo en tarea |
| `/horario 08:00` | Reporte diario personal |
| `/actividad` | Log de actividad reciente |
| `/ip` | IPs local y Tailscale |
| `/vincular CODIGO` | Vincular con cuenta web |
| `/mirol` `/ayuda` | Info de usuario |

### Correo inteligente (IMAP + Groq IA)
- Conecta Outlook, Gmail o cualquier IMAP
- Cada usuario configura su propia cuenta
- Groq clasifica automaticamente: urgente, tarea, informativo, spam
- Filtros personalizables por remitente (prioritario, normal, silencioso, ignorar)
- Notifica urgentes por Telegram al instante
- Convierte correos en tareas insistentes con un comando
- Revision automatica cada 5 minutos

### Tareas, proyectos y calendario
- Crear tareas con asignados (`@persona`, `@all`)
- Modo confirmacion: requiere `/confirmar` o `/rechazar`
- Modo insistente: recuerda cada 8 min (configurable con `!cada5`)
- Proyectos con barras de progreso
- Calendario con eventos, invitaciones y recordatorios
- Todo gestionable desde web y Telegram

### Explorador de archivos
- Navegacion completa del sistema de archivos
- Subir, descargar, eliminar, crear carpetas
- Links temporales para compartir archivos (expiran en 24h)
- Descargar archivos desde URL directo al NAS
- Imprimir directamente archivos compatibles
- Accesos rapidos configurables

### Notas Markdown
- Editor split con preview en vivo
- Soporte: headers, bold, italic, code, listas, links
- Colaborativo: cualquier usuario puede editar

### Impresion CUPS
- Opciones dinamicas de cada impresora (papel, calidad, color, duplex)
- Reanudar/pausar impresoras desde la web
- Cola de impresion con cancelacion
- Drag & drop para imprimir

### Impresoras 3D (sistema completo)
- Soporte OctoPrint y Moonraker
- Control de impresion: iniciar, pausar, reanudar, cancelar
- Temperaturas en tiempo real con barras visuales (hotend + cama)
- Precalentar y enfriar con un click
- Jog pad para mover ejes (0.1, 1, 10, 100mm)
- Home de ejes y envio de G-code manual
- Archivos en la impresora: listar, imprimir, eliminar
- Snapshot de webcam en la web y por Telegram (`/camara`)
- Monitor automatico: notifica por Telegram al terminar o si hay error
- Auto-deteccion en la red
- Upload de .gcode con drag & drop

### Red y seguridad ("sistema inmune")
- Escaneo de red con deteccion de MAC y fabricante
- Dispositivos conocidos vs desconocidos
- Alerta por Telegram cuando aparece dispositivo nuevo
- Etiquetar dispositivos con nombre personalizado

### Terminal web
- Shell completa via WebSocket + PTY
- Terminal interactiva via Telegram (`/cmd`)
- Soporta sudo y comandos interactivos

### Autenticacion y roles
- Login/registro web con bcrypt
- Primera cuenta = administrador
- 4 roles: pendiente, observador, operador, admin
- Permisos granulares: terminal, impresion, archivos
- Middleware que bloquea acciones no autorizadas a nivel de API
- Sesiones con expiracion de 24h
- Rate limiting en login
- Vinculacion de cuentas web <-> Telegram

### Acceso remoto (Tailscale)
- Accede desde cualquier parte sin abrir puertos
- Funciona en Linux, Windows, macOS, iPhone, Android
- Instalacion con un click desde Configuracion
- LabNAS detecta Tailscale automaticamente

### Auto-update
- Chequea GitHub releases cada 6 horas
- Notifica al admin por Telegram
- Boton "Actualizar" en Configuracion
- Descarga, extrae y reinicia automaticamente

### Extras
- Saludo por Telegram al encender (con IP local y Tailscale)
- Deteccion automatica de firewall (ufw) al iniciar
- Log de actividad: uploads, eliminaciones, impresiones, escaneos
- Horario personal de reportes diarios por usuario
- 4 temas: Dracula, Light, Nord, Solarized
- Cambio de contrasena desde configuracion

## Stack Tecnico

| Componente | Tecnologia |
|---|---|
| Backend | Rust + Axum 0.8 |
| Frontend | React 19 + TypeScript 5.9 |
| Estilos | Tailwind CSS 4 |
| Bundler | Vite 8 |
| Bot | Telegram Bot API (long polling) |
| IA | Groq API (Llama 3.3 70B) |
| Correo | IMAP + native-tls + mailparse |
| Auth | bcrypt + UUID sessions |
| Terminal | xterm.js 6 + portable-pty |
| Ping | surge-ping (ICMP nativo) |
| 3D Printers | reqwest (OctoPrint/Moonraker) |
| CUPS | CLI nativo (lp, lpstat, cancel) |
| VPN | Tailscale (opcional) |

## Estructura del Proyecto

```
labnas/
├── backend/src/
│   ├── main.rs              # Router + servidor + startup checks
│   ├── state.rs             # Estado compartido (sessions, terminals, emails)
│   ├── config.rs            # Persistencia JSON (~/.labnas/config.json)
│   ├── middleware.rs         # Verificacion de permisos por ruta
│   ├── handlers/
│   │   ├── auth.rs          # Login, registro, roles, vinculacion
│   │   ├── files.rs         # Explorador de archivos
│   │   ├── network.rs       # Escaner de red + MAC + vendor
│   │   ├── system.rs        # Info sistema + autostart + auto-update
│   │   ├── terminal.rs      # Terminal WebSocket
│   │   ├── notifications.rs # Bot Telegram (40+ comandos)
│   │   ├── tasks.rs         # Tareas, proyectos, calendario
│   │   ├── email.rs         # IMAP + Groq IA + filtros
│   │   ├── extras.rs        # Links temporales, download URL, notas
│   │   ├── printers3d.rs    # Impresoras 3D
│   │   └── printing.rs      # Impresion CUPS
│   └── models/              # Structs de datos
├── frontend/src/
│   ├── auth/AuthContext.tsx  # Autenticacion + polling de permisos
│   ├── pages/
│   │   ├── LoginPage.tsx
│   │   ├── DashboardPage.tsx
│   │   ├── FilesPage.tsx     # + compartir + descargar URL
│   │   ├── PrintingPage.tsx  # + opciones dinamicas
│   │   ├── Printers3DPage.tsx
│   │   ├── NetworkPage.tsx   # + MAC + vendor + conocidos
│   │   ├── TasksPage.tsx     # + calendario
│   │   ├── NotesPage.tsx     # Editor Markdown
│   │   ├── TerminalPage.tsx
│   │   └── SettingsPage.tsx  # Roles, Telegram, Tailscale, update
│   ├── api/index.ts          # Wrapper con auth automatico
│   └── themes/
├── labnas.sh                 # Script de gestion
├── setup-tailscale.sh        # Instalador de Tailscale
└── diagnostico.sh            # Diagnostico de problemas
```

## Requisitos

- **Linux** (Arch, Ubuntu, Debian, Fedora, Manjaro)
- Para desarrollo: Rust (cargo) + Node.js 18+ (npm)
- Para produccion: solo el binario del release
- **CUPS** (opcional, para impresion)
- **Tailscale** (opcional, para acceso remoto)

## Configuracion

### Telegram Bot
1. Crear bot con BotFather en Telegram (`/newbot`)
2. Pegar token en Configuracion > Telegram
3. Enviar `/start` al bot — el primero es admin

### Correo con IA
1. Obtener API key gratis en https://console.groq.com
2. Admin pone la key en Configuracion
3. Cada usuario configura su IMAP (Outlook: `outlook.office365.com:993`)
4. Configurar filtros por remitente

### Acceso remoto
```bash
sudo bash setup-tailscale.sh
```
Luego instalar Tailscale en tus dispositivos con la misma cuenta.

## Seguridad

- Autenticacion obligatoria con bcrypt + sesiones de 24h
- Middleware de permisos por ruta (admin, operador, observador)
- Bot token de Telegram nunca expuesto en la API
- IDOR protegido: verificacion de ownership en CRUD
- Rate limiting en login (2s delay por intento fallido)
- Validacion de inputs en comandos CUPS
- Rutas del sistema protegidas contra eliminacion
- Codigos de vinculacion de 8 caracteres con expiracion de 5 min

## Licencia

MIT
