# Escáner en LabNAS

LabNAS escanea con **SANE** (`scanimage`) usando el escáner conectado al servidor,
por USB o por red. Se usa desde la web: **Impresión > Escanear**. Así se puede
escanear desde cualquier equipo (Windows, Linux, Mac o celular) sin instalar drivers
en él.

- Se escanea página por página. Al terminar se descarga o se guarda en una carpeta del
  NAS como **PDF** (todas las páginas) o **JPG** (una imagen por página). Al guardar se
  respetan los permisos de carpeta de cada usuario.
- Pide el mismo permiso que imprimir y escanea un trabajo a la vez.
- Cada escaneo y cada guardado quedan en la auditoría.

## 1. Instalar SANE

```bash
sudo ./install.sh --deps                      # o a mano:
sudo pacman -S sane                           # Arch
sudo apt install sane-utils                   # Debian / Ubuntu
```

Los escáneres de red con AirScan/eSCL (casi todos los multifuncionales modernos)
funcionan con el paquete `sane-airscan` y no necesitan nada más.

## 2. Driver de la Brother DCP-165C (USB)

Esta multifuncional usa el driver **brscan3** de Brother:

```bash
# Arch (AUR)
yay -S brscan3

# Debian / Ubuntu: descargar desde el sitio de soporte de Brother (modelo DCP-165C,
# Linux, paquete .deb) "brscan3" y "udev rules", e instalar:
sudo dpkg -i --force-all brscan3-*.amd64.deb
sudo dpkg -i --force-all brother-udev-rule-type1-*.all.deb
```

Revisar que SANE la vea (con la impresora encendida y el USB conectado):

```bash
scanimage -L
# device `brother3:bus2;dev1' is a Brother DCP-165C USB scanner
```

## 3. Permiso para el usuario del servicio

LabNAS **no corre como root**: el usuario del servicio necesita acceso al USB del escáner.
Probar como ese usuario (el mismo que aparece en `systemctl status labnas`):

```bash
sudo -u <usuario> scanimage -L
```

Si no aparece el escáner (o la web dice «no tiene permiso»), dar acceso por grupo:

```bash
sudo groupadd -f scanner
sudo usermod -aG scanner <usuario>
# 04f9 = Brother. Para otra marca, ver el ID con lsusb
echo 'SUBSYSTEM=="usb", ATTRS{idVendor}=="04f9", MODE="0664", GROUP="scanner"' | \
  sudo tee /etc/udev/rules.d/60-labnas-escaner.rules
sudo udevadm control --reload && sudo udevadm trigger
sudo systemctl restart labnas
```

## Si `scanimage -L` no muestra la Brother

(Las webcams también aparecen en `scanimage -L`; LabNAS las oculta.)

```bash
lsusb | grep -i brother             # ¿el sistema ve el USB? (ID 04f9:...)
brsaneconfig3 -q | grep -i 165      # ¿brscan3 instalado y reconoce el modelo?
grep -n brother3 /etc/sane.d/dll.conf   # el backend debe estar habilitado
sudo scanimage -L                   # como root: si aquí aparece y como usuario no, es permiso (paso 3)
```

- `brsaneconfig3: command not found`: falta el driver (paso 2).
- `dll.conf` sin la línea `brother3`: agregarla con `echo brother3 | sudo tee -a /etc/sane.d/dll.conf`.
- Con root aparece y con el usuario del servicio no: paso 3, y reiniciar el servicio.

## Problemas comunes

| Mensaje en la web | Qué hacer |
|---|---|
| SANE (scanimage) no está instalado | Paso 1 |
| No se encontró ningún escáner | Encender la impresora, revisar el USB, `scanimage -L`; botón ↻ en la web para buscar de nuevo |
| LabNAS no tiene permiso para usar el escáner | Paso 3 |
| El escáner está ocupado | Hay otro escaneo en curso (de otra persona o de otro programa); esperar |
| El escaneo tardó demasiado | Bajar la resolución; 300 ppp alcanza para documentos |

Una hoja carta a 300 ppp en color pesa entre 0,5 y 1,5 MB en el PDF. Para documentos
de texto conviene el modo gris.

## «sane_start: Invalid argument»

Algunos drivers declaran un área más larga que su vidrio (brscan3 en la DCP-165C informa
215,9 × 355,6 mm, Legal, pero el vidrio es A4). Si se escanea el área máxima, `sane_start`
falla. LabNAS escanea **Carta** por defecto y permite elegir el tamaño de hoja; si aparece
el error, elige Carta o A4. Desde la terminal: `scanimage -d '<dispositivo>' -x 215.9 -y 279.4 > prueba.pnm`.
