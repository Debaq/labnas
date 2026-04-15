use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::db;
use crate::state::AppState;

pub async fn list_modules(
    State(state): State<AppState>,
) -> Result<Json<Vec<db::ModuleInfo>>, (StatusCode, String)> {
    db::db_op(&state.db, |conn| Ok(db::get_modules(conn))).await.map(Json)
}

#[derive(Deserialize)]
pub struct ToggleBody {
    pub enabled: bool,
}

pub async fn toggle_module(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ToggleBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if id == "dashboard" {
        return Err((StatusCode::BAD_REQUEST, "No se puede desactivar el dashboard".into()));
    }

    let id_clone = id.clone();
    let enabled = body.enabled;
    db::db_op(&state.db, move |conn| {
        db::set_module_enabled(conn, &id_clone, enabled)
    }).await?;

    // Update in-memory cache
    let mut cache = state.enabled_modules.lock().await;
    if enabled {
        cache.insert(id.clone());
    } else {
        cache.remove(&id);
    }

    Ok(Json(serde_json::json!({ "ok": true, "id": id, "enabled": enabled })))
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
