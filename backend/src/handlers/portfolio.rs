use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;

use crate::config::save_config;
use crate::models::portfolio::*;
use crate::state::AppState;

// ── List all entries ──

pub async fn list_entries(State(state): State<AppState>) -> Json<Vec<PortfolioEntry>> {
    let config = state.config.lock().await;
    Json(config.portfolio.entries.clone())
}

// ── Create entry ──

pub async fn create_entry(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<PortfolioEntry>, (StatusCode, String)> {
    let mut entry: PortfolioEntry = serde_json::from_value(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Datos invalidos: {}", e)))?;
    entry.id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    entry.created_at = Utc::now();

    let mut config = state.config.lock().await;
    config.portfolio.entries.push(entry.clone());
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(entry))
}

// ── Update entry ──

pub async fn update_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<PortfolioEntry>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let entry = config.portfolio.entries.iter_mut().find(|e| e.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Entrada no encontrada".to_string()))?;

    let saved_id = entry.id.clone();
    let saved_at = entry.created_at;
    *entry = serde_json::from_value(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Datos invalidos: {}", e)))?;
    entry.id = saved_id;
    entry.created_at = saved_at;

    let result = entry.clone();
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}

// ── Delete entry ──

pub async fn delete_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.portfolio.entries.len();
    config.portfolio.entries.retain(|e| e.id != id);
    if config.portfolio.entries.len() == before {
        return Err((StatusCode::NOT_FOUND, "Entrada no encontrada".to_string()));
    }
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Toggle requirement ──

pub async fn toggle_requirement(
    State(state): State<AppState>,
    Path((id, req_id)): Path<(String, String)>,
) -> Result<Json<PortfolioEntry>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let entry = config.portfolio.entries.iter_mut().find(|e| e.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Entrada no encontrada".to_string()))?;
    let req = entry.requirements.iter_mut().find(|r| r.id == req_id)
        .ok_or((StatusCode::NOT_FOUND, "Requisito no encontrado".to_string()))?;
    req.completed = !req.completed;

    let result = entry.clone();
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}

// ── Toggle milestone ──

pub async fn toggle_milestone(
    State(state): State<AppState>,
    Path((id, mil_id)): Path<(String, String)>,
) -> Result<Json<PortfolioEntry>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let entry = config.portfolio.entries.iter_mut().find(|e| e.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Entrada no encontrada".to_string()))?;
    let mil = entry.milestones.iter_mut().find(|m| m.id == mil_id)
        .ok_or((StatusCode::NOT_FOUND, "Hito no encontrado".to_string()))?;
    mil.completed = !mil.completed;

    let result = entry.clone();
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}
