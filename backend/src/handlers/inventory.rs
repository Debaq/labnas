use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::config::save_config;
use crate::models::inventory::*;
use crate::state::AppState;

// ── Categories ──

pub async fn list_categories(State(state): State<AppState>) -> Json<Vec<InventoryCategory>> {
    let config = state.config.lock().await;
    Json(config.inventory.categories.clone())
}

#[derive(Debug, Deserialize)]
pub struct CreateCategoryReq {
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub description: String,
}

pub async fn create_category(
    State(state): State<AppState>,
    Json(req): Json<CreateCategoryReq>,
) -> Result<Json<InventoryCategory>, (StatusCode, String)> {
    let cat = InventoryCategory {
        id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
        name: req.name.trim().to_string(),
        icon: req.icon,
        description: req.description,
    };
    let mut config = state.config.lock().await;
    config.inventory.categories.push(cat.clone());
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(cat))
}

pub async fn update_category(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateCategoryReq>,
) -> Result<Json<InventoryCategory>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let cat = config.inventory.categories.iter_mut().find(|c| c.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Categoria no encontrada".to_string()))?;
    cat.name = req.name.trim().to_string();
    cat.icon = req.icon;
    cat.description = req.description;
    let result = cat.clone();
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}

pub async fn delete_category(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.inventory.categories.len();
    config.inventory.categories.retain(|c| c.id != id);
    if config.inventory.categories.len() == before {
        return Err((StatusCode::NOT_FOUND, "Categoria no encontrada".to_string()));
    }
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Items ──

pub async fn list_items(State(state): State<AppState>) -> Json<Vec<InventoryItem>> {
    let config = state.config.lock().await;
    Json(config.inventory.items.clone())
}

pub async fn create_item(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<InventoryItem>, (StatusCode, String)> {
    let mut item: InventoryItem = serde_json::from_value(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Datos invalidos: {}", e)))?;
    item.id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    item.created_at = Utc::now();

    let mut config = state.config.lock().await;
    config.inventory.items.push(item.clone());
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(item))
}

pub async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<InventoryItem>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let item = config.inventory.items.iter_mut().find(|i| i.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Item no encontrado".to_string()))?;

    // Update fields from JSON, preserving id and created_at
    let saved_id = item.id.clone();
    let saved_at = item.created_at;
    *item = serde_json::from_value(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Datos invalidos: {}", e)))?;
    item.id = saved_id;
    item.created_at = saved_at;

    let result = item.clone();
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}

pub async fn delete_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.inventory.items.len();
    config.inventory.items.retain(|i| i.id != id);
    if config.inventory.items.len() == before {
        return Err((StatusCode::NOT_FOUND, "Item no encontrado".to_string()));
    }
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Print History ──

pub async fn list_print_history(State(state): State<AppState>) -> Json<Vec<PrintHistoryEntry>> {
    let config = state.config.lock().await;
    Json(config.inventory.print_history.clone())
}

pub async fn add_print_history(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<PrintHistoryEntry>, (StatusCode, String)> {
    let mut entry: PrintHistoryEntry = serde_json::from_value(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Datos invalidos: {}", e)))?;
    entry.id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    entry.created_at = Utc::now();

    // Descontar filamento del inventario si se especificó
    let mut config = state.config.lock().await;
    if let Some(ref fil_id) = entry.filament_id {
        if let Some(fil) = config.inventory.items.iter_mut().find(|i| i.id == *fil_id) {
            if let Some(ref mut remaining) = fil.filament_remaining {
                *remaining = (*remaining - entry.weight_grams).max(0.0);
            }
        }
    }

    config.inventory.print_history.push(entry.clone());
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(entry))
}

pub async fn delete_print_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.inventory.print_history.len();
    config.inventory.print_history.retain(|e| e.id != id);
    if config.inventory.print_history.len() == before {
        return Err((StatusCode::NOT_FOUND, "Entrada no encontrada".to_string()));
    }
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::NO_CONTENT)
}
