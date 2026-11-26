# Inventario ampliado

Además del catálogo de siempre (categorías, cantidad, costo, proveedor, filamento), cada ítem
puede tener:

- **Stock mínimo**: cuando la cantidad baja de ese número llega un aviso por Telegram a quienes
  gestionan Inventario, una sola vez; al reponer se rearma. La fila muestra «Stock bajo».
- **Fecha de vencimiento** (resinas, reactivos, baterías, licencias): aparece en **Plazos**
  con los avisos de siempre (30, 7 y 1 día y el mismo día) para quienes gestionan Inventario.
- **Prestable**: cualquiera con acceso a Inventario puede **llevárselo prestado** desde la
  ficha del ítem, con fecha de devolución. Quien gestiona Inventario puede prestar cualquier
  ítem a cualquier persona.
- **Etiqueta QR**: en la ficha del ítem, **Etiqueta QR** abre una etiqueta para imprimir. Al
  escanearla se abre Inventario con ese ítem desplegado.

## Préstamos

- No se puede prestar más de lo disponible (cantidad menos lo ya prestado).
- Lo devuelve quien lo tiene, o lo **recibe** quien gestiona Inventario.
- Si se pasa la fecha, llega un aviso diario a quien lo tiene y a quienes gestionan Inventario.
- Arriba de la lista se ven los préstamos abiertos (los propios, o todos para el staff), con
  los atrasados en rojo.
- Cada préstamo y devolución queda en la auditoría.

## Retazos

Botón **Retazos** en Inventario: cada sobrante de plancha (MDF, acrílico, terciado…) con
material, espesor, medidas, dónde está y foto.

- Antes de cortar una plancha nueva, se indica la pieza que se necesita (ancho y alto) y se
  muestran los retazos que alcanzan, los más chicos primero (se pueden girar).
- Quien lo usa toca **Lo tomé** y sale de la lista.
- Lo agrega cualquiera con acceso a Inventario; lo borra quien lo subió o quien gestiona.

## Reciclaje

Botón **Reciclaje** en Inventario: se anota cada movimiento de plástico con fecha, material
y kilos, en tres tipos: **triturado**, **filamento recuperado** y **desecho** (lo que no se
pudo reciclar). Arriba se ven los totales del último año y el porcentaje reaprovechado, un
gráfico por mes y la lista de movimientos. Sirve para informes de sostenibilidad.

## Lista de materiales de KiCad

Botón **BOM KiCad** en Inventario: se sube el CSV de la lista de materiales que exporta KiCad
(Esquema → Herramientas → Generar lista de materiales) y se indica cuántas placas se van a
armar. LabNAS agrupa las líneas por valor y huella y las cruza con el Inventario buscando el
valor y el tamaño de la huella (0805, 0603…) en el nombre, modelo o descripción de cada ítem,
por ejemplo «Resistencia 10k 0805». Muestra cuántas hay disponibles (sin contar lo prestado)
y arma la **lista de compra** en CSV con lo que falta. Todo ocurre en el navegador: el CSV no
se guarda en el servidor.
