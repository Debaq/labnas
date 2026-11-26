# Roles y permisos por módulo

Configuración > Usuarios. Solo el administrador ve esta sección y la cambia.

## Niveles

Cada rol tiene un nivel en cada módulo:

| Nivel | Qué permite |
|---|---|
| **Sin acceso** | El módulo no aparece en el menú, la API responde 403 y sus avisos no le llegan |
| **Ver** | Consultar: listas, estados, historial |
| **Usar** | Lo de todos los días: imprimir, reservar, encolar una pieza, crear tareas y notas propias, poner música |
| **Gestionar** | Lo que antes era de operador: editar lo de otros, aprobar reservas, controlar impresoras 3D, despertar equipos |

El **Administrador** lo puede todo, incluida la configuración del sistema (usuarios, roles,
respaldos, actualizaciones), y su rol no se edita. Una cuenta **Pendiente** no entra a nada
hasta que alguien le asigna un rol.

Casos especiales:

- **Archivos:** con *Ver* se lee; *Usar* equivale al antiguo «permiso de escritura» (el principal
  `perm:write` de las carpetas). Los permisos por carpeta siguen mandando por encima.
  Crear enlaces públicos pide *Usar*.
- **Terminal:** *Usar* abre la terminal (y `/cmd` en Telegram).
- **Cámaras:** *Ver* mira las cámaras y descarga grabaciones, *Usar* graba y saca fotos. El aviso
  «se está grabando» lo ve todo el mundo. La detección con IA sigue limitada al admin y a las
  personas autorizadas.
- **Tareas:** *Gestionar* edita tareas, proyectos y eventos ajenos, y recibe las tareas
  asignadas a `@all`.

## Roles de partida

| Rol | Para quién |
|---|---|
| Operador | Personal del laboratorio (mismo acceso que antes) |
| Encargado técnico | Mantiene máquinas, impresoras 3D, inventario y red |
| Investigador responsable | Dirige proyectos: gestiona tareas, portafolio y plazos |
| Administrativo | Compras, inventario y plazos; no opera equipos |
| Docente | Gestiona tareas, portafolio y plazos de sus cursos |
| Ayudante | Apoya a estudiantes: gestiona la cola de impresión 3D |
| Estudiante | Reserva máquinas, imprime y lleva sus tareas |
| Observador | Usuario general (mismo acceso que antes) |
| Usuario externo | Solo reserva máquinas y encola piezas |
| Auditor | Ve todo sin modificar nada; al asignarlo se propone una fecha de término a 90 días |

Todos se editan. **Restaurar plantilla** devuelve un rol a sus niveles de partida. Un rol
nuevo se crea **partiendo de otro** (copia sus niveles) o desde cero (sin acceso a nada).
Un rol solo se borra si ninguna cuenta ni chat de Telegram lo usa.

Cuando se agrega un módulo nuevo a LabNAS, los roles de partida (y los copiados de ellos)
reciben el nivel que trae su plantilla; los roles creados desde cero quedan sin acceso
hasta que el admin decida.

## Por persona

En la ficha de cada cuenta, **Vencimiento y excepciones**:

- **Fecha de término:** después de ese día la cuenta no entra (web, WebDAV, bandeja,
  impresora de red ni Telegram) y sus sesiones abiertas se cortan. El administrador no puede
  tener fecha de término.
- **Excepciones:** subir o bajar un módulo solo para esa persona, sin cambiarle el rol.
  Los permisos sueltos de antes (terminal, impresión, escribir archivos) se migraron como
  excepciones donde no coincidían con el rol.

Los cambios valen al instante para las sesiones abiertas.

## Telegram

Un chat vinculado a una cuenta web tiene los permisos de esa cuenta (rol + excepciones). Un
chat sin cuenta usa los niveles de su rol. Los comandos de consulta (`/tareas`, `/impresoras`,
`/musica`…) piden *Ver* y el resto *Usar*; controlar impresoras 3D pide *Gestionar*.

## Para programar

- `backend/src/roles.rs`: niveles, plantillas (`template_level`) y consultas
  (`users_with_level`, `chats_with_level_sql`).
- `middleware::required_access` devuelve `Any`, `Ver`, `Usar`, `Gestionar` o `Admin`; el nivel
  se mide en el módulo de la ruta (`crate::modules::for_path`, según el campo `api` del registro). El test
  `niveles_siempre_con_modulo` revisa que toda ruta con nivel tenga módulo.
- En un handler: `session.can("machines", Level::Gestionar)`; nunca comparar nombres de rol.
- Módulo nuevo: agregarlo al registro `backend/src/modules.rs` y a cada fila de
  `template_level` (el test `plantillas_cubren_todos_los_modulos` lo recuerda).
