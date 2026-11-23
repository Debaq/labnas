# Plazos en LabNAS

El módulo **Plazos** muestra cuántos días faltan para cada fecha importante del
laboratorio: cierre de proyectos, informes técnicos y financieros, rendiciones,
garantías, convocatorias a fondos, entregas de tesis, vencimiento de licencias o
certificaciones.

Viene encendido. Se apaga en **Configuración > Laboratorio > Módulos**.

## De dónde salen los plazos

- **Portafolio** (automático): de cada proyecto *propio* que esté planificado, en curso o
  postulado salen su **fecha de término** («Cierre de …») y sus **hitos no cumplidos** que
  tengan fecha. Cuando el proyecto se cierra o el hito se marca cumplido, desaparecen.
  Se editan en el Portafolio.
- **Contadores sueltos**: botón **Nuevo plazo**. Nombre, tipo, fecha límite, notas,
  proyecto vinculado (opcional) y a quién avisar. Se marcan **cumplidos** (quedan en el
  historial, botón *Ver cumplidos*) y se pueden reabrir.

## Colores

| Color | Cuándo |
|---|---|
| Verde | Faltan más de 30 días |
| Amarillo | 30 días o menos |
| Rojo | 7 días o menos, o ya venció (muestra los días de atraso) |

El **dashboard** muestra una tarjeta con los plazos vencidos y los de los próximos 60
días (hasta 6).

## Avisos por Telegram

Llegan a los **30, 7 y 1 día** antes y **el mismo día**, entre las 9:00 y las 21:00. Cada
aviso sale una sola vez; si se cambia la fecha, los avisos vuelven a empezar. Un plazo
creado con menos de 30 días de margen avisa una vez al crearse y después en los
siguientes umbrales.

| Plazo | Quién recibe el aviso |
|---|---|
| Contador suelto, «Solo a mí» | Quien lo creó |
| Contador suelto, «Operadores y administradores» | Quien lo creó, operadores y administradores |
| Contador suelto, «Todo el equipo» | Todos los chats aprobados del bot |
| Del Portafolio | Operadores, administradores y participantes del proyecto |

Los avisos personales llegan a los chats **vinculados a la cuenta web** (Configuración >
Notificaciones); además aparecen en la web.

## Permisos

Todos los usuarios aprobados ven los plazos y crean contadores. Editar, marcar cumplido o
eliminar un contador: quien lo creó, operadores y administradores. Todo queda en la
auditoría.
