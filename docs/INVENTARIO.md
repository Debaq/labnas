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
