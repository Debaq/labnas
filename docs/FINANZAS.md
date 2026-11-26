# Finanzas

Etapa 1 de la [hoja de ruta](HOJA_DE_RUTA.md): que se pueda **demostrar** cómo se mueve el
dinero de cada proyecto. Módulo `finanzas` (Configuración > Módulos), página **Finanzas**.

## Base común

- **Un proyecto es una entrada del Portafolio.** Finanzas le agrega presupuesto, compras,
  viajes y rendición; no crea proyectos aparte.
- **Todo lo financiero va a la [bitácora inalterable](BITACORA.md)** (`finance::ledger`):
  aprobaciones, pagos, recepciones, cambios de presupuesto. Nada se edita ni se borra; un
  error se corrige con un registro nuevo.
- **Lo comprometido y lo gastado** de cada flujo (compra, viaje, caja chica, honorario) se
  anota en `fin_commitments` (`finance::set_commitment`). La ejecución presupuestaria suma
  esa tabla, así cada flujo nuevo cuenta sin tocar el presupuesto.
- **Respaldos** (facturas, cotizaciones, actas, fotos) en `fin_files` con su huella SHA-256,
  que también queda en la bitácora. PDF, XML, JPEG, PNG, XLSX o DOCX, hasta 15 MB.
- **Separación de funciones:** quien pide, quien aprueba y quien recibe son personas
  distintas cuando hay dinero de por medio.

### Niveles (plantillas de rol)

| Rol | Finanzas | Transparencia |
|---|---|---|
| Investigador responsable, administrativo | gestionar | gestionar |
| Docente | usar (pide compras) | ver |
| Operador, auditor | ver | ver |
| Resto | sin acceso | ver |

## Presupuesto y ejecución

*(ideas 52, 59, 60, 64, 65)*

Pestaña **Presupuesto** de Finanzas. Se elige el proyecto (una entrada del Portafolio) arriba;
sin proyecto elegido se ve el resumen de todos.

**Presupuesto por ítem.** Cada proyecto tiene su configuración (responsable, % de alerta,
moneda) y sus ítems: categoría (personal, operación, equipos, viajes, otro), nombre, monto
aprobado y notas. Crear, editar y borrar ítems lo hace quien gestiona Finanzas. Cambiar un
monto aprobado exige **motivo** y queda en la bitácora con el antes y el después; el aprobado no
puede quedar bajo lo ya comprometido. Un ítem con compromisos o gastos no se borra. Un proyecto
**cerrado** es de solo lectura (reabrirlo exige motivo y también queda en la bitácora).

**Ejecución en vivo.** Por ítem, por categoría y por proyecto: aprobado, comprometido, gastado,
disponible y porcentaje, con barras que cambian de color (verde, ámbar desde el % de alerta,
rojo desde el 100%). Suma lo que cada flujo (compras, viajes, caja chica) registra con
`finance::set_commitment`. Al cruzar el % de alerta y al llegar al 100% se avisa por Telegram,
**una sola vez por umbral**, a quienes gestionan Finanzas y al responsable del proyecto; si el
ítem baja, el aviso se rearma. La revisión corre cada 15 minutos (tarea de fondo del módulo
Finanzas) y también al editar el presupuesto o registrar un gasto de caja.

**Bienes del proyecto.** En Inventario cada bien tiene proyecto, número de inventario (único) y
responsable. La sección «Bienes del proyecto» los lista con su costo total. También muestra las
horas de uso de máquinas cuyas sesiones declararon ese proyecto (el vínculo es el nombre del
proyecto escrito al empezar el uso; las máquinas no están ligadas a ítems de Inventario).

**Caja chica.** Un fondo por proyecto, con un responsable que lo custodia. Quien gestiona
Finanzas repone el fondo; el responsable (o quien gestiona) registra gastos con **respaldo**
(boleta o foto, con huella SHA-256) imputados a un ítem, y cuentan como *gastado*. El **arqueo**
compara el dinero contado con el saldo calculado; si hay diferencia hay que explicarla y se avisa
al equipo. Quien custodia la caja **no** puede hacer su propio arqueo. Nada se borra: un movimiento
se anula con motivo.

**Ingresos por servicios.** Monto, origen, fecha y destino, por proyecto, con respaldo opcional;
se anulan con motivo, no se borran.

**Plazos.** El cierre del proyecto ya sale de la fecha de término del Portafolio; el arqueo
pendiente de cada caja chica (último arqueo más su frecuencia) aparece en Plazos y en su
aviso por Telegram.

Todo movimiento (ítem, cambio de monto, cierre, gasto, arqueo, ingreso, anulación) queda en la
bitácora inalterable con su número de registro.

## Compras

*(ideas 53, 54, 55, 56, 58, 69)*

Pestaña **Compras** de Finanzas. Cada compra tiene un número correlativo (folio) y avanza
por estados que **no se pueden saltar**: solicitud → cotización → aprobada → orden →
recibida → facturada → pagada → rendida. También puede quedar *rechazada* o *anulada*,
siempre con motivo. Cada paso queda en la ficha (línea de tiempo), en la
[bitácora inalterable](BITACORA.md) (con su número y huella) y en la actividad.

**Quién hace qué**

| Paso | Nivel | Regla |
|---|---|---|
| Pedir (proyecto, ítem de presupuesto opcional, qué, justificación, monto estimado) | usar | la justificación es obligatoria |
| Cotizar, subir factura | quien pidió, o gestionar | cada cotización lleva su archivo |
| Aprobar, rechazar, emitir orden, confirmar factura, pagar, rendir | gestionar | quien pidió **no** aprueba ni rechaza su propia compra |
| Recibir | usar | quien recibe no es quien pidió ni quien aprobó |
| Anular | quien pidió (solo antes de aprobar) o gestionar | motivo obligatorio |

**Presupuesto.** Al aprobar queda *comprometido* el monto de la cotización elegida (o el
estimado si no hubo cotizaciones); al pagar pasa a *gastado* con el monto de la factura;
si se rechaza o anula vuelve a cero. Si el monto supera lo disponible del ítem
(aprobado − comprometido − gastado) se muestra una advertencia, pero no se bloquea.

**Cotizaciones.** Cuadro comparativo con archivo y huella. Al elegir una que no es la más
barata hay que escribir la razón. Desde un monto umbral se exige un mínimo de cotizaciones
(por defecto **3 desde $1.000.000**); quien gestiona lo cambia en *Ajustes* y no se puede
aprobar sin cumplirlo.

**Factura.** Se sube el respaldo: si es el **XML de la factura electrónica del SII** se leen
exactos tipo, folio, fecha, RUT y razón social del emisor, neto, IVA y total; si es un PDF y
la IA del NAS está configurada, propone los datos; si no, se escriben a mano. **Una persona
que gestiona Finanzas confirma** los datos antes de que la factura cuente (y la compra pasa
a *facturada* con ese monto). Una factura con el mismo RUT de emisor y folio ya registrada
se rechaza (409).

**Recepción conforme.** Acta con quién y cuándo recibió, conforme sí/no, observaciones y
foto de respaldo. Si no es conforme la compra sigue en *orden* y se avisa a quienes
gestionan. Opcionalmente crea el bien en Inventario (categoría «Bienes de proyectos»,
ligado al proyecto).

**Firma con passkey.** Con el ajuste *Exigir passkey para aprobar* (apagado por defecto), la
aprobación se firma con la passkey de quien aprueba. El desafío queda ligado a esa compra,
ese paso y ese monto, vence a los 5 minutos y sirve una sola vez: nadie puede decir «yo no
aprobé eso». Quien aprueba debe tener una passkey creada en su perfil.

**Conflicto de interés y acta.** Antes de aprobar, quien aprueba declara si tiene conflicto
con el proveedor elegido. La declaración queda guardada y en la bitácora; con conflicto no
puede aprobar esa compra (la aprueba otra persona). El botón **Acta de la decisión de gasto**
muestra la compra completa (pasos, cotizaciones, quién aprobó, declaración, huellas de los
respaldos y números de bitácora) lista para imprimir o guardar como PDF.

Avisos por Telegram: a quienes gestionan Finanzas cuando hay una solicitud nueva o lista
para aprobar, y a quien pidió cuando su compra avanza.

## Viajes y viáticos

*(idea 63)* Pestaña **Viajes** de Finanzas.

1. **Pedir** (nivel Usar): proyecto, ítem de presupuesto (opcional), quién viaja, destino,
   fechas, motivo y montos estimados de pasajes, alojamiento y viático.
2. **Aprobar o rechazar** (Gestionar): lo hace otra persona que gestione Finanzas, **nunca
   quien pidió ni quien viaja** (el servidor responde 403). Rechazar pide el motivo.
   Al aprobar, el estimado queda como *comprometido* en el ítem de presupuesto.
3. **Comprobantes**: cada uno con tipo, monto y respaldo adjunto (boleta, factura, pasaje).
   Solo el viático puede ir sin respaldo. El archivo queda con su huella SHA-256.
4. **Informe de comisión**: texto (mínimo 20 caracteres) y, si hace falta, archivos como
   «Adjunto del informe». **Sin informe no se cierra** el viaje.
5. **Cerrar**: lo gastado pasa a ser la suma de los comprobantes y lo comprometido vuelve a 0.
   Un viaje rechazado o anulado (con motivo) deja todo en 0.

La aprobación, el rechazo, la anulación y el cierre quedan en la bitácora, con las huellas de
los comprobantes en el cierre.

## Rendición y auditoría

*(ideas 61, 62, 66, 67, 68)* Página Finanzas > **Rendición y auditoría**, con cinco secciones.
Todo lo de lectura es nivel **Ver**: quien solo ve Finanzas puede mirar y descargar, nunca cambiar.

### Rendición (61)
Por proyecto (el selector de arriba) y período (desde/hasta): los **gastos con documento**
(solo documentos confirmados por una persona: tipo, folio, RUT y emisor, fecha, neto, IVA,
total, ítem de presupuesto y compra asociada) y lo **gastado contra lo aprobado** por ítem.
Se descarga en **Excel (.xlsx)**, **CSV** (abre en Excel, separador `;`), **Excel + respaldos**
(ZIP con las facturas anexas) o **versión imprimible** (en el navegador: Imprimir > Guardar
como PDF). Cada hoja trae la huella SHA-256 de su respaldo.

### Expediente (66)
Botón «Descargar expediente» (hay que elegir un proyecto): un ZIP con `indice.html` y los
CSV de compras, pasos (quién y cuándo), cotizaciones, documentos, recepciones, presupuesto,
compromisos y horas; los respaldos en `respaldos/` con nombres ordenados; `huellas.txt` con
el SHA-256 de cada archivo (se comprueba con `sha256sum -c huellas.txt`); `bitacora.jsonl`
con los registros de la [bitácora](BITACORA.md) que mencionan el proyecto, y `LEEME.txt` con
cómo verificar sin LabNAS (incluye `verificar_bitacora.py`). Cada descarga queda en la
bitácora (tipo `expediente`, con la huella del `huellas.txt`).

**Cuenta de auditor:** Configuración > Usuarios > crear o aprobar la cuenta con el rol
**Auditor** y poner la **fecha de término**. Con nivel Ver en Finanzas puede ver y descargar
la rendición y el expediente (y el resto de la pestaña), sin poder crear, aprobar ni editar
nada. Al vencer, la cuenta deja de entrar.

### Manual de procedimientos (67)
Texto (Markdown) por **versión inmutable**: una versión nueva es un registro nuevo con su
resumen de cambios; la base impide editar o borrar las anteriores. **Publicar** (quien
gestiona Finanzas) deja en la bitácora la huella SHA-256 del texto y avisa por Telegram a
quienes deben firmar. **Firmar** («confirmo que lo leí», nivel Ver) guarda fecha y queda en
la bitácora. La lista muestra quién falta de firmar la versión vigente; al publicar una
nueva, todos deben firmarla otra vez.

### Anomalías (68)
Informe de solo lectura, cada hallazgo con su explicación y enlace a la compra:
facturas **duplicadas** (mismo RUT y folio, o mismo emisor y monto en 7 días o menos),
**proveedores nuevos** (primer documento con ese RUT en los últimos 60 días), posible
**fraccionamiento** (compras al mismo proveedor o ítem en 30 días que suman más que el umbral
de cotizaciones, cada una por debajo; umbral 1.000.000 por defecto, o el ajuste de Compras si
existe), compras **pagadas sin factura confirmada**, compras **aprobadas por quien las pidió o
recibidas por quien las aprobó**, y gastos o compromisos **sobre lo aprobado**. Un hallazgo no
es una acusación: es algo que conviene mirar.

### Horas por persona y proyecto (62)
Cada persona registra sus horas del mes por proyecto, con el **contrato o la boleta de
honorarios** adjunta (obligatoria si hay monto) y el monto; quien gestiona Finanzas puede
registrar las de otros. Las aprueba quien gestiona Finanzas o el **responsable del proyecto**,
**nunca la misma persona** (ni quien las registró). Al aprobar cuenta como gastado en el ítem
de presupuesto (`honorario` en `fin_commitments`) y queda en la bitácora con la huella del
respaldo. Rechazar o anular piden motivo; anular deja de contarlo. Hay un resumen de horas y
montos por proyecto y persona.

### Para quien programa
Código en `backend/src/handlers/finance/rendicion.rs` y su carpeta `rendicion/` (`xlsx.rs`
arma el .xlsx a mano: un zip de XML, sin dependencias). Migración 40. API: `GET
/api/finance/rendicion[/export|/print]`, `GET /api/finance/expediente`, `GET
/api/finance/anomalies`, `GET /api/finance/manual[/versions/{id}|/signatures]`, `POST
/api/finance/manual/versions|sign`, `GET|POST /api/finance/hours`, `GET /api/finance/hours/summary`,
`POST /api/finance/hours/{id}/approve|reject|annul`.
