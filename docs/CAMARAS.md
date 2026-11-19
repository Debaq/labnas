# Cámaras en LabNAS

El módulo **Cámaras** muestra en vivo las webcams conectadas al servidor y las cámaras
IP de la red, incluidas las de las impresoras 3D. También toma fotos y graba video con
audio en una carpeta del NAS.

**Viene apagado.** Se activa en **Configuración > Laboratorio > Módulos**.

## Quién puede hacer qué

| Acción | Quién |
|---|---|
| Ver en vivo, tomar fotos, grabar y detener | Operadores y administradores |
| Agregar o editar cámaras, elegir la carpeta, borrar archivos | Administradores |
| Ver el aviso «● Grabando» | Todos los usuarios |

- Las cámaras **solo funcionan dentro de la red del laboratorio** (LAN o Tailscale), no por el puente de internet.
- Cada vista (una vez cada 30 minutos por persona y cámara), foto, grabación, descarga y borrado queda en la **auditoría**.
- El registro de fotos y grabaciones **no se borra**: si el admin borra un archivo, la fila queda marcada «borrado por …».

> ⚠️ **Audio y privacidad.** En Chile, grabar conversaciones sin el conocimiento de los
> presentes puede ser delito (art. 161-A del Código Penal). LabNAS pide confirmación antes de
> grabar y muestra el aviso a todos los usuarios, pero quienes estén en la sala deben
> saberlo. Conviene un letrero visible y avisar al empezar.

## Puesta en marcha

1. **ffmpeg** en el servidor (`sudo ./install.sh --deps`, o `pacman -S ffmpeg` / `apt install ffmpeg`).
2. **Permisos del usuario del servicio** (LabNAS no corre como root):
   ```bash
   sudo usermod -aG video,audio <usuario>   # cámara (/dev/video*) y micrófono (ALSA)
   sudo systemctl restart labnas
   ```
3. En **Cámaras > Ajustes**: la carpeta del NAS donde se guardan fotos y grabaciones (una
   subcarpeta por cámara) y la duración máxima de cada grabación (60 min por defecto).
4. **Agregar cámara:**
   - **USB:** «detectar» lista las webcams del servidor, con ruta estable (`/dev/v4l/by-id/...`)
     para que no cambie al reiniciar. El micrófono se elige de la lista (`arecord -l`, paquete `alsa-utils`).
   - **IP:** la URL RTSP (`rtsp://usuario:clave@IP:554/...`) o MJPEG (`http://...`). Va cifrada
     en la base y la clave nunca se muestra en la web.
   - Las cámaras de las **impresoras 3D** aparecen solas. Se configuran en su propio módulo.

## Cómo funciona

- Por cámara hay **un solo ffmpeg** que alimenta a todos los que miran y a la grabación.
  Una webcam USB no se puede abrir dos veces. Se apaga solo 15 s después de que nadie mire.
- La grilla muestra una imagen por segundo. «Ver en vivo» abre el video continuo a 10 cuadros por segundo.
- Las grabaciones son MP4 (H.264 + AAC) **fragmentado**: si se corta la luz, lo grabado
  hasta ese momento se puede reproducir y la fila queda «interrumpida».
- Al empezar o terminar una grabación, la vista se corta un instante (ffmpeg se reinicia con la salida nueva).
- No empieza a grabar con menos de 1 GB libre. Al llegar a la duración máxima, se detiene y avisa.
- El micrófono **solo se abre mientras se graba**. La vista en vivo no lleva audio.

## Problemas comunes

| Mensaje | Qué hacer |
|---|---|
| Sin permiso para abrir la cámara | Paso 2 (grupo `video`) |
| La cámara está en uso por otro programa | Cerrar otro programa que la use (videollamada, etc.) |
| No se encontró la cámara | Revisar el USB; volver a «detectar» |
| La grabación quedó sin audio o falla | Probar el micrófono con `arecord -D plughw:CARD=...,DEV=0 -d 3 prueba.wav` como el usuario del servicio; grupo `audio` |
| Cámara IP sin imagen | Probar la URL con `ffplay <url>` desde el servidor |
