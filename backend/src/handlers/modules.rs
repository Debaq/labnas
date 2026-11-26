use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::db;
use crate::state::{AppState, SessionInfo};

pub async fn list_modules(
    State(state): State<AppState>,
) -> Result<Json<Vec<db::ModuleInfo>>, (StatusCode, String)> {
    db::db_op(&state.db, |conn| Ok(db::get_modules(conn))).await.map(Json)
}

#[derive(Deserialize)]
pub struct ToggleBody {
    pub enabled: bool,
    /// Apagar aunque otro modulo encendido lo use
    #[serde(default)]
    pub force: bool,
}

pub async fn toggle_module(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(body): Json<ToggleBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if id == "dashboard" {
        return Err((StatusCode::BAD_REQUEST, "No se puede desactivar el dashboard".into()));
    }

    if !body.enabled && !body.force {
        let on = state.enabled_modules.lock().await.clone();
        if let Some(why) = crate::modules::disable_warning(&id, &on) {
            return Err((StatusCode::CONFLICT, why));
        }
    }

    let id_clone = id.clone();
    let enabled = body.enabled;
    db::db_op(&state.db, move |conn| {
        db::set_module_enabled(conn, &id_clone, enabled)
    }).await?;

    // Update in-memory cache
    {
        let mut cache = state.enabled_modules.lock().await;
        if enabled {
            cache.insert(id.clone());
        } else {
            cache.remove(&id);
        }
    }

    // Tareas de fondo del modulo, sin reiniciar
    if enabled {
        crate::module_tasks::start(&state, &id);
    } else {
        crate::module_tasks::stop(&state, &id).await;
    }

    // Publicar o dejar de publicar datos hacia afuera queda en la bitacora inalterable
    if id == "transparencia" {
        crate::finance::ledger(&state, &session.username, "transparencia", "modulo", serde_json::json!({ "activado": enabled })).await?;
        state.log_activity("Transparencia", if enabled { "Modulo activado" } else { "Modulo desactivado" }, &session.username).await;
    }

    // Apagar la terminal corta tambien las sesiones abiertas desde Telegram
    if id == "terminal" && !enabled {
        let sessions: Vec<_> = state.tg_terminals.lock().await.drain().map(|(_, s)| s).collect();
        for mut s in sessions {
            let _ = s.child.kill().await;
        }
    }

    // Al encender: lo que usa y esta apagado (la interfaz lo menciona)
    let missing: Vec<&str> = if enabled {
        let on = state.enabled_modules.lock().await;
        crate::modules::uses(&id).into_iter().filter(|m| !on.contains(*m)).collect()
    } else {
        Vec::new()
    };

    Ok(Json(serde_json::json!({ "ok": true, "id": id, "enabled": enabled, "missing": missing })))
}

#[derive(Deserialize)]
pub struct ReorderItem {
    pub id: String,
    pub order: i32,
}

pub async fn reorder_modules(
    State(state): State<AppState>,
    Json(items): Json<Vec<ReorderItem>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pairs: Vec<(String, i32)> = items.into_iter().map(|i| (i.id, i.order)).collect();
    db::db_op(&state.db, move |conn| {
        db::set_module_order(conn, &pairs)
    }).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
