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
| Eliminar una máquina (con sus reservas y fallas) | Administradores |

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
habilita a quien tiene la inducción (18), mantenimiento por horas de uso (5) y una sola
cola para todas las máquinas (23).
