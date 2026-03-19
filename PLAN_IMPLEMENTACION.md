# Plan: LabNAS - Impresoras 3D, Impresion CUPS y mas

**Session ID anterior:** `9fb07ccc-b46f-4cc0-a003-2df05bfee8d0`
**Transcript:** `/home/nick/.claude/projects/-home-nick-Escritorio-Proyectos-labnas/9fb07ccc-b46f-4cc0-a003-2df05bfee8d0.jsonl`

---

## Contexto
LabNAS ya tiene: explorador de archivos (acceso total), escaner de red, terminal web, dashboard con info de sistema, 4 temas. El usuario quiere expandirlo con gestion de impresoras 3D (OctoPrint/Moonraker), impresion de documentos via CUPS, y otras features para laboratorio.

---

## Fase 0: Preparacion

### 0.1 Persistencia con JSON
Guardar configuracion en `~/.labnas/config.json` (usando serde_json + tokio::fs). Sin esto no se pueden recordar las impresoras configuradas entre reinicios.

### 0.2 Modularizar backend
Con ~870 lineas actuales y 2 features grandes, main.rs se vuelve inmanejable. Dividir en:
```
backend/src/
  main.rs           → solo router + server setup
  state.rs          → AppState + LabNasConfig
  config.rs         → read/write config JSON
  handlers/
    files.rs, network.rs, system.rs, terminal.rs  → mover de main.rs
    printers3d.rs   → NUEVO
    printing.rs     → NUEVO
  models/
    mod.rs, files.rs, network.rs, system.rs       → mover structs
    printers3d.rs   → NUEVO
    printing.rs     → NUEVO
```

### 0.3 Agregar reqwest
`reqwest = { version = "0.12", features = ["json", "multipart"] }` para llamar APIs de impresoras 3D.

---

## Fase 1: Impresoras 3D

### Backend

**Rutas API:**
| Ruta | Metodo | Funcion |
|------|--------|---------|
| `/api/printers3d` | GET | Listar impresoras configuradas |
| `/api/printers3d` | POST | Agregar impresora (ip, nombre, tipo, puerto, api_key) |
| `/api/printers3d/{id}` | DELETE | Eliminar impresora |
| `/api/printers3d/{id}/status` | GET | Estado: online, temperaturas, trabajo actual |
| `/api/printers3d/{id}/upload` | POST | Subir .gcode a la impresora |
| `/api/printers3d/detect` | POST | Auto-detectar en hosts escaneados (probar puertos 80/5000/7125) |

**Protocolos soportados:**
- **OctoPrint** (puerto 80/5000): `/api/version`, `/api/printer`, `/api/job`, `/api/files/local`
- **Moonraker** (puerto 7125): `/printer/info`, `/printer/objects/query`, `/server/files/upload`

**Structs clave:** `Printer3DConfig`, `Printer3DStatus`, `PrinterTemps`, `PrintJob`

### Frontend

**Nueva pagina `Printers3DPage.tsx`:**
- Grid de tarjetas por impresora: nombre, IP, estado (badge), temperaturas (hotend/bed), progreso de impresion (barra)
- Boton "Agregar Impresora" → modal con selector de IP (de hosts escaneados), tipo, puerto, API key
- Boton "Detectar automaticamente" que busca impresoras en la red
- Upload .gcode con drag & drop en cada tarjeta
- Polling de estado cada 5-10 segundos

**Modificar `NetworkPage.tsx`:** Agregar boton "Configurar como impresora 3D" en cada host.

**Modificar `DashboardPage.tsx`:** Tarjeta resumen de impresoras 3D activas.

---

## Fase 2: Impresion de Documentos (CUPS)

### Backend

**Integracion via comandos CLI** (no requiere dependencias extra):
- `lpstat -p -d` → listar impresoras
- `lp -d {impresora} -n {copias} -o {opciones} {archivo}` → imprimir
- `lpstat -o` → cola de impresion
- `cancel {job_id}` → cancelar trabajo

**Rutas API:**
| Ruta | Metodo | Funcion |
|------|--------|---------|
| `/api/printing/printers` | GET | Listar impresoras CUPS |
| `/api/printing/print` | POST | Imprimir archivo (multipart + opciones) |
| `/api/printing/jobs` | GET | Cola de impresion |
| `/api/printing/jobs/{id}` | DELETE | Cancelar trabajo |

**Structs:** `CupsPrinter` (name, description, is_default, state), `CupsPrintJob`, `PrintRequest` (printer, copies, orientation, double_sided, pages)

### Frontend

**Nueva pagina `PrintingPage.tsx`:**
- Tarjetas de impresoras CUPS detectadas con estado
- Zona de drag & drop para arrastrar archivos
- Al soltar: modal con opciones (impresora, copias, orientacion, doble cara, rango paginas)
- Cola de impresion con boton cancelar

**Modificar `FilesPage.tsx`:** Boton "Imprimir" en acciones de archivos imprimibles (pdf, doc, txt, png, jpg, etc.)

---

## Fase 3: Features complementarias (opcional, una a una)

### 3.1 Wake-on-LAN
- `POST /api/network/wol` con MAC address
- Auto-detectar MACs con `ip neigh` / `arp -n`
- Boton "Despertar" en hosts inactivos de NetworkPage

### 3.2 Media Preview
- `GET /api/files/preview?path=` sirve archivos con content-type correcto
- Modal en FilesPage: visor de imagenes, PDF inline, texto con highlight

### 3.3 Docker Monitoring
- Comunicacion via `/var/run/docker.sock`
- Pagina nueva: lista containers, start/stop/restart, logs
- Dependencia: `bollard` o HTTP sobre unix socket

---

## Cambios en navegacion (Layout.tsx)

```
Dashboard | Archivos | Impresion | Impresoras 3D | Red | Terminal | Config
```
Iconos: Printer para CUPS, Box/Layers para 3D

---

## Orden de ejecucion recomendado

1. **Fase 0** — Modularizar + persistencia (preparacion)
2. **Fase 1** — Impresoras 3D (feature principal)
3. **Fase 2** — CUPS (se integra con FilesPage)
4. **Fase 3** — WoL, Preview, Docker (incrementales)

---

## Verificacion

- Compilar backend: `cargo build` sin errores
- Compilar frontend: `npx vite build` sin errores
- Test funcional: agregar impresora 3D mock, imprimir test page via CUPS
- `./labnas.sh dev` levanta todo y las nuevas paginas cargan
