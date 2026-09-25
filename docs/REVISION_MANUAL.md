# Revisión manual antes del release

Lo automatizado (64 tests de Rust, tests del frontend, clippy, ESLint, build) ya pasa en CI. Esta lista cubre lo que **no se ha visto en pantalla**: UI, navegador real, visor y hardware. Marcar cada punto; si algo falla, anotar qué se esperaba y qué pasó.

## Preparación

```bash
./labnas.sh build
cd backend/target/release && ./labnas-backend          # sin sudo
```

Base limpia: `LABNAS_HOME=/tmp/labnas-prueba ./labnas-backend` (no toca `~/.labnas`). Tener dos navegadores (o una ventana privada) para probar con dos usuarios a la vez.

## 1. Cuentas y sesión
- [ ] Primer registro → queda admin y entra directo
- [ ] Segundo registro (otro navegador) → ve "Cuenta pendiente de aprobación"
- [ ] Admin aprueba (Configuración > Usuarios) → el pendiente entra **solo, sin recargar** (evento `auth.changed`)
- [ ] Contraseña de 7 caracteres → rechazada con mensaje
- [ ] 5 logins fallidos → el 6.º dice "Demasiados intentos… espera N min"
- [ ] Reiniciar el backend → la sesión sigue abierta

## 2. Archivos
- [ ] El explorador abre en el home; "/" muestra solo las carpetas accesibles
- [ ] Subir (botón y arrastrando), crear carpeta, descargar (archivo grande: no se congela la pestaña)
- [ ] Clic en imagen / video (adelantar) / PDF / texto → vista previa; Esc cierra
- [ ] Un `.html` subido se ve como texto, no se ejecuta
- [ ] Borrar → "Mover a la papelera"; Papelera → restaurar (con sufijo si ya existe) y borrar definitivo; admin puede vaciar
- [ ] Compartir link → abre desde otro equipo sin sesión

## 3. Permisos por carpeta (Configuración > Sistema)
- [ ] Agregar una carpeta y dejar "Leen: Rol operador", "Escriben: usuario X"
- [ ] Un observador no la ve en "/" ni puede abrirla; el operador la ve pero no sube; el usuario X sube
- [ ] Quitar la última carpeta está bloqueado; restaurar predeterminadas funciona

## 4. Tiempo real y notificaciones
- [ ] Con dos pestañas: cambiar volumen/pista de música en una → la otra se actualiza al instante
- [ ] Detener el backend unos segundos → la web sigue (vuelve a consultar) y reconecta sola al volver
- [ ] Aparecen toasts: usuario pendiente (admin), respaldo terminado, pedido de impresión
- [ ] Terminal web: abre, escribe, redimensiona; tras ir a otra página y volver, la sesión sigue

## 5. Respaldos (Configuración > Respaldos)
- [ ] Crear tarea (origen en una carpeta accesible, destino en otro disco) → "Ejecutar ahora" → estado OK
- [ ] Ver copias; en el disco destino hay `AAAA-MM-DD_HHMMSS/` y `latest`; una 2.ª copia sin cambios casi no ocupa espacio (`du -sh`)
- [ ] Destino sin permiso → estado Error con mensaje y aviso por Telegram

## 6. Impresoras 3D
- [ ] Estado en vivo de impresoras reales (temperaturas, progreso)
- [ ] Calculadora: escribir en los campos **no pierde el foco**; "Cargar GCODE" llena peso y horas (probar un gcode de PrusaSlicer/Orca y uno de Cura)
- [ ] Cola: un observador pide; el operador sube/baja, marca "Imprimiendo" y "Terminado" → el observador recibe aviso
- [ ] Timelapse: activar en una impresora con cámara, imprimir algo corto → aparece `Timelapses/<impresora>/<fecha>/timelapse.mp4` y llega el aviso

## 7. Red, sensores, SMART
- [ ] Etiquetar un equipo, apagarlo, re-escanear → sigue listado apagado con su IP; "Encender" lo despierta (si tiene WoL activado en BIOS)
- [ ] Sensores: generar token de un dispositivo, configurarlo en el firmware; activar "Exigir token" → los que no tienen token dejan de registrar
- [ ] SMART: activar sin setup → muestra cómo configurarlo; tras `sudo bash setup-smart.sh <usuario>` → estado de cada disco

## 8. WebDAV (Configuración > Sistema)
- [ ] Activar; en Linux: Archivos > Otras ubicaciones > `dav://IP:3001/dav/` con usuario y contraseña de la web
- [ ] Se ven solo las carpetas permitidas; copiar un archivo, crear carpeta, renombrar; borrar → aparece en la papelera de LabNAS

## 9. Visor de escritorio
- [ ] `./viewer/install.sh` → aparece "LabNAS" en el menú de aplicaciones con ícono
- [ ] Sin URL configurada y con mDNS activo en el servidor (Configuración > Red) → abre el NAS solo (`labnas-viewer --buscar` lo muestra)
- [ ] Ícono en la bandeja; cerrar la ventana la oculta; llegan notificaciones nativas; "Salir" cierra
- [ ] Descargar un archivo → queda en `~/Descargas` con notificación; un link externo abre el navegador del sistema

## 10. HTTPS (Configuración > Sistema)
- [ ] Abrir `https://IP:3443`: el navegador avisa (autofirmado); descargar el certificado, comparar la huella e instalarlo → ya no avisa
- [ ] Login, terminal y música funcionan por HTTPS (WebSocket `wss://`)
- [ ] Activar "Redirigir", reiniciar: `http://IP:3001` lleva a HTTPS; `http://localhost:3001` y el visor siguen por HTTP; los sensores siguen enviando
- [ ] (Opcional) Certificado de Tailscale: cargar rutas → el navegador ya no avisa, sin reiniciar

## 11. Actualización (necesita un release real)
- [ ] Desde la versión anterior instalada, "Actualizar" → descarga, verifica checksum, reinicia en la nueva
- [ ] "Volver a vX" → vuelve a la anterior y la sesión sigue abierta
