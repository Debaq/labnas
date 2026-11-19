# Detección con IA en cámaras

En **Cámaras > Detección con IA** el administrador crea reglas por cámara: qué detectar,
cuándo y qué hacer al detectar.

**Es vigilancia, así que es privada:** solo la ven los administradores y las personas que un
admin autorice en «Quién puede ver» (por ejemplo, investigadores fijos). Eso incluye las
reglas, el historial y las fotos y clips que hace la IA: los demás usuarios no los ven en el
registro de grabaciones ni los pueden descargar. Las personas autorizadas necesitan rol de
operador para abrir la página de Cámaras.

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
- **Telegram con foto** a los administradores (chats con rol admin o vinculados a una cuenta
  de admin) y, si la regla lo indica, también a las personas autorizadas
- **Clip de video** de unos segundos (si la cámara no está grabando ya)

Cada regla tiene un **horario** (por ejemplo, noches de lunes a viernes de 20:00 a 07:00; si
la hora de término es menor, la ventana termina al día siguiente), una **confianza mínima**,
cada cuántos segundos se analiza y una **pausa entre avisos** para no repetir el mismo aviso.
«Probar ahora» corre la regla sobre la imagen actual y muestra lo que detectó.

## Privacidad

- **El análisis corre en el NAS.** Las imágenes no se envían a ningún servicio externo (la
  única salida es el aviso de Telegram, si se activa).
- **Acceso restringido**: solo administradores y personas autorizadas. Quién está autorizado
  y cada cambio de esa lista quedan en la auditoría.
- Cada detección, foto y clip queda en el registro y en la auditoría. El registro de
  detecciones no se borra (los archivos sí, por un administrador, y queda constancia).
- La web no muestra un aviso de «detección activa» a los demás usuarios. Por eso el **letrero
  físico** en el lugar es lo que informa a las personas: analizar personas es tratamiento de
  datos personales (Ley 19.628). Defina con su institución quién accede a las fotos y por
  cuánto tiempo se guardan.

## El modelo

YOLOX-nano (Megvii, licencia Apache-2.0, ~4 MB). No viene dentro de LabNAS: se descarga
desde el release oficial de YOLOX la primera vez que se activa una regla de personas u objetos
(o con el botón «Descargar modelo»), se verifica su huella SHA-256 y se guarda en
`~/.labnas/models/`. Corre con [tract](https://github.com/sonos/tract) (Rust, sin instalar
nada más). En un PC analiza un cuadro en ~40 ms; en una Raspberry Pi tarda más, por eso cada
regla analiza un cuadro cada N segundos (2 por defecto) y nunca dos a la vez.

Si el NAS no tiene internet, copie `yolox_nano.onnx` (del release 0.1.1rc0 de YOLOX) a
`~/.labnas/models/` del usuario del servicio.
