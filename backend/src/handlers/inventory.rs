use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use rusqlite::params;
use serde::Deserialize;

use crate::models::inventory::*;
use crate::state::AppState;

// ── Categories ──

pub async fn list_categories(State(state): State<AppState>) -> Json<Vec<InventoryCategory>> {
    let categories = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare("SELECT id, name, icon, description FROM inventory_categories ORDER BY name")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(InventoryCategory {
                id: row.get(0)?,
                name: row.get(1)?,
                icon: row.get(2)?,
                description: row.get(3)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await.unwrap_or_default();

    Json(categories)
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
    let cat_clone = cat.clone();
    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO inventory_categories (id, name, icon, description) VALUES (?1, ?2, ?3, ?4)",
            params![cat_clone.id, cat_clone.name, cat_clone.icon, cat_clone.description],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await?;
    Ok(Json(cat))
}

pub async fn update_category(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateCategoryReq>,
) -> Result<Json<InventoryCategory>, (StatusCode, String)> {
    let name = req.name.trim().to_string();
    let icon = req.icon;
    let description = req.description;
    let id_clone = id.clone();
    let cat = InventoryCategory {
        id: id.clone(),
        name: name.clone(),
        icon: icon.clone(),
        description: description.clone(),
    };
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute(
            "UPDATE inventory_categories SET name = ?1, icon = ?2, description = ?3 WHERE id = ?4",
            params![name, icon, description, id_clone],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Categoria no encontrada".to_string()));
        }
        Ok(())
    }).await?;
    Ok(Json(cat))
}

pub async fn delete_category(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute(
            "DELETE FROM inventory_categories WHERE id = ?1",
            params![id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Categoria no encontrada".to_string()));
        }
        Ok(())
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Items ──

fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<InventoryItem> {
    let status_str: String = row.get(12)?;
    let status: ItemStatus = serde_json::from_str(&format!("\"{}\"", status_str))
        .unwrap_or_default();
    let custom_fields_str: String = row.get(22)?;
    let custom_fields: std::collections::HashMap<String, String> =
        serde_json::from_str(&custom_fields_str).unwrap_or_default();
    let created_at_str: String = row.get(23)?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(InventoryItem {
        id: row.get(0)?,
        category_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        quantity: row.get(4)?,
        unit: row.get(5)?,
        cost: row.get(6)?,
        supplier: row.get(7)?,
        supplier_url: row.get(8)?,
        location: row.get(9)?,
        serial_number: row.get(10)?,
        purchase_date: row.get(11)?,
        status,
        notes: row.get(13)?,
        brand: row.get(14)?,
        model_name: row.get(15)?,
        material_type: row.get(16)?,
        filament_diameter: row.get(17)?,
        filament_weight: row.get(18)?,
        filament_remaining: row.get(19)?,
        filament_color: row.get(20)?,
        filament_density: row.get(21)?,
        custom_fields,
        created_at,
    })
}

const ITEM_SELECT: &str = "SELECT id, category_id, name, description, quantity, unit, cost, supplier, supplier_url, location, serial_number, purchase_date, status, notes, brand, model_name, material_type, filament_diameter, filament_weight, filament_remaining, filament_color, filament_density, custom_fields, created_at FROM inventory_items";

pub async fn list_items(State(state): State<AppState>) -> Json<Vec<InventoryItem>> {
    let items = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(&format!("{} ORDER BY name", ITEM_SELECT))
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row_to_item(row))
            .map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await.unwrap_or_default();

    Json(items)
}

pub async fn create_item(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<InventoryItem>, (StatusCode, String)> {
    let mut item: InventoryItem = serde_json::from_value(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Datos invalidos: {}", e)))?;
    item.id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    item.created_at = Utc::now();

    let item_clone = item.clone();
    crate::db::db_op(&state.db, move |conn| {
        insert_item(conn, &item_clone)
    }).await?;
    Ok(Json(item))
}

fn insert_item(conn: &rusqlite::Connection, item: &InventoryItem) -> Result<(), String> {
    let status_str = serde_json::to_value(&item.status)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "activo".to_string());
    let custom_fields_str = serde_json::to_string(&item.custom_fields).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT INTO inventory_items (id, category_id, name, description, quantity, unit, cost, supplier, supplier_url, location, serial_number, purchase_date, status, notes, brand, model_name, material_type, filament_diameter, filament_weight, filament_remaining, filament_color, filament_density, custom_fields, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        params![
            item.id, item.category_id, item.name, item.description,
            item.quantity, item.unit, item.cost, item.supplier, item.supplier_url,
            item.location, item.serial_number, item.purchase_date,
            status_str, item.notes, item.brand, item.model_name,
            item.material_type, item.filament_diameter, item.filament_weight,
            item.filament_remaining, item.filament_color, item.filament_density,
            custom_fields_str,
            item.created_at.to_rfc3339(),
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<InventoryItem>, (StatusCode, String)> {
    let mut item: InventoryItem = serde_json::from_value(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Datos invalidos: {}", e)))?;

    let id_clone = id.clone();
    // Retrieve original id and created_at
    let (saved_id, saved_at) = crate::db::db_op_status(&state.db, move |conn| {
        let result: Result<(String, String), _> = conn.query_row(
            "SELECT id, created_at FROM inventory_items WHERE id = ?1",
            params![id_clone],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok(v) => Ok(v),
            Err(_) => Err((StatusCode::NOT_FOUND, "Item no encontrado".to_string())),
        }
    }).await?;

    let created_at = chrono::DateTime::parse_from_rfc3339(&saved_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    item.id = saved_id;
    item.created_at = created_at;

    let item_clone = item.clone();
    let id_for_delete = id.clone();
    crate::db::db_op(&state.db, move |conn| {
        // Delete and re-insert to handle all fields cleanly
        conn.execute("DELETE FROM inventory_items WHERE id = ?1", params![id_for_delete])
            .map_err(|e| e.to_string())?;
        insert_item(conn, &item_clone)
    }).await?;

    Ok(Json(item))
}

pub async fn delete_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute(
            "DELETE FROM inventory_items WHERE id = ?1",
            params![id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Item no encontrado".to_string()));
        }
        Ok(())
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Print History ──

fn row_to_print_history(row: &rusqlite::Row) -> rusqlite::Result<PrintHistoryEntry> {
    let created_at_str: String = row.get(11)?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(PrintHistoryEntry {
        id: row.get(0)?,
        printer_id: row.get(1)?,
        file_name: row.get(2)?,
        filament_id: row.get(3)?,
        weight_grams: row.get(4)?,
        print_time_minutes: row.get(5)?,
        material_cost: row.get(6)?,
        electricity_cost: row.get(7)?,
        total_cost: row.get(8)?,
        success: row.get(9)?,
        notes: row.get(10)?,
        created_at,
    })
}

pub async fn list_print_history(State(state): State<AppState>) -> Json<Vec<PrintHistoryEntry>> {
    let history = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, printer_id, file_name, filament_id, weight_grams, print_time_minutes, material_cost, electricity_cost, total_cost, success, notes, created_at FROM print_history ORDER BY created_at DESC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row_to_print_history(row))
            .map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await.unwrap_or_default();

    Json(history)
}

pub async fn add_print_history(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<PrintHistoryEntry>, (StatusCode, String)> {
    let mut entry: PrintHistoryEntry = serde_json::from_value(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Datos invalidos: {}", e)))?;
    entry.id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    entry.created_at = Utc::now();

    let entry_clone = entry.clone();
    crate::db::db_op(&state.db, move |conn| {
        // Descontar filamento del inventario si se especifico
        if let Some(ref fil_id) = entry_clone.filament_id {
            let current_remaining: Option<f64> = conn.query_row(
                "SELECT filament_remaining FROM inventory_items WHERE id = ?1",
                params![fil_id],
                |row| row.get(0),
            ).ok().flatten();

            if let Some(remaining) = current_remaining {
                let new_remaining = (remaining - entry_clone.weight_grams).max(0.0);
                conn.execute(
                    "UPDATE inventory_items SET filament_remaining = ?1 WHERE id = ?2",
                    params![new_remaining, fil_id],
                ).map_err(|e| e.to_string())?;
            }
        }

        conn.execute(
            "INSERT INTO print_history (id, printer_id, file_name, filament_id, weight_grams, print_time_minutes, material_cost, electricity_cost, total_cost, success, notes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                entry_clone.id, entry_clone.printer_id, entry_clone.file_name, entry_clone.filament_id,
                entry_clone.weight_grams, entry_clone.print_time_minutes, entry_clone.material_cost,
                entry_clone.electricity_cost, entry_clone.total_cost, entry_clone.success, entry_clone.notes,
                entry_clone.created_at.to_rfc3339(),
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await?;
    Ok(Json(entry))
}

pub async fn delete_print_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute(
            "DELETE FROM print_history WHERE id = ?1",
            params![id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Entrada no encontrada".to_string()));
        }
        Ok(())
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}
