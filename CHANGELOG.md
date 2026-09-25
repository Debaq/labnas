# Changelog

## [Sin publicar] — desde v2.8.2

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
- 64 tests en Rust (unitarios + integración con el servidor completo) y tests del frontend (`npm test`), en CI junto a `clippy -D warnings` y ESLint
- Archivos grandes del backend y la API del frontend partidos por módulo
