# Detección con IA en cámaras

En **Cámaras > Detección con IA** el administrador crea reglas por cámara: qué detectar,
cuándo y qué hacer al detectar. Los operadores ven las reglas y el historial.

## Detectores

| Tipo | Qué detecta | Cómo |
|---|---|---|
| **Personas** | una persona en la imagen | YOLOX-nano (IA) |
| **Objetos** | los objetos elegidos: notebook, mochila, celular, bicicleta, perro… (80 tipos) | YOLOX-nano (IA) |
| **Movimiento** | que cambió una parte de la imagen | diferencia entre cuadros (sin IA, muy liviano; reacciona también a cambios de luz) |

Próximamente: **impresión 3D fallida** (con [Obico](https://www.obico.io/) instalado aparte)
y **modelos propios** (humo/fuego, elementos de protección…) entrenados por el laboratorio.

## Acciones al detectar

- **Foto** con lo detectado marcado, en la carpeta de grabaciones (aparece en el registro de
  fotos y grabaciones, hecha por «IA (personas)», «IA (movimiento)», etc.)
- **Telegram con foto** a los administradores que vincularon el bot
- **Clip de video** de unos segundos (si la cámara no está grabando ya)

Cada regla tiene un **horario** (por ejemplo, noches de lunes a viernes de 20:00 a 07:00; si
la hora de término es menor, la ventana termina al día siguiente), una **confianza mínima**,
cada cuántos segundos se analiza y una **pausa entre avisos** para no repetir el mismo aviso.
«Probar ahora» corre la regla sobre la imagen actual y muestra lo que detectó.

## Privacidad y transparencia

- **El análisis corre en el NAS.** Las imágenes no se envían a ningún servicio externo (la
  única salida es el aviso de Telegram, si se activa).
- Mientras una regla está activa, **todos los usuarios ven el aviso «Detección activa»**,
  igual que el de grabación.
- Cada detección, foto y clip queda en el registro y en la auditoría. El registro de
  detecciones no se borra (los archivos sí, por un administrador, y queda constancia).
- Grabar o analizar personas es tratamiento de datos personales (Ley 19.628): avise con un
  letrero en el lugar y defina con su institución quién accede a las fotos y por cuánto tiempo.

## El modelo

YOLOX-nano (Megvii, licencia Apache-2.0, ~4 MB). No viene dentro de LabNAS: se descarga
desde el release oficial de YOLOX la primera vez que se activa una regla de personas u objetos
(o con el botón «Descargar modelo»), se verifica su huella SHA-256 y se guarda en
`~/.labnas/models/`. Corre con [tract](https://github.com/sonos/tract) (Rust, sin instalar
nada más). En un PC analiza un cuadro en ~40 ms; en una Raspberry Pi tarda más, por eso cada
regla analiza un cuadro cada N segundos (2 por defecto) y nunca dos a la vez.

Si el NAS no tiene internet, copie `yolox_nano.onnx` (del release 0.1.1rc0 de YOLOX) a
`~/.labnas/models/` del usuario del servicio.
