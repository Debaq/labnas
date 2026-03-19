<p align="center">
  <img src="frontend/public/favicon.svg" width="80" alt="LabNAS" />
</p>

<h1 align="center">LabNAS</h1>

<p align="center">
  <strong>NAS de laboratorio con gestion de impresoras 3D, impresion CUPS y herramientas de red</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-Axum-orange?style=flat-square" alt="Rust" />
  <img src="https://img.shields.io/badge/React-19-blue?style=flat-square" alt="React" />
  <img src="https://img.shields.io/badge/TypeScript-5.9-blue?style=flat-square" alt="TypeScript" />
  <img src="https://img.shields.io/badge/TailwindCSS-4-cyan?style=flat-square" alt="Tailwind" />
  <img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="License" />
</p>

---

## Que es LabNAS?

Una aplicacion web self-hosted para gestionar un servidor NAS de laboratorio. Combina explorador de archivos, terminal remota, escaner de red, gestion de impresoras 3D y impresion de documentos en una sola interfaz con 4 temas visuales.

## Funcionalidades

### Explorador de Archivos
- Navegacion completa del sistema de archivos con breadcrumbs
- Subir, descargar y eliminar archivos
- Crear carpetas
- Accesos rapidos (Inicio, Documentos, Descargas, etc.)
- Boton de impresion directa para archivos compatibles

### Impresoras 3D
- Soporte para **OctoPrint** (puertos 80/5000) y **Moonraker** (puerto 7125)
- Agregar impresoras manualmente o auto-detectar en la red
- Monitoreo en tiempo real: temperaturas (hotend/cama), progreso de impresion
- Subida de archivos `.gcode` con drag & drop
- Persistencia de configuracion entre reinicios

### Impresion de Documentos (CUPS)
- Deteccion automatica de impresoras CUPS del sistema
- Zona de drag & drop para imprimir archivos
- Opciones: copias, orientacion, doble cara, rango de paginas
- Cola de impresion con cancelacion de trabajos
- Imprimir directamente desde el explorador de archivos

### Red Local
- Escaneo de red con ping ICMP (resolucion DNS)
- Tiempos de respuesta por host
- Configurar hosts como impresoras 3D desde la tabla

### Terminal Web
- Terminal completa via WebSocket + PTY
- Shell del usuario con colores y resize
- Soporte xterm-256color

### Dashboard
- Informacion del sistema (hostname, SO, kernel, RAM, CPUs)
- Discos montados con espacio disponible
- Dispositivos activos en la red
- Resumen de impresoras 3D

### Temas
4 temas incluidos: **Dracula**, **Light**, **Nord**, **Solarized**

## Stack Tecnico

| Componente | Tecnologia |
|---|---|
| Backend | Rust + Axum 0.8 |
| Frontend | React 19 + TypeScript 5.9 |
| Estilos | Tailwind CSS 4 |
| Bundler | Vite 8 |
| Terminal | xterm.js 6 + portable-pty |
| Ping | surge-ping (ICMP nativo) |
| 3D Printers | reqwest (OctoPrint/Moonraker API) |
| CUPS | CLI nativo (lp, lpstat, cancel) |
| Iconos | Lucide React |

## Estructura del Proyecto

```
labnas/
├── backend/
│   └── src/
│       ├── main.rs              # Router + servidor
│       ├── state.rs             # Estado compartido
│       ├── config.rs            # Persistencia JSON
│       ├── handlers/
│       │   ├── files.rs         # Explorador de archivos
│       │   ├── network.rs       # Escaner de red
│       │   ├── system.rs        # Info del sistema
│       │   ├── terminal.rs      # Terminal WebSocket
│       │   ├── printers3d.rs    # Impresoras 3D
│       │   └── printing.rs      # Impresion CUPS
│       └── models/
│           ├── files.rs
│           ├── network.rs
│           ├── system.rs
│           ├── printers3d.rs
│           └── printing.rs
├── frontend/
│   └── src/
│       ├── pages/
│       │   ├── DashboardPage.tsx
│       │   ├── FilesPage.tsx
│       │   ├── Printers3DPage.tsx
│       │   ├── PrintingPage.tsx
│       │   ├── NetworkPage.tsx
│       │   ├── TerminalPage.tsx
│       │   └── SettingsPage.tsx
│       ├── api/index.ts
│       ├── types/index.ts
│       └── themes/
└── labnas.sh                    # Script de gestion
```

## Requisitos

- **Rust** (cargo)
- **Node.js** 18+ (npm)
- **Linux** (para ping ICMP se necesita `cap_net_raw` o ejecutar con sudo)
- **CUPS** (opcional, para impresion de documentos)

## Instalacion y Uso

```bash
# Clonar
git clone https://github.com/Debaq/labnas.git
cd labnas

# Modo desarrollo (hot reload)
./labnas.sh dev

# Compilar para produccion
./labnas.sh build

# Ejecutar build de produccion
./labnas.sh run
```

### Modo Desarrollo

```
Backend:  http://localhost:3001
Frontend: http://localhost:5173 (con proxy a backend)
```

### Modo Produccion

```
Todo en: http://localhost:3001
```

## API Endpoints

### Sistema
| Metodo | Ruta | Descripcion |
|---|---|---|
| GET | `/api/health` | Estado del servidor |
| GET | `/api/system/info` | Informacion del sistema |
| GET | `/api/system/disks` | Discos montados |

### Archivos
| Metodo | Ruta | Descripcion |
|---|---|---|
| GET | `/api/files?path=` | Listar directorio |
| POST | `/api/files/upload` | Subir archivo (multipart) |
| GET | `/api/files/download?path=` | Descargar archivo |
| DELETE | `/api/files?path=` | Eliminar archivo/carpeta |
| POST | `/api/files/directory` | Crear carpeta |

### Red
| Metodo | Ruta | Descripcion |
|---|---|---|
| POST | `/api/network/scan` | Escanear red local |
| GET | `/api/network/hosts` | Hosts escaneados |

### Impresoras 3D
| Metodo | Ruta | Descripcion |
|---|---|---|
| GET | `/api/printers3d` | Listar impresoras |
| POST | `/api/printers3d` | Agregar impresora |
| DELETE | `/api/printers3d/{id}` | Eliminar impresora |
| GET | `/api/printers3d/{id}/status` | Estado y temperaturas |
| POST | `/api/printers3d/{id}/upload` | Subir .gcode |
| POST | `/api/printers3d/detect` | Auto-detectar en red |

### Impresion CUPS
| Metodo | Ruta | Descripcion |
|---|---|---|
| GET | `/api/printing/printers` | Listar impresoras CUPS |
| POST | `/api/printing/print` | Imprimir archivo (multipart) |
| POST | `/api/printing/print-file` | Imprimir por ruta |
| GET | `/api/printing/jobs` | Cola de impresion |
| DELETE | `/api/printing/jobs/{id}` | Cancelar trabajo |

### Terminal
| Metodo | Ruta | Descripcion |
|---|---|---|
| GET | `/api/terminal` | WebSocket terminal |

## Configuracion

La configuracion de impresoras 3D se guarda en `~/.labnas/config.json` y persiste entre reinicios.

## Seguridad

- Rutas del sistema protegidas contra eliminacion (`/`, `/bin`, `/usr`, etc.)
- Validacion de inputs en comandos CUPS (prevencion de inyeccion)
- API keys de impresoras 3D almacenadas localmente, nunca expuestas al frontend en las respuestas de status
- Sin autenticacion por defecto — diseñado para redes locales de confianza

## Licencia

MIT
