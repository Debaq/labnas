<p align="center">
  <img src="frontend/public/favicon.svg" width="80" alt="LabNAS" />
</p>

<h1 align="center">LabNAS</h1>

<p align="center">
  <strong>Self-hosted lab server: NAS + network scanner + 3D printers + music streaming + AI email + task management + Telegram bot — one binary, one web UI.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-Axum_0.8-orange?style=flat-square" alt="Rust" />
  <img src="https://img.shields.io/badge/React-19-blue?style=flat-square" alt="React" />
  <img src="https://img.shields.io/badge/TypeScript-5.9-blue?style=flat-square" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Telegram-40%2B_commands-26A5E4?style=flat-square" alt="Telegram" />
  <img src="https://img.shields.io/badge/Groq-AI_Classification-orange?style=flat-square" alt="Groq" />
  <img src="https://img.shields.io/badge/Tested_on-Arch_Linux-1793D1?style=flat-square" alt="Arch" />
  <img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="License" />
</p>

---

## What is LabNAS?

A self-hosted platform for managing a laboratory server. It combines a file explorer, web terminal, bidirectional Telegram bot, AI-powered email, project/task management, 3D printer control, music/video streaming, document printing, and network security — all in a single modern web interface.

Ships as a **statically-linked binary** (musl) with embedded web UI. No Docker, no runtime dependencies beyond the binary itself.

## Quick Start

```bash
# Download latest release
TAG=$(curl -s https://api.github.com/repos/Debaq/labnas/releases/latest | grep tag_name | cut -d'"' -f4)
curl -sL "https://github.com/Debaq/labnas/releases/download/${TAG}/labnas-${TAG}-linux-x86_64.tar.gz" | tar xz

# Run
cd labnas
sudo ./labnas-backend
```

Open `http://localhost:3001` — the first account you create becomes admin.

## Features

### File Management
- Full file browser: upload, download, delete, create directories
- Quick access shortcuts to common paths
- Share files via temporary links (24h expiry)
- Download files from URL directly to the NAS
- Print files directly to CUPS printers
- BTRFS subvolume deduplication

### Music & Video Player
- YouTube search and playback via `yt-dlp` + `mpv`
- **Dual playback modes**: NAS speakers or browser streaming
- Full transport controls: play, pause, previous, next, stop
- Queue management: reorder (drag up/down), play any item, remove
- Shuffle and repeat modes (off / all / one)
- Volume control (ALSA/PulseAudio on NAS, Web Audio in browser)
- AI-powered recommendations using YouTube Mix — multi-seed, artist-diversified
- **Video output**: fullscreen on any connected display with multi-monitor support (X11)
- Persistent side panel accessible from every page
- Full Telegram bot control: `/play`, `/next`, `/stop`, `/pause`, `/mix`, `/vol`

### Email (IMAP & POP3)
- IMAP and POP3 protocol support with in-app protocol selector
- Compatible with Gmail (IMAP), Outlook (POP3 + app password), and any standard mail server
- AI classification via Groq LLM: urgent, task, informational, spam
- Auto-generated summaries and suggested actions per email
- Sender-based filters: priority, normal, silent, ignore
- Convert any email to a task with one click
- Background checking every 5 minutes with Telegram alerts for urgent mail
- Telegram commands: `/correos`, `/leer UID`, `/correo2tarea UID`

### 3D Printers
- OctoPrint and Moonraker (Klipper) integration
- Real-time temperatures with visual bars (hotend + bed)
- Job control: start, pause, resume, cancel
- Jog pad for axis movement (0.1, 1, 10, 100mm)
- Home axes and manual G-code commands
- File management on printer: list, print, delete
- Webcam snapshots in web UI and via Telegram (`/camara`)
- Upload `.gcode` files with drag & drop
- Auto-monitor: Telegram notification when print finishes or errors
- Network auto-detection of printers

### Tasks & Projects
- Create tasks with assignments (`@user`, `@all`), due dates, and project grouping
- Confirmation workflow: requires explicit accept/reject
- Insistent reminders via Telegram (default 8 min, configurable)
- Project progress tracking with visual bars
- Calendar with events, invitations (RSVP), and reminders
- Full Telegram command set: `/tarea`, `/tareas`, `/hecho`, `/confirmar`, `/rechazar`, `/proyecto`, `/eventos`

### Network Scanner
- ICMP-based network scanning with automatic device discovery
- MAC address and manufacturer detection
- Known vs unknown device tracking
- Telegram alerts for new unknown devices
- Custom device labeling
- Periodic background scanning (every 5 minutes)

### Telegram Bot (40+ commands)

| Category | Commands |
|----------|---------|
| System | `/estado` `/discos` `/ram` `/cpu` `/uptime` `/red` `/ip` `/actividad` |
| 3D Printers | `/impresoras` `/temp` `/camara` `/imprimir` `/pausar` `/cancelar3d` |
| Terminal | `/cmd <command>` — interactive remote shell with sudo support |
| Tasks | `/tarea` `/tareas` `/hecho` `/confirmar` `/rechazar` `/avance` |
| Projects | `/proyecto` `/proyectos` |
| Calendar | `/evento` `/eventos` `/aceptar` `/declinar` |
| Email | `/correos` `/leer` `/correo2tarea` |
| Music | `/musica` `/play` `/next` `/stop` `/pause` `/mix` `/vol` |
| User | `/vincular` `/mirol` `/horario` `/ayuda` |

### Web Terminal
- Full PTY terminal over WebSocket (xterm.js)
- Runs as the logged-in system user (not root) for security
- Supports resize, 256 colors, and interactive programs
- Interactive terminal also available via Telegram (`/cmd`)

### Document Printing
- CUPS integration with dynamic per-printer options
- Paper size, quality, color, duplex settings
- Print queue with cancellation
- Drag & drop file printing

### Notes
- Markdown editor with live split preview
- Headers, bold, italic, code blocks, lists, links
- Collaborative: all users can view and edit

### Lab Services Dashboard
- Register lab services running on other ports (CVAT, Label Studio, CUPS, Jupyter, etc.)
- Quick-access link cards on the dashboard
- Admin management from Settings

### Notifications
- Full Telegram bot with long polling
- Daily scheduled reports (system status + activity log) — per-user configurable time
- Real-time alerts: new network devices, urgent emails, 3D print completion/errors
- Role-based notification filtering

### System & Administration
- Real-time dashboard: CPU, RAM, disk usage, network hosts, printers, uptime
- 4 themes: Dracula (default), Light, Nord, Solarized + auto mode
- Custom branding: lab name, logo, accent color, institution, mission/vision
- Self-update from GitHub Releases — checks every 6 hours, one-click update
- Systemd service with auto-restart
- mDNS/Bonjour advertisement (`labnas.local`)
- Shutdown from web UI (admin only)

### Authentication & Security
- Multi-user with 4 roles: Admin, Operator, Observer, Pending
- bcrypt password hashing + session tokens (24h expiry)
- Per-role granular permissions (terminal, printing, files)
- Route-level middleware permission enforcement
- Rate limiting on login (2s delay per failed attempt)
- Bot token never exposed in API responses
- Telegram ↔ web account linking with 8-char verification codes (5 min expiry)

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Backend | Rust, Axum 0.8, Tokio |
| Frontend | React 19, TypeScript 5.9, Vite 8, TailwindCSS 4 |
| Terminal | portable-pty + xterm.js 6 (WebSocket) |
| Music/Video | yt-dlp + mpv (X11 multi-monitor) |
| Email | IMAP (native), POP3 (custom TLS implementation) |
| AI | Groq API (Llama 3.3 70B Versatile) |
| 3D Printers | OctoPrint API, Moonraker API |
| Printing | CUPS CLI (lp, lpstat, cancel) |
| Network | ICMP ping (surge-ping), DNS lookup |
| mDNS | mdns-sd |
| Notifications | Telegram Bot API (long polling) |
| Auth | bcrypt + UUID session tokens |
| Build | Static binary via musl + vendored OpenSSL |

## Requirements

**Tested on Arch Linux.** Should work on any modern Linux distribution.

- **OS**: Linux x86_64
- **Optional runtime dependencies** (for full functionality):

| Dependency | Used for | Install (Arch) |
|-----------|---------|----------------|
| `mpv` | Music/video playback | `pacman -S mpv` |
| `yt-dlp` | YouTube search & streaming | `pacman -S yt-dlp` |
| `alsa-utils` | Volume control | `pacman -S alsa-utils` |
| `cups` | Document printing | `pacman -S cups` |
| `avahi` + `nss-mdns` | mDNS hostname resolution | `pacman -S avahi nss-mdns` |

```bash
# Install all optional dependencies at once (Arch)
sudo pacman -S mpv yt-dlp alsa-utils cups avahi nss-mdns
```

## Installation

### Pre-built Binary (recommended)

```bash
# Download and extract
TAG=$(curl -s https://api.github.com/repos/Debaq/labnas/releases/latest | grep tag_name | cut -d'"' -f4)
curl -sL "https://github.com/Debaq/labnas/releases/download/${TAG}/labnas-${TAG}-linux-x86_64.tar.gz" | tar xz

# Move to system directory
sudo mv labnas /opt/labnas

# Run directly
sudo /opt/labnas/labnas-backend
```

### Systemd Service (production)

```bash
sudo tee /etc/systemd/system/labnas.service > /dev/null << 'EOF'
[Unit]
Description=LabNAS Server
After=network.target

[Service]
Type=simple
ExecStart=/opt/labnas/labnas-backend
WorkingDirectory=/opt/labnas
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now labnas
```

### mDNS Setup

Enable mDNS from **Settings > mDNS** in the web UI.

For client machines to resolve `labnas.local`:

```bash
# Use the included helper script
sudo bash /opt/labnas/setup-mdns.sh

# Or manually (Arch):
sudo pacman -S avahi nss-mdns
sudo systemctl enable --now avahi-daemon
# Ensure /etc/nsswitch.conf has: hosts: ... mdns4_minimal [NOTFOUND=return] ... dns
```

**Windows clients**: Install [Bonjour Print Services](https://support.apple.com/kb/DL999) or ensure UDP port 5353 is open.

### Build from Source

```bash
git clone https://github.com/Debaq/labnas.git
cd labnas

# Interactive menu
./labnas.sh

# Or directly:
./labnas.sh build    # Production binary + frontend
./labnas.sh run      # Run production build
./labnas.sh dev      # Development with hot reload
```

**Build requirements**: Rust toolchain (stable), Node.js 20+, npm

## Configuration

Config is stored at `~/.labnas/config.json` (relative to the binary owner's home). Override with `LABNAS_CONFIG` env var.

### First Setup
1. Open `http://localhost:3001` (or `http://labnas.local:3001`)
2. Create your admin account
3. Go to **Settings** to configure:
   - Telegram bot token
   - Lab branding (name, logo, colors)
   - 3D printers
   - Email accounts
   - Lab services (port links)
   - mDNS hostname

### Telegram Bot
1. Create a bot via [@BotFather](https://t.me/botfather)
2. Paste the token in **Settings > Notifications**
3. Send `/start` to your bot
4. Link web account: generate code in Settings, then `/vincular CODE` in Telegram

### Email with AI
1. Get a free API key at [console.groq.com](https://console.groq.com)
2. Admin sets the key in **Settings > Email > Groq API Key**
3. Each user configures their mail account:
   - **Gmail**: IMAP, `imap.gmail.com:993`, app password
   - **Outlook**: POP3, `outlook.office365.com:995`, app password

### Video on External Displays
When running as a systemd service, LabNAS automatically:
1. Detects the user with the active X session
2. Grants root X11 access via `xhost`
3. Detects connected displays via DRM

Select a display from the music panel menu (three dots icon) to play video fullscreen.

## Ports

| Service | Port | Protocol |
|---------|------|----------|
| Web UI + API | 3001 | HTTP |
| mDNS | 5353 | UDP multicast |

## Project Structure

```
labnas/
  backend/src/
    main.rs              # Router, server, background tasks
    state.rs             # Shared state (sessions, terminals, music)
    config.rs            # JSON config persistence
    middleware.rs         # Role-based permission enforcement
    handlers/
      auth.rs            # Login, register, roles, linking
      files.rs           # File browser + sharing
      music.rs           # Music/video player + YouTube
      email.rs           # IMAP/POP3 + AI classification
      network.rs         # Network scanner + MAC detection
      system.rs          # System info, update, branding, services
      terminal.rs        # WebSocket PTY terminal
      notifications.rs   # Telegram bot (40+ commands)
      tasks.rs           # Tasks, projects, calendar
      printers3d.rs      # 3D printer control
      printing.rs        # CUPS document printing
      extras.rs          # Temp links, URL download, notes
    models/              # Data structures
  frontend/src/
    pages/               # Page components
    components/
      Layout.tsx         # App shell with sidebar
      MusicPanel.tsx     # Global music side panel
    themes/              # Theme system (4 themes + auto)
    auth/                # Auth context + permission hooks
    api/                 # Type-safe API client
  labnas.sh              # Dev/build/run interactive script
  setup-mdns.sh          # mDNS client setup helper
```

## Self-Update

LabNAS checks GitHub for new releases every 6 hours. When an update is available:
- Admin gets a Telegram notification
- **Settings** shows an "Update" button
- One click downloads, extracts, replaces, and restarts via systemd

## Security Notes

- All API routes require authentication (except login/register)
- Passwords hashed with bcrypt (cost factor 12)
- Session tokens: UUID v4, 24-hour expiry
- Role-based middleware blocks unauthorized API calls
- Bot token never exposed in API responses
- CUPS commands sanitized against injection
- System paths protected from deletion
- Terminal runs as the desktop user, not root

## License

MIT

---

<p align="center">
  Built by <a href="https://github.com/Debaq">TecMedHub</a>
</p>
