# Changelog

## [Sin publicar]

### Correo con IA

- **Proveedor de IA genérico** compatible con OpenAI (URL, modelo y clave cifrada; solo admin): DeepSeek por defecto, también Groq, OpenAI u Ollama local. Reemplaza la clave de Groq (se migra sola). Endpoints `/api/ai/config` y `/api/ai/test`; se elimina `/api/email/groq-key`
- **Propuestas de agenda**: la IA detecta eventos y tareas en cada correo nuevo y crea propuestas que el usuario aprueba (con edición), o descarta, desde la web o Telegram (`/propuestas`, `/aprobar ID`, `/descartar ID`). Las invitaciones `.ics` se leen sin IA. Nada se agenda sin aprobación
- **Correo persistente** en SQLite (antes en RAM: se perdía al reiniciar y la IA volvía a clasificar los mismos correos cada 5 minutos). Solo se descargan y analizan correos nuevos; retención de 90 días (`email_retention_days`)
- IMAP y POP3 asíncronos (ya no ocupan hilos del runtime), con timeouts, y sin cortes que hacían caer el proceso con tildes o emoji en asuntos y cuerpos

### Build

- **Sin OpenSSL**: todo el TLS saliente usa rustls (reqwest, IMAP, POP3). Corrige la compilación en distros con OpenSSL 4 y simplifica el build musl. La feature `vendored-ssl` queda sin efecto

### Seguridad

- **Impresión:** el nombre del archivo subido se reduce a su nombre base; antes un nombre con `../` escribía fuera de `/tmp` (impresión normal, dúplex y subida de G-code)
- **Descargar desde URL:** solo a internet pública. Se rechazan localhost, la LAN, Tailscale (100.64/10), link-local/metadatos y esquemas no http(s), también tras redirecciones. La descarga va en streaming a disco con el tope de `upload_limit_mb`
- **Links compartidos:** cada link tiene dueño; un usuario solo ve y revoca los suyos (el admin, todos). La descarga es en streaming (antes cargaba el archivo entero en RAM) y los vencidos se purgan. La URL ahora usa el origen con que se entró (antes `http://IP:3001` fijo)
- **Telegram `/cmd`:** ya no se cae al truncar salidas con tildes o emoji

## [3.0.0] — 2026-09-25

Desde v2.8.2. Instalación:

```bash
curl -fsSL https://github.com/Debaq/labnas/releases/latest/download/install.sh | sudo bash
```

Cambios grandes pensados para una instalación nueva. Leer **Cambios que afectan la instalación** antes de desplegar.

### Cambios que afectan la instalación

- **LabNAS ya no debe correr como root.** El servicio systemd corre como un usuario normal (`User=`) con solo `CAP_NET_RAW` (ping) y `CAP_NET_BIND_SERVICE` (puerto 80). Se ejecuta sin `sudo`; la instalación en `/opt/labnas` debe pertenecer a ese usuario para que se auto-actualice. Ver README > Instalación
- **Cuentas nuevas quedan pendientes** hasta que un admin las apruebe (el primer usuario sigue siendo admin). Contraseñas de mínimo 8 caracteres
- **Solo se accede a las "carpetas accesibles"** (por defecto el home del servicio, `/media`, `/mnt`, `/run/media`); `~/.labnas` nunca es accesible
- **Guardar `~/.labnas/secret.key`**: cifra los secretos de la base y no va en los respaldos; sin ella no se leen tokens ni contraseñas guardadas al restaurar en otra máquina
- **Sin CORS por defecto** (`LABNAS_CORS_ORIGINS` para autorizar orígenes externos)
- **El updater exige checksum SHA-256** de cada tarball (los releases anteriores a este no lo traen)

### Seguridad
- Middleware que **deniega por defecto**: toda ruta no declarada es solo admin (antes varias rutas sensibles quedaban abiertas a cualquier usuario)
- Rutas de archivos canonicalizadas (sin `..` ni symlinks que escapen) y **permisos por carpeta** (lectores/escritores por usuario, rol o "con permiso de escritura")
- Sesiones persistentes en SQLite; bloqueo de 5 min tras 5 logins fallidos; cambiar la contraseña cierra las demás sesiones
- Tickets de un solo uso para WebSockets (el token de sesión nunca va en una URL); links temporales de un archivo para vista previa y descargas
- Secretos cifrados en la base (XChaCha20-Poly1305): token del bot, API keys, contraseñas de correo
- Token por dispositivo para `/api/sensors/data` con modo compatible y opción "exigir token"
- Terminal web y `/cmd` de Telegram nunca abren un shell de root
- HTML/XML/JS se previsualizan como texto y SVG sin scripts

### Nuevo
- **Doble factor (TOTP)** por cuenta (Google Authenticator, Aegis, 2FAS…): activación con QR, 10 códigos de recuperación de un solo uso, contraseña de aplicación para WebDAV y reinicio por un admin si se pierde el teléfono. Los códigos errados cuentan para el bloqueo de login
- **Asistente de primer arranque** para el admin: carpetas compartidas y permisos, nombre mDNS, HTTPS, bot de Telegram, doble factor y recordatorio de respaldar `secret.key`; se puede omitir y reabrir desde Configuración
- **Instalador de un comando** (`curl -fsSL …/releases/latest/download/install.sh | sudo bash`): detecta arquitectura, verifica SHA-256, crea el servicio systemd sin root, abre puertos en ufw; también actualiza y desinstala
- **HTTPS** en 3443 (y 443 con permiso): certificado autofirmado automático o propio (p.ej. `tailscale cert`), recarga en caliente, descarga del certificado con huella para instalarlo, redirección HTTP→HTTPS opcional (excepto sensores, localhost y el visor)
- **Visor de escritorio** (`labnas-viewer`): ventana nativa sin navegador, notificaciones nativas, ícono en la bandeja, descubre el NAS por mDNS
- **Tiempo real** (WebSocket `/api/live`): impresoras, música, sensores y respaldos sin consultar cada pocos segundos; notificaciones en la web y en el visor
- **Respaldos programados** con snapshots incrementales (rsync `--link-dest`) + copia diaria de la base
- **Papelera** (restaurar, borrar definitivo, limpieza a 30 días)
- **Vista previa** de imágenes, video/audio (con Range), PDF y texto
- **WebDAV** en `/dav/` para montar el NAS como unidad (desactivado por defecto)
- **Auditoría persistente** con filtros (Configuración > Administración)
- **Wake-on-LAN** y listado de equipos conocidos apagados
- **Impresoras 3D**: calculadora que lee peso/tiempo del gcode, cola compartida de pedidos, timelapse con video (ffmpeg)
- **Salud de discos (SMART)** con avisos (desactivado por defecto; requiere `setup-smart.sh`)
- Rollback del updater: automático si la versión nueva no arranca, y manual desde Configuración

### Mejoras y correcciones
- Updater: elige el tarball exacto por arquitectura (antes un NAS ARM descargaba el binario x86_64), verifica que el binario nuevo arranque y reemplaza con `rename` atómico
- El reinicio tras actualizar re-ejecutaba el binario viejo (`current_exe` sigue al inode movido a `.rollback/`)
- El apagado detenía solo uno de los dos listeners (3001/80)
- `foreign_keys` y `busy_timeout` de SQLite solo aplicaban a una conexión del pool; migraciones versionadas (`user_version`)
- Monitor de impresoras 3D: consulta en paralelo, una vez para todas las pestañas; fin de impresión ya no depende de Telegram
- Calculadora de costos 3D: los campos perdían el foco en cada tecla
- Descargas y subidas en streaming (antes cargaban el archivo entero en memoria)
- Bundle inicial de la web: 1.1 MB → 319 KB (páginas y terminal bajo demanda)

### Interno
- 74 tests en Rust (unitarios + integración con el servidor completo) y tests del frontend (`npm test`), en CI junto a `clippy -D warnings`, ESLint y `shellcheck` de los scripts
- Archivos grandes del backend y la API del frontend partidos por módulo
