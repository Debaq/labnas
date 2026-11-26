# Niveles de acceso

Cualquier registro o archivo de LabNAS tiene uno de tres niveles:

| Nivel | Quién lo ve |
|---|---|
| 🔒 **Privado** (por defecto) | El equipo, según su rol, en su módulo de siempre |
| 🤝 **Con autorización** | Además, las personas autorizadas, cada una hasta una fecha, aunque su rol no tenga el módulo |
| 🌐 **Público** | Cualquiera con el enlace, sin cuenta |

Hoy aplica a: **proyectos** del Portafolio, **bienes** del Inventario, **notas**, **plazos**
sueltos y **archivos** del explorador.

## Cómo se usa

- En cada registro aparece una insignia (candado, apretón de manos o globo). Al hacer clic
  se abre la ventana **Quién puede verlo**: se elige el nivel, se autoriza a personas con
  fecha de término y un motivo (por ejemplo «rendición ANID»), y se copia el enlace público.
- Autorizar a alguien saca el registro de privado. Volver a privado deja las
  autorizaciones en suspenso y **mata el enlace público**; al publicarlo de nuevo se genera
  otro enlace.
- La persona autorizada recibe un aviso por Telegram (si lo tiene vinculado) y ve el
  registro en **Compartido conmigo** (aparece en el menú solo si tiene algo vigente).
- Para alguien de fuera (Finanzas, el financiador, un auditor) se crea antes una cuenta
  con rol **Usuario externo** o **Auditor**; su cuenta y cada autorización vencen solas.

## Quién decide el nivel

- Quien tiene nivel **Gestionar** en el módulo del registro, o el admin.
- Quien **creó** el registro (notas y plazos), si tiene nivel **Usar** en el módulo.
- Archivos: quien puede leer el archivo y tiene **Usar** en Archivos (lo mismo que pide un
  enlace de descarga).

## Qué se muestra afuera

Autorizados y público no entran al módulo: ven una **ficha de solo lectura** que arma cada
tipo de registro, sin datos internos (contacto, notas privadas del proyecto). Un archivo se
puede descargar desde su ficha.

Si el módulo del registro está apagado, la ficha y el enlace público responden «no
encontrado».

## Trazabilidad

- Cada cambio de nivel y cada autorización dada o quitada queda en la auditoría
  (acción «Nivel de acceso»).
- Cada vista y descarga por autorización o por enlace público queda en el registro de
  accesos (idea 72).
- Al borrar un registro se borran su nivel y sus autorizaciones.

## Para desarrolladores

- `backend/src/handlers/access.rs`: tipos (`KINDS`), ficha (`load_record`), reglas
  (`can_manage`) y rutas `/api/access*`, `/api/shared*` y `/api/public/{token}`.
- Tablas `record_access` (nivel y token por registro) y `record_grants` (persona y fecha de
  término), migración 25.
- Tipo nuevo: agregarlo a `KINDS`, armar su ficha en `load_record`, llamar a
  `access::forget` al borrar y poner `<AccessBadge>` en su página
  (`frontend/src/components/access`).
