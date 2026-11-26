# Transparencia

Módulo `transparencia` (Configuración > Módulos). **Viene apagado**: publica datos hacia
afuera, así que se enciende a propósito. Apagado, `/portal-transparencia` y todo lo público
responden «no encontrado» (no «prohibido»), para no revelar que existe. Cada vez que se
enciende o se apaga queda en la bitácora inalterable.

Quien **gestiona** Transparencia configura el portal, sube informes y atiende solicitudes. El
resto de las personas con acceso puede ver la página (Ver) pero no las solicitudes, que
llevan el nombre y el correo de quien preguntó.

## Portal público (idea 70)

Dirección: `/portal-transparencia`, sin cuenta. Se genera **siempre desde los datos reales**;
nada se escribe a mano. En Transparencia > Portal se eligen las secciones y debajo se ve la
vista previa, que usa el mismo código que la página pública, antes de guardar.

| Sección | Qué muestra | De dónde sale |
|---|---|---|
| Proyectos | título, institución, fuente de financiamiento, fechas y estado | Portafolio (solo los que tienen presupuesto en Finanzas) |
| Montos | aprobado, comprometido y gastado por proyecto y categoría | Presupuesto y compromisos de Finanzas |
| Bienes | nombre, número de inventario, costo y proyecto | Inventario (los asociados a un proyecto) |
| Horas de uso | horas y sesiones por máquina y mes | Uso de máquinas (sesiones cerradas) |
| Informes | título, fecha, proyecto y archivo para descargar | Informes subidos en Transparencia > Informes |

Lo que **nunca** sale: números de serie, responsables de los bienes, nombres de quienes
usaron una máquina, ni nada de `/api/finance/files` (los informes se descargan por una ruta
propia y solo si están publicados).

- Cada cambio de secciones, plazo o introducción queda en la bitácora (antes y después).
- Los informes se suben con la huella SHA-256 del archivo. Publicar y retirar un informe
  también queda en la bitácora. Un informe retirado no se borra: deja de ser público y sigue
  guardado.

## Solicitudes de información (idea 71)

Cualquiera puede enviar una solicitud desde el portal (nombre, correo opcional y el texto).

1. Recibe un **folio** (`SI-2026-0001`) y un **código secreto** de 10 caracteres. El código
   se muestra una sola vez y en la base solo se guarda su huella: no se puede recuperar.
2. Con folio + código consulta el estado y la respuesta en el mismo portal.
3. El **plazo** es de 20 días hábiles por defecto (lunes a viernes, sin contar feriados);
   se cambia en Transparencia > Portal.
4. Quien gestiona asigna un **responsable**, y cambia el estado: *recibida*, *en curso*,
   *respondida* (con respuesta), *derivada* (indicando a quién) o *rechazada* (con motivo).
   La respuesta, el motivo y la derivación los ve quien preguntó. Una solicitud cerrada no
   se reabre.
5. Todo cambio de estado o de responsable queda en la bitácora (sin datos personales:
   solo folio y estado).

**Avisos por Telegram** a quienes gestionan Transparencia (y al responsable): al llegar una
solicitud, cuando a una le quedan 3 días hábiles o menos y cuando vence. Cada umbral avisa
una sola vez.

En la lista, el plazo va en colores: verde con holgura, amarillo a 3 días hábiles o menos,
rojo si está atrasada.

### Protección del formulario público

- Largos máximos (nombre 100, correo 120, solicitud 4.000 caracteres) y máximo 30
  solicitudes nuevas por hora en total (detrás del puente no se ve la IP real).
- No se aceptan nuevas solicitudes si hay 500 abiertas.
- Consultar con un folio y código malos da siempre el mismo «no encontrado». Tras 5 intentos
  fallidos sobre un mismo folio, ese folio se bloquea 15 minutos.
