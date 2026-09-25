//! Auditoria persistente: consulta (admin) y limpieza por antiguedad.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use rusqlite::params_from_iter;
use serde::Deserialize;

use crate::db::DbPool;
use crate::state::{ActivityEvent, AppState};

/// Dias que se conservan los eventos (setting `audit_retention_days`)
const DEFAULT_RETENTION_DAYS: u32 = 180;
const MAX_PAGE: u32 = 500;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    /// Usuario exacto
    pub user: Option<String>,
    /// Texto a buscar en accion o detalle
    pub q: Option<String>,
    /// Paginacion: solo eventos con id menor
    pub before_id: Option<i64>,
    pub limit: Option<u32>,
}

fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<ActivityEvent> {
    Ok(ActivityEvent {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        username: row.get(2)?,
        action: row.get(3)?,
        details: row.get(4)?,
    })
}

/// GET /api/audit — mas recientes primero
pub async fn list_audit(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<ActivityEvent>>, (StatusCode, String)> {
    let events = crate::db::db_op(&state.db, move |conn| {
        let mut sql = String::from(
            "SELECT id, timestamp, username, action, details FROM audit_log WHERE 1=1",
        );
        let mut args: Vec<String> = Vec::new();

        if let Some(user) = query.user.filter(|u| !u.trim().is_empty()) {
            sql.push_str(" AND username = ?");
            args.push(user.trim().to_string());
        }
        if let Some(q) = query.q.filter(|q| !q.trim().is_empty()) {
            sql.push_str(" AND (action LIKE ? OR details LIKE ?)");
            let pattern = format!("%{}%", q.trim());
            args.push(pattern.clone());
            args.push(pattern);
        }
        if let Some(before) = query.before_id {
            sql.push_str(" AND id < ?");
            args.push(before.to_string());
        }
        let limit = query.limit.unwrap_or(100).clamp(1, MAX_PAGE);
        sql.push_str(&format!(" ORDER BY id DESC LIMIT {}", limit));

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params_from_iter(args.iter()), row_to_event)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
    .await?;

    Ok(Json(events))
}

/// Ultimos `n` eventos (mas recientes primero). Para el bot de Telegram.
pub async fn recent(pool: &DbPool, n: u32) -> Vec<ActivityEvent> {
    crate::db::db_op(pool, move |conn| {
        let mut stmt = conn
            .prepare("SELECT id, timestamp, username, action, details FROM audit_log ORDER BY id DESC LIMIT ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([n], row_to_event).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_default()
}

/// Borra eventos mas antiguos que la retencion configurada, una vez al dia.
pub async fn audit_cleanup_loop(state: AppState) {
    loop {
        let res = crate::db::db_op(&state.db, |conn| {
            let days = crate::db::get_setting_u32(conn, "audit_retention_days", DEFAULT_RETENTION_DAYS);
            let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
            conn.execute("DELETE FROM audit_log WHERE timestamp < ?1", [cutoff])
                .map_err(|e| e.to_string())
        })
        .await;
        if let Ok(deleted) = res {
            if deleted > 0 {
                println!("[Auditoria] Limpieza: {} eventos eliminados", deleted);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
    }
}
