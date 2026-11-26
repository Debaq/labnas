# Máquinas en LabNAS

El módulo **Máquinas** es el catálogo de los equipos del laboratorio (láser, CNC,
impresoras 3D, estaciones de electrónica…). Cada máquina tiene una ficha con foto,
ubicación, responsable e instrucciones de uso, y un **QR** para pegar en ella. Al
escanearlo con el celular se abre la ficha, donde se puede **reservar** o **reportar una falla**.

Viene encendido. Se apaga en **Configuración > Laboratorio > Módulos**.

## Quién puede hacer qué

| Acción | Quién |
|---|---|
| Ver máquinas y reservas, reservar, cancelar sus reservas, reportar fallas, pedir la inducción | Todos los usuarios aprobados |
| Crear y editar máquinas, cambiar la foto, imprimir el QR | Operadores y administradores |
| Aprobar o rechazar reservas, bloquear horarios, cancelar reservas ajenas, marcar fallas reparadas | Operadores y administradores |
| Dar, renovar o quitar inducciones | Operadores y administradores |
| Armar el plan de mantención y fijar las horas de uso | Operadores, técnicos y administradores |
| Registrar mantenciones en la bitácora | El responsable de la máquina, operadores, técnicos y administradores |
| Eliminar una máquina (con sus reservas, fallas y bitácora) | Administradores |

Todo queda en la **auditoría** (Configuración > Sistema).

## Reservas

- No se pueden cruzar: si el horario choca, LabNAS dice con quién.
- Se reserva desde ahora hasta **90 días** adelante. Cada máquina puede tener un
  **máximo de horas por reserva** (los operadores pueden pasarse).
- Si la máquina tiene **«requiere aprobación»**, la reserva queda *por aprobar* (igual
  ocupa el horario) y los operadores reciben un aviso por Telegram. Al aprobarla o
  rechazarla, la persona recibe un aviso.
- Los operadores pueden **bloquear un horario** (mantención, clase) para que nadie reserve.
- **15 minutos antes** de cada reserva confirmada llega un recordatorio por Telegram.

## Inducción

Al editar una máquina se puede marcar **«Exigir inducción para reservar»**. Desde ese
momento solo reserva quien tiene la inducción vigente de esa máquina; operadores y
administradores no la necesitan.

1. La persona entra a la ficha y toca **Pedir inducción** (con una nota opcional:
   cuándo puede, para qué la necesita). El responsable de la máquina, los operadores y
   los administradores reciben un aviso por Telegram y en la web.
2. Después de hacerla, un operador la marca como hecha en la ficha o en la pestaña
   **Por aprobar** («Ya la hizo»). Puede ponerle **fecha de vencimiento**; sin fecha no
   vence. La persona recibe un aviso.
3. En la ficha el staff ve quién tiene la inducción, quién la pidió y quién la tiene
   vencida, y puede darla directamente a alguien, renovarla o quitarla.

Los vencimientos aparecen en **Plazos** (con sus avisos a 30, 7 y 1 día y el mismo día) y
desaparecen 30 días después de vencer. Una inducción vencida no deja reservar: hay que
pedirla de nuevo.

## Fallas

1. Cualquiera reporta la falla desde la ficha, con una foto si quiere (en el celular se
   abre la cámara).
2. La máquina queda **fuera de servicio**: no admite reservas nuevas. Los operadores y
   administradores reciben un aviso por Telegram (con la foto), y quienes ya tenían
   reservas, otro aviso.
3. Un operador la **marca reparada** con una nota de lo que se hizo. Si no quedan otras
   fallas abiertas, la máquina vuelve a estar disponible y se avisa a quien reportó.

El historial de fallas queda en la ficha.

## Mantención y calibración

Cada máquina tiene en su ficha un **plan** de tareas que se repiten y una **bitácora**.

- **Tareas del plan** (las arma un operador o técnico): por ejemplo «Limpiar el lente»
  cada 40 horas de uso o «Calibrar la cama» cada 30 días; pueden ser por fecha, por horas
  o por lo que pase primero. Cada una muestra cuánto falta: *al día*, *pronto* o *vencida*.
  - Las de **fecha** aparecen en **Plazos**, con sus avisos a 30, 7 y 1 día y el mismo día.
  - Las de **horas de uso** avisan por Telegram al vencer, al responsable de la máquina y a
    quienes gestionan Máquinas.
  - Una tarea puede **bloquear las reservas** mientras esté vencida (revisiones de
    seguridad); el staff igual puede reservar, por ejemplo para hacer la mantención.
- **Registrar**: el responsable de la máquina, un operador o un técnico toca **Hecho** en la
  tarea (con nota y foto opcionales) y la tarea vuelve a contar desde ese momento. También se
  puede **anotar en la bitácora** algo suelto: una calibración, una reparación, una nota.
- **Bitácora**: quién hizo qué, cuándo y con cuántas horas de uso tenía la máquina. No se
  edita ni se borra; quitar una tarea del plan conserva lo registrado. Las fallas reparadas
  entran solas.
- **Horas de uso**: se suman las sesiones de uso reales ya terminadas (ver «Registro de
  uso») y las reservas confirmadas ya terminadas que no tengan una sesión ligada (si la
  tienen, manda la sesión, para no contar doble). Si la máquina tiene horómetro, un operador
  lo lee y lo **fija** en la ficha; desde ahí se siguen sumando las sesiones y reservas.
  Cuando exista el medidor de corriente (idea 19) se usará su lectura.

La pestaña **Mantención** de Máquinas (staff) junta las tareas de todas las máquinas, las
vencidas primero.

## Registro de uso

En la ficha (la que abre el QR), el bloque **Uso** tiene un botón grande **Empezar a usar** y,
cuando ya empezó, **Terminar** (con proyecto y nota opcionales). Queda una sesión con quién,
desde cuándo y hasta cuándo; la tarjeta de la máquina muestra «En uso por …» mientras haya una
abierta.

- Una sola sesión abierta por máquina.
- No se puede empezar si la máquina está fuera de servicio o retirada, si exige inducción y no
  la tienes, o si hay una mantención vencida que bloquea. El staff sí puede (salvo con la
  máquina fuera de servicio).
- Si tienes una reserva en curso, la sesión se liga a ella.
- Termina la sesión quien la empezó o el staff.
- Una sesión abierta más de 12 h se cierra sola, con la nota «cerrada automaticamente» (cuenta
  solo 12 h) y se avisa por Telegram a quien la tenía.
- La pestaña **Horas de uso** de Máquinas (staff) da el informe por rango de fechas: horas por
  máquina, por persona y por proyecto.

Endpoints: `GET /api/machines/{id}/usage` (sesión abierta y recientes),
`POST /api/machines/{id}/usage/start`, `POST /api/machines/usage/{sid}/stop` (ambos con nivel
Usar) y `GET /api/machines/usage/report?from=&to=` (staff). `PUT /api/machines/{id}/usage`
sigue siendo el horómetro.

## Parámetros de corte

En la ficha de cada máquina, **Parámetros de corte** guarda lo que funcionó: material,
espesor, operación (corte, grabado, marcado, fresado), potencia, velocidad, pasadas, notas
(foco, aire, orden) y una foto del resultado. Así nadie tiene que probar de cero.

- Lo agrega cualquiera que use Máquinas; lo edita o borra quien lo subió o quien gestiona.
- Los demás marcan **me funcionó** (👍); los buenos y más votados salen primero, y los de
  resultado «malo» quedan como advertencia.
- La pestaña **Parámetros de corte** de Máquinas busca por material en todas las máquinas.

## Etiqueta QR

En la ficha, **Etiqueta QR → Imprimir etiqueta**. El QR lleva la dirección con que se
abrió LabNAS en el navegador: si la abriste como `localhost`, entra primero con la IP o el
nombre del servidor (por ejemplo `https://192.168.50.114:3443`) para que el QR sirva desde
otros equipos. Si no hay sesión abierta, el QR lleva al inicio de sesión y después a la ficha.

## Avisos por Telegram

Llegan a los chats **vinculados a la cuenta web** de cada persona (Configuración >
Notificaciones). Sin chat vinculado, la persona solo ve el aviso en la web.

## Lo que viene

Este módulo es la base de varias ideas de la hoja de ruta: llave electrónica que solo
habilita a quien tiene la inducción (18), horas de uso medidas con un medidor de corriente
(19) y una sola cola para todas las máquinas (23).
