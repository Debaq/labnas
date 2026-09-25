//! Cola compartida de impresion 3D: cualquiera pide, operadores/admins gestionan.

use super::*;
use axum::Extension;
use rusqlite::OptionalExtension;

use crate::events::{Audience, Level};
use crate::models::notifications::UserRole;
use crate::state::SessionInfo;

const STATUSES: &[&str] = &["pendiente", "imprimiendo", "terminado", "cancelado"];
/// Terminados/cancelados que se siguen mostrando
const HISTORY_LIMIT: u32 = 30;

#[derive(Debug, Clone, serde::Serialize)]
pub struct QueueItem {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub printer_id: Option<String>,
    pub requested_by: String,
    pub notes: String,
    pub grams: Option<f64>,
    pub seconds: Option<i64>,
    pub status: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct QueueRequest {
    pub title: String,
    #[serde(default)]
    pub file_name: String,
    pub printer_id: Option<String>,
    #[serde(default)]
    pub notes: String,
    pub grams: Option<f64>,
    pub seconds: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct QueueUpdate {
    pub title: Option<String>,
    pub file_name: Option<String>,
    /// Ausente: no cambia; null: quita la impresora asignada
    #[serde(default, deserialize_with = "present")]
    pub printer_id: Option<Option<String>>,
    pub notes: Option<String>,
    pub grams: Option<f64>,
    pub seconds: Option<i64>,
    pub status: Option<String>,
}

/// Distingue "campo ausente" (None) de "null" (Some(None))
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    use serde::Deserialize;
    Option::<String>::deserialize(d).map(Some)
}

#[derive(Debug, serde::Deserialize)]
pub struct QueueReorder {
    pub ids: Vec<String>,
}

const SELECT: &str = "SELECT id, title, file_name, printer_id, requested_by, notes, grams, seconds, status, position, \
     created_at, updated_at FROM print_queue";

fn row_to_item(r: &rusqlite::Row) -> rusqlite::Result<QueueItem> {
    Ok(QueueItem {
        id: r.get(0)?,
        title: r.get(1)?,
        file_name: r.get(2)?,
        printer_id: r.get(3)?,
        requested_by: r.get(4)?,
        notes: r.get(5)?,
        grams: r.get(6)?,
        seconds: r.get(7)?,
        status: r.get(8)?,
        position: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

fn is_manager(s: &SessionInfo) -> bool {
    matches!(s.role, UserRole::Admin | UserRole::Operador)
}

fn publish(state: &AppState) {
    state.events.publish("printers3d.queue", (), Audience::All, Some("printers3d"));
}

fn load_item(conn: &rusqlite::Connection, id: &str) -> Result<Option<QueueItem>, String> {
    conn.query_row(&format!("{} WHERE id = ?1", SELECT), [id], row_to_item)
        .optional()
        .map_err(|e| e.to_string())
}

/// GET /api/printers3d/queue — activos por posicion, luego los ultimos terminados
pub async fn list_queue(State(state): State<AppState>) -> Result<Json<Vec<QueueItem>>, (StatusCode, String)> {
    let items = db_op(&state.db, |conn| {
        let mut out = Vec::new();
        let mut stmt = conn
            .prepare(&format!(
                "{} WHERE status IN ('pendiente','imprimiendo') ORDER BY status = 'imprimiendo' DESC, position",
                SELECT
            ))
            .map_err(|e| e.to_string())?;
        out.extend(stmt.query_map([], row_to_item).map_err(|e| e.to_string())?.filter_map(|r| r.ok()));
        let mut stmt = conn
            .prepare(&format!(
                "{} WHERE status IN ('terminado','cancelado') ORDER BY updated_at DESC LIMIT {}",
                SELECT, HISTORY_LIMIT
            ))
            .map_err(|e| e.to_string())?;
        out.extend(stmt.query_map([], row_to_item).map_err(|e| e.to_string())?.filter_map(|r| r.ok()));
        Ok(out)
    })
    .await?;
    Ok(Json(items))
}

fn validate(title: &str, notes: &str) -> Result<(), (StatusCode, String)> {
    if title.trim().is_empty() || title.len() > 120 {
        return Err((StatusCode::BAD_REQUEST, "El titulo debe tener entre 1 y 120 caracteres".to_string()));
    }
    if notes.len() > 2000 {
        return Err((StatusCode::BAD_REQUEST, "Notas demasiado largas".to_string()));
    }
    Ok(())
}

/// POST /api/printers3d/queue — cualquier usuario pide una impresion
pub async fn add_to_queue(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<QueueRequest>,
) -> Result<(StatusCode, Json<QueueItem>), (StatusCode, String)> {
    validate(&req.title, &req.notes)?;
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let (id2, user) = (id.clone(), session.username.clone());
    let item = db_op(&state.db, move |conn| {
        let pos: i64 = conn
            .query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM print_queue", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO print_queue (id, title, file_name, printer_id, requested_by, notes, grams, seconds, status, position, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pendiente', ?9, ?10, ?10)",
            params![id2, req.title.trim(), req.file_name.trim(), req.printer_id, user, req.notes.trim(), req.grams, req.seconds, pos, now],
        )
        .map_err(|e| e.to_string())?;
        load_item(conn, &id2)?.ok_or_else(|| "No se pudo crear".to_string())
    })
    .await?;

    state.log_activity("Cola 3D", &format!("Nuevo pedido: {}", item.title), &session.username).await;
    crate::events::notify(
        &state,
        Audience::Admins,
        Some("printers3d"),
        Level::Info,
        "Nuevo pedido de impresion 3D",
        &format!("{}: {}", session.username, item.title),
    );
    publish(&state);
    Ok((StatusCode::CREATED, Json(item)))
}

/// PUT /api/printers3d/queue/{id}
/// El solicitante edita o cancela su pedido pendiente; operadores/admins todo.
pub async fn update_queue_item(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<QueueUpdate>,
) -> Result<Json<QueueItem>, (StatusCode, String)> {
    let id2 = id.clone();
    let current = db_op(&state.db, move |conn| load_item(conn, &id2))
        .await?
        .ok_or((StatusCode::NOT_FOUND, "Pedido no encontrado".to_string()))?;

    let manager = is_manager(&session);
    let owner = current.requested_by == session.username;
    if !manager {
        let only_cancel = req.status.as_deref().is_none_or(|s| s == "cancelado");
        if !owner || current.status != "pendiente" || !only_cancel || req.printer_id.is_some() {
            return Err((StatusCode::FORBIDDEN, "Solo puedes editar o cancelar tus pedidos pendientes".to_string()));
        }
    }
    if let Some(s) = &req.status {
        if !STATUSES.contains(&s.as_str()) {
            return Err((StatusCode::BAD_REQUEST, "Estado invalido".to_string()));
        }
    }

    let mut next = current.clone();
    if let Some(t) = req.title { next.title = t.trim().to_string(); }
    if let Some(f) = req.file_name { next.file_name = f.trim().to_string(); }
    if let Some(p) = req.printer_id { next.printer_id = p.filter(|p| !p.is_empty()); }
    if let Some(n) = req.notes { next.notes = n.trim().to_string(); }
    if req.grams.is_some() { next.grams = req.grams; }
    if req.seconds.is_some() { next.seconds = req.seconds; }
    if let Some(s) = req.status { next.status = s; }
    validate(&next.title, &next.notes)?;
    next.updated_at = chrono::Utc::now().to_rfc3339();

    let n = next.clone();
    let item = db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE print_queue SET title=?1, file_name=?2, printer_id=?3, notes=?4, grams=?5, seconds=?6, status=?7, updated_at=?8 WHERE id=?9",
            params![n.title, n.file_name, n.printer_id, n.notes, n.grams, n.seconds, n.status, n.updated_at, n.id],
        )
        .map_err(|e| e.to_string())?;
        load_item(conn, &n.id)?.ok_or_else(|| "Pedido no encontrado".to_string())
    })
    .await?;

    if item.status != current.status {
        state
            .log_activity("Cola 3D", &format!("{}: {} -> {}", item.title, current.status, item.status), &session.username)
            .await;
        // avisar al solicitante cuando otro cambia el estado de su pedido
        if item.requested_by != session.username {
            let (level, text) = match item.status.as_str() {
                "imprimiendo" => (Level::Info, "Tu pedido se esta imprimiendo"),
                "terminado" => (Level::Success, "Tu pedido esta listo"),
                "cancelado" => (Level::Warning, "Tu pedido fue cancelado"),
                _ => (Level::Info, "Tu pedido volvio a la cola"),
            };
            crate::events::notify(&state, Audience::User(item.requested_by.clone()), Some("printers3d"), level, text, &item.title);
        }
    }
    publish(&state);
    Ok(Json(item))
}

/// DELETE /api/printers3d/queue/{id} — el solicitante (si esta pendiente) o un operador
pub async fn delete_queue_item(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let id2 = id.clone();
    let current = db_op(&state.db, move |conn| load_item(conn, &id2))
        .await?
        .ok_or((StatusCode::NOT_FOUND, "Pedido no encontrado".to_string()))?;
    let owner_pending = current.requested_by == session.username && current.status == "pendiente";
    if !is_manager(&session) && !owner_pending {
        return Err((StatusCode::FORBIDDEN, "Sin permiso para borrar este pedido".to_string()));
    }
    db_op(&state.db, move |conn| {
        conn.execute("DELETE FROM print_queue WHERE id = ?1", [&id]).map_err(|e| e.to_string())
    })
    .await?;
    state.log_activity("Cola 3D", &format!("Pedido eliminado: {}", current.title), &session.username).await;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/printers3d/queue/reorder — operadores/admins (orden de los pendientes)
pub async fn reorder_queue(
    State(state): State<AppState>,
    Json(req): Json<QueueReorder>,
) -> Result<StatusCode, (StatusCode, String)> {
    db_op(&state.db, move |conn| {
        for (i, id) in req.ids.iter().enumerate() {
            conn.execute("UPDATE print_queue SET position = ?1 WHERE id = ?2", params![i as i64 + 1, id])
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await?;
    publish(&state);
    Ok(StatusCode::OK)
}
