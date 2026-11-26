# Talleres: inscripción, asistencia y certificados

Se montan sobre las fichas del Portafolio de tipo **taller** o **curso**. Al expandir la
ficha aparece la sección «Inscripciones». El código está en `backend/src/handlers/workshops.rs`.

## Inscripción

- **Con sesión:** el botón «Inscribirme» (nivel Usar en Portafolio). La persona puede cancelar
  su inscripción desde el mismo lugar.
- **Pública:** quien gestiona el taller copia el enlace secreto `/taller/<token>` y lo comparte.
  Sin cuenta, la persona deja nombre, correo y afiliación o carrera.
- **Cupo:** pasado el cupo (0 = sin límite) la persona queda en **lista de espera**. Si alguien
  cancela, o se sube el cupo, el primero de la espera pasa a inscrito.
- **Límites anti-abuso:** el correo se valida y los campos tienen largo máximo; una persona
  (por correo o cuenta) solo puede tener una inscripción vigente y hasta 3 intentos por taller;
  cada taller admite como máximo 1000 inscripciones.
- El enlace público responde 404 si la inscripción está cerrada, si se cambió el enlace o si el
  módulo Portafolio está apagado.

## Gestión (nivel Gestionar en Portafolio)

Abrir y cerrar la inscripción, cupo, sesiones (una fecha o nombre por línea), asistencia mínima
para certificar (por defecto 75 %), cambiar el enlace público, lista de inscritos con asistencia
por sesión, cancelar inscripciones y exportar **CSV** (las celdas que empiezan con `=`, `+`, `-`
o `@` se neutralizan).

## Certificados

Quien asistió al menos al porcentaje mínimo de las sesiones puede recibir su certificado: la
persona desde «Mi certificado» o quien gestiona desde «Certificar». Se abre una página
imprimible (Imprimir, Guardar como PDF: no se usa ninguna librería de PDF) con nombre, taller,
horas, fechas, nombre del laboratorio, institución y logo (los de Ajustes, tabla `branding`) y
un **código de verificación** `XXXX-XXXX-XXXX-XXXX`. La página pública `/certificado/<código>`
confirma que es auténtico; no muestra el correo.

## Endpoints

| Ruta | Quién |
|---|---|
| `GET/PUT /api/portfolio/{id}/workshop` | Ver (la lista solo a Gestionar) / Gestionar |
| `POST/DELETE /api/portfolio/{id}/workshop/enroll` | Usar |
| `POST /api/portfolio/{id}/workshop/my-certificate` | Usar |
| `GET /api/portfolio/{id}/workshop/export.csv` | Gestionar |
| `PUT .../registrations/{rid}/attendance`, `POST .../cancel`, `POST .../certificate` | Gestionar |
| `GET/POST /api/portfolio/taller/{token}` | público |
| `GET /api/portfolio/certificado/{código}` | público, solo lectura |

Tablas: `workshop_settings` y `workshop_registrations` (migración «idea 40» al final de `MIGRATIONS`). Al borrar la ficha se
borran sus inscripciones. Todo lo que gestiona (apertura y cierre, asistencia, cancelaciones,
certificados, exportación) queda en la auditoría como «Portafolio».
