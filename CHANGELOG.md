# Changelog

## [Sin publicar]

### Bitácora inalterable

- **Configuración > Admin > Bitácora inalterable**: registros encadenados con huella SHA-256 que no se pueden editar ni borrar (triggers en la base); un error se corrige con un registro de **anulación** con motivo. Base de la trazabilidad financiera (idea 57 de la hoja de ruta)
- **Notas con huella de archivos del NAS** (facturas, actas, fotos) y botón para comprobar si siguen iguales
- **Verificación** de la cadena completa, **exportación** `.jsonl` verificable sin LabNAS con un script de Python incluido en `docs/BITACORA.md`
- **Ancla externa**: cada día, si la bitácora creció, la huella final va a los admins por Telegram; botón «Anclar» con texto para copiar en un correo. Si la cadena aparece alterada o recortada, avisa por Telegram y en la web y lo deja en la auditoría
- Migración 16 (`ledger_entries`). Solo admin y no disponible por el puente de internet

### IA

- El **proveedor de IA y su API key** se configuran en **Configuración > Sistema** (antes estaban dentro de Correo y no se podían configurar con ese módulo apagado). Es una sola configuración para todos los módulos

### Cámaras

- **Módulo Cámaras** (viene apagado; se activa en Configuración > Laboratorio): webcams USB del servidor y cámaras IP (RTSP o MJPEG) de la red, más las cámaras de las impresoras 3D, que aparecen solas. Grilla con imagen cada segundo, vista en vivo, **fotos** y **grabaciones con audio** (MP4 fragmentado, reproducible aunque se corte) en una carpeta del NAS, con duración máxima configurable
- Un solo ffmpeg por cámara alimenta a todos los que miran y a la grabación; el micrófono solo se abre mientras se graba
- **Transparencia:** ven y graban operadores y admin; **todos los usuarios ven el aviso «● Grabando»** con quién graba; cada vista, foto, grabación, descarga y borrado queda en la auditoría; el registro de grabaciones no se borra (el archivo sí, y queda «borrado por …»). Las cámaras no se exponen por el puente de internet
- URL de cámaras IP cifrada en la base (migración 15) y nunca mostrada con la clave. Guía y permisos (grupos `video` y `audio`) en `docs/CAMARAS.md`
- **Los módulos pueden venir apagados por defecto** (lo usa Cámaras)

### Escáner

- La lista de escáneres ya no muestra las webcams (backend `v4l` de SANE)

## [3.1.0] — 2026-11-19

Desde v3.0.0.

### Escáner desde la web

- **Impresión > Escanear**: LabNAS escanea con SANE (`scanimage`) usando el escáner conectado al servidor (USB o red), así se escanea desde Windows, Linux o el celular sin drivers. Página por página con vista previa; se descarga o se guarda en una carpeta del NAS (respeta los permisos de carpeta, sin pisar archivos) como **PDF** o **JPG**
- El PDF se arma en LabNAS (JPEG incrustado al tamaño real escaneado), sin dependencias externas; acepta blanco y negro, gris y color de 8 o 16 bits
- Pide el permiso de impresión, escanea un trabajo a la vez y deja cada escaneo y guardado en la auditoría. Modo, resolución y origen se validan contra lo que reporta el escáner
- `install.sh --deps` instala SANE; guía para la Brother DCP-165C (brscan3) y permisos USB en `docs/ESCANER.md`

### Impresora de red (IPP)

- **Imprimir desde Windows, Linux o macOS pasando por LabNAS**: con el módulo de impresión encendido, LabNAS actúa como impresora IPP en el puerto 8631 (HTTP, solo LAN). Cada usuario genera en Impresión su **dirección personal** por impresora y la agrega en Windows con «Seleccionar una impresora compartida por nombre»; Windows usa su driver genérico (IPP Class Driver), sin instalar drivers
- El token de la dirección identifica al usuario: el trabajo respeta el permiso de impresión, queda en la auditoría (con la IP del equipo) y se cuenta en estadísticas y costos a su nombre. Se puede regenerar o revocar; se guarda como hash más una copia cifrada (migración 14)
- Acepta PDF, PWG raster (lo que manda Windows), JPEG y PostScript; cuenta las páginas del PWG raster. Atributos IPP Everywhere (papel carta, oficio, A4…, color y dúplex según el PPD) validados con `ipptool`
- `install.sh` abre el puerto 8631 en `ufw`; `LABNAS_IPP_PORT` lo cambia

### Snapshots btrfs/ZFS y servicio endurecido

- **Snapshots programados** de subvolúmenes btrfs o datasets ZFS (Configuración > Respaldos > Snapshots): frecuencia, retención y botón «Ahora». Sin root: `setup-snapshots.sh` instala un wrapper que solo actúa sobre las rutas elegidas y solo toca snapshots `labnas-*`, con una regla de sudo limitada a él
- **Servicio systemd endurecido** (`PrivateTmp`, `ProtectKernel*`, `ProtectClock`, `RestrictNamespaces`, `LockPersonality`, `RestrictAddressFamilies`…), elegido para no romper la terminal con sudo, SMART, música ni CUPS. `install.sh --no-hardening` lo desactiva

### Passkeys y releases firmados

- **Passkeys (WebAuthn)**: iniciar sesión sin contraseña con huella, Face ID, Windows Hello o una llave de seguridad (Configuración > General). Verificación propia con criptografía pura en Rust (ES256 y RS256), sin OpenSSL; desafíos de un solo uso, control de origen y de contador de firmas. Requieren HTTPS y un nombre de dominio: funcionan por el puente o por `https://labnas.local` con el certificado instalado
- **Releases firmados con minisign**: si el release se compila con `MINISIGN_PUBKEY`, el updater exige una firma válida **antes de extraer o ejecutar** el paquete, e `install.sh` la verifica si está `minisign`. El workflow firma tarballs e instalador cuando existe el secreto `MINISIGN_SECRET_KEY`
- El updater ya no instala versiones anteriores a la actual

### Sesiones más seguras

- **La sesión web va en una cookie HttpOnly + SameSite=Strict** (Secure por el puente): el token ya no se guarda en `localStorage`, así que un XSS no puede robarlo. Con sesión por cookie, los métodos que modifican exigen la cabecera `X-LabNAS-CSRF`. El token Bearer sigue funcionando para clientes y scripts
- Un 401 en cualquier pantalla devuelve al login (antes mostraba listas vacías)
- Tras actualizar hay que **volver a iniciar sesión una vez** (se descarta la sesión guardada en el formato viejo)

### Respaldos con restic

- Tareas de respaldo tipo **restic**: repositorio cifrado y deduplicado en disco local o remoto (SFTP, S3, B2, REST...). Contraseña y credenciales cifradas en la base (migración 12), retención diaria, **verificación** de integridad y **restauración** a `<origen>/.labnas-restore/` sin pisar archivos. Las tareas rsync siguen igual
- Los snapshots restic con base de datos incluyen también `secret.key` (cifrada en el repositorio): sin ella los secretos de una base restaurada no se pueden descifrar. La restauración web trae solo la carpeta respaldada; la base y la llave se recuperan por terminal
- `install.sh --deps` instala también `restic`

### Acceso remoto: puente PHP (etapas B1–B4)

- **Conector PHP** para hosting compartido (PHP 7.0 a 8.x, Apache) que permite usar LabNAS desde internet **sin abrir puertos**: LabNAS sale hacia el hosting, recoge los pedidos (long-poll) y devuelve las respuestas; todo el canal va firmado con HMAC-SHA256. Diseño en `docs/PUENTE_PHP.md`
- Enlace con código de un solo uso (`instalar.php`); los datos del conector van fuera de `public_html`. LabNAS entrega el paquete (`GET /api/bridge/package`: PHP + frontend de la misma versión)
- Por el puente se niegan la terminal, los WebSocket, el registro de cuentas y todo lo de admin; los inicios de sesión quedan en la auditoría con la IP real y no se revela la IP de la LAN
- **Archivos de cualquier tamaño** (B2): las respuestas viajan en partes del tamaño que acepta el hosting (con contrapresión y `Content-Length`, así funcionan video y descargas grandes) y las subidas grandes usan el nuevo `POST /api/files/upload/chunked` (también fuera del puente; reanudable)
- **El conector se actualiza solo** (B3) cuando LabNAS cambia de versión: manifiesto firmado, verificación sha256 de cada archivo y reemplazo con vuelta atrás; `index.php` y `.htaccess` nunca se tocan. Botón «Actualizar conector»
- **Tiempo real sin WebSocket** (B4): `GET /api/live/poll` (long-poll del mismo bus de eventos); el frontend lo usa por el puente o si el WebSocket no conecta. Aviso «nuevo inicio de sesión por internet» en las otras sesiones del usuario
- La subida normal ya no sigue symlinks (temporal `create_new` + rename)

### Correo con IA

- **Proveedor de IA genérico** compatible con OpenAI (URL, modelo y clave cifrada; solo admin): DeepSeek por defecto, también Groq, OpenAI u Ollama local. Reemplaza la clave de Groq (se migra sola). Endpoints `/api/ai/config` y `/api/ai/test`; se elimina `/api/email/groq-key`
- **Propuestas de agenda**: la IA detecta eventos y tareas en cada correo nuevo y crea propuestas que el usuario aprueba (con edición), o descarta, desde la web o Telegram (`/propuestas`, `/aprobar ID`, `/descartar ID`). Las invitaciones `.ics` se leen sin IA. Nada se agenda sin aprobación
- **Correo persistente** en SQLite (antes en RAM: se perdía al reiniciar y la IA volvía a clasificar los mismos correos cada 5 minutos). Solo se descargan y analizan correos nuevos; retención de 90 días (`email_retention_days`)
- **Cuentas Microsoft (Outlook, Hotmail, Microsoft 365) con OAuth2**: inicio de sesión por código de dispositivo (sin URL pública), refresh token cifrado, IMAP con XOAUTH2. El admin registra una app gratuita en Microsoft Entra y guarda su Client ID (`/api/email/oauth/microsoft/*`). Microsoft ya no acepta contraseñas por IMAP/POP
- La bandeja muestra el error de la última revisión (p. ej. login rechazado) con opción de reconectar; `GET /api/email/account`. Corrige que la página creyera que siempre había cuenta configurada
- IMAP y POP3 asíncronos (ya no ocupan hilos del runtime), con timeouts, y sin cortes que hacían caer el proceso con tildes o emoji en asuntos y cuerpos

### Build

- **Sin OpenSSL**: todo el TLS saliente usa rustls (reqwest, IMAP, POP3). Corrige la compilación en distros con OpenSSL 4 y simplifica el build musl. La feature `vendored-ssl` queda sin efecto

### Seguridad

- **Tareas de fondo en vivo**: encender o apagar un módulo inicia o detiene al momento su revisión de correo, monitor de impresoras 3D, receptor de sensores, música o escaneo de red; antes solo se aplicaba al reiniciar LabNAS. Apagar Música detiene la reproducción
- **Módulos apagados también por Telegram**: antes el bot seguía respondiendo comandos de módulos desactivados (incluido `/cmd` con el módulo Terminal apagado). Ahora responde que el módulo está desactivado, `/ayuda` oculta sus comandos y apagar la Terminal cierra las sesiones abiertas desde Telegram
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
