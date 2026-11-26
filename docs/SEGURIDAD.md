# Seguridad laboral

Módulo **Seguridad** (`/safety`, encendido por defecto). Tiene tres pestañas. Las inducciones
**por máquina** siguen en Máquinas (ver MAQUINAS.md); aquí van las capacitaciones **generales**
del laboratorio (primeros auxilios, extintores, manejo de reactivos...).

## Niveles

| Nivel | Qué puede |
|---|---|
| Ver | Ver las fichas, el catálogo y sus propias capacitaciones; ver sus incidentes |
| Usar | Lo anterior y **reportar** incidentes y casi accidentes |
| Gestionar | Revisar y cerrar incidentes (ve todos), administrar el catálogo y los registros, editar fichas |

Plantillas de rol: Operador, Encargado técnico y Docente *Gestionar*; Auditor *Ver*; el resto
*Usar* (ver ROLES.md).

## Incidentes y casi accidentes

Quien reporta indica gravedad (`casi_accidente`, `leve`, `grave`), descripción, cuándo y dónde
ocurrió, máquina opcional, acciones tomadas en el momento y una foto opcional (JPEG o PNG,
hasta 2 MB, guardada en la base). Cada persona ve solo lo que reportó; quien gestiona ve todo.

- **Grave:** aviso inmediato (Telegram y notificación) a quienes gestionan Seguridad.
- **Estados:** `abierto` → `en_revision` → `cerrado`. Cerrar exige una nota; queda quién y cuándo.
- **Trazabilidad:** un incidente nunca se borra (no hay `DELETE`); todo cambio queda en la bitácora
  de actividad bajo «Seguridad». Quien reportó recibe un aviso cuando cambia el estado.

## Capacitaciones

Catálogo con nombre, descripción, vigencia en meses (opcional) y si es obligatoria. Quien
gestiona registra quién la tiene: si la capacitación tiene vigencia y no se indica fecha, el
vencimiento se calcula desde la fecha en que se hizo. «Mis capacitaciones» muestra el estado
propio: vigente, por vencer (30 días), vencida o pendiente. La insignia de la pestaña cuenta
las obligatorias que faltan.

Los vencimientos aparecen en **Plazos** (fuente `capacitacion`, hasta 30 días después de vencer)
y avisan a la persona y a quienes gestionan Seguridad (30, 7 y 1 día antes, y el mismo día).

## Fichas de seguridad

Hojas de datos de seguridad de materiales, procedimientos y fichas de equipos: título, tipo,
texto, enlace (`https://` o ruta del NAS `/...`) y archivo adjunto opcional (PDF, JPEG o PNG, hasta
5 MB, guardado en la base y validado por su contenido). Se pueden vincular a una máquina y/o a
un ítem del Inventario. Todos con acceso al módulo las ven; solo *Gestionar* las edita.

## API (`/api/safety`)

| Método y ruta | Nivel |
|---|---|
| `GET /summary` | Ver |
| `GET /incidents`, `GET /incidents/{id}/photo` | Ver (solo lo propio si no gestiona) |
| `POST /incidents` | Usar |
| `PUT /incidents/{id}` | Gestionar |
| `GET /trainings`, `GET /trainings/mine`, `GET /records` | Ver (registros: solo los propios si no gestiona) |
| `POST/PUT/DELETE /trainings[/{id}]`, `POST /records`, `DELETE /records/{training_id}/{username}` | Gestionar |
| `GET /sheets`, `GET /sheets/{id}/file` | Ver |
| `POST/PUT/DELETE /sheets[/{id}]` | Gestionar |

Código: `backend/src/handlers/safety.rs`, tablas `safety_*` (migración al final de `db.rs`),
página `frontend/src/pages/SafetyPage.tsx` y `components/safety/`. Tests:
`backend/src/integration_tests/safety.rs`.
