use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use rusqlite::params;

use crate::models::portfolio::*;
use crate::state::AppState;

// ── Helpers ──

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<PortfolioEntry> {
    let entry_type_str: String = row.get(1)?;
    let entry_type: PortfolioType = serde_json::from_str(&format!("\"{}\"", entry_type_str))
        .unwrap_or_default();
    let scope_str: String = row.get(2)?;
    let scope: PortfolioScope = serde_json::from_str(&format!("\"{}\"", scope_str))
        .unwrap_or_default();
    let status_str: String = row.get(17)?;
    let status: PortfolioStatus = serde_json::from_str(&format!("\"{}\"", status_str))
        .unwrap_or_default();

    let collaborators_str: String = row.get(11)?;
    let collaborators: Vec<String> = serde_json::from_str(&collaborators_str).unwrap_or_default();
    let participants_str: String = row.get(12)?;
    let participants: Vec<String> = serde_json::from_str(&participants_str).unwrap_or_default();
    let requirements_str: String = row.get(18)?;
    let requirements: Vec<Requirement> = serde_json::from_str(&requirements_str).unwrap_or_default();
    let milestones_str: String = row.get(19)?;
    let milestones: Vec<Milestone> = serde_json::from_str(&milestones_str).unwrap_or_default();
    let tags_str: String = row.get(20)?;
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    let related_files_str: String = row.get(22)?;
    let related_files: Vec<String> = serde_json::from_str(&related_files_str).unwrap_or_default();
    let related_inventory_str: String = row.get(23)?;
    let related_inventory: Vec<String> = serde_json::from_str(&related_inventory_str).unwrap_or_default();

    let created_at_str: String = row.get(24)?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(PortfolioEntry {
        id: row.get(0)?,
        entry_type,
        scope,
        title: row.get(3)?,
        description: row.get(4)?,
        institution: row.get(5)?,
        url: row.get(6)?,
        contact: row.get(7)?,
        funding_source: row.get(8)?,
        budget: row.get(9)?,
        principal_investigator: row.get(10)?,
        collaborators,
        participants,
        start_date: row.get(13)?,
        end_date: row.get(14)?,
        hours: row.get(15)?,
        modality: row.get(16)?,
        status,
        requirements,
        milestones,
        tags,
        notes: row.get(21)?,
        related_files,
        related_inventory,
        created_at,
    })
}

const ENTRY_SELECT: &str = "SELECT id, entry_type, scope, title, description, institution, url, contact, funding_source, budget, principal_investigator, collaborators, participants, start_date, end_date, hours, modality, status, requirements, milestones, tags, notes, related_files, related_inventory, created_at FROM portfolio_entries";

fn insert_entry(conn: &rusqlite::Connection, entry: &PortfolioEntry) -> Result<(), String> {
    let type_str = serde_json::to_value(&entry.entry_type)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "project".to_string());
    let scope_str = serde_json::to_value(&entry.scope)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "own".to_string());
    let status_str = serde_json::to_value(&entry.status)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "planned".to_string());

    conn.execute(
        "INSERT INTO portfolio_entries (id, entry_type, scope, title, description, institution, url, contact, funding_source, budget, principal_investigator, collaborators, participants, start_date, end_date, hours, modality, status, requirements, milestones, tags, notes, related_files, related_inventory, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
        params![
            entry.id, type_str, scope_str, entry.title, entry.description,
            entry.institution, entry.url, entry.contact, entry.funding_source, entry.budget,
            entry.principal_investigator,
            serde_json::to_string(&entry.collaborators).unwrap_or_default(),
            serde_json::to_string(&entry.participants).unwrap_or_default(),
            entry.start_date, entry.end_date, entry.hours, entry.modality, status_str,
            serde_json::to_string(&entry.requirements).unwrap_or_default(),
            serde_json::to_string(&entry.milestones).unwrap_or_default(),
            serde_json::to_string(&entry.tags).unwrap_or_default(),
            entry.notes,
            serde_json::to_string(&entry.related_files).unwrap_or_default(),
            serde_json::to_string(&entry.related_inventory).unwrap_or_default(),
            entry.created_at.to_rfc3339(),
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ── List all entries ──

pub async fn list_entries(State(state): State<AppState>) -> Json<Vec<PortfolioEntry>> {
    let entries = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(&format!("{} ORDER BY created_at DESC", ENTRY_SELECT))
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row_to_entry(row))
            .map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await.unwrap_or_default();

    Json(entries)
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

    let entry_clone = entry.clone();
    crate::db::db_op(&state.db, move |conn| {
        insert_entry(conn, &entry_clone)
    }).await?;
    Ok(Json(entry))
}

// ── Update entry ──

pub async fn update_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<PortfolioEntry>, (StatusCode, String)> {
    let mut entry: PortfolioEntry = serde_json::from_value(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Datos invalidos: {}", e)))?;

    let id_clone = id.clone();
    // Retrieve original id and created_at
    let (saved_id, saved_at) = crate::db::db_op_status(&state.db, move |conn| {
        let result: Result<(String, String), _> = conn.query_row(
            "SELECT id, created_at FROM portfolio_entries WHERE id = ?1",
            params![id_clone],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok(v) => Ok(v),
            Err(_) => Err((StatusCode::NOT_FOUND, "Entrada no encontrada".to_string())),
        }
    }).await?;

    let created_at = chrono::DateTime::parse_from_rfc3339(&saved_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    entry.id = saved_id;
    entry.created_at = created_at;

    let entry_clone = entry.clone();
    let id_for_delete = id.clone();
    crate::db::db_op(&state.db, move |conn| {
        conn.execute("DELETE FROM portfolio_entries WHERE id = ?1", params![id_for_delete])
            .map_err(|e| e.to_string())?;
        insert_entry(conn, &entry_clone)
    }).await?;

    Ok(Json(entry))
}

// ── Delete entry ──

pub async fn delete_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute(
            "DELETE FROM portfolio_entries WHERE id = ?1",
            params![id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Entrada no encontrada".to_string()));
        }
        Ok(())
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Toggle requirement ──

pub async fn toggle_requirement(
    State(state): State<AppState>,
    Path((id, req_id)): Path<(String, String)>,
) -> Result<Json<PortfolioEntry>, (StatusCode, String)> {
    let entry = crate::db::db_op_status(&state.db, move |conn| {
        // Read current requirements JSON
        let requirements_str: String = conn.query_row(
            "SELECT requirements FROM portfolio_entries WHERE id = ?1",
            params![id],
            |row| row.get(0),
        ).map_err(|_| (StatusCode::NOT_FOUND, "Entrada no encontrada".to_string()))?;

        let mut requirements: Vec<Requirement> = serde_json::from_str(&requirements_str)
            .unwrap_or_default();

        let req = requirements.iter_mut().find(|r| r.id == req_id)
            .ok_or((StatusCode::NOT_FOUND, "Requisito no encontrado".to_string()))?;
        req.completed = !req.completed;

        let new_json = serde_json::to_string(&requirements)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        conn.execute(
            "UPDATE portfolio_entries SET requirements = ?1 WHERE id = ?2",
            params![new_json, id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Return updated full entry
        let entry = conn.query_row(
            &format!("{} WHERE id = ?1", ENTRY_SELECT),
            params![id],
            |row| row_to_entry(row),
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(entry)
    }).await?;

    Ok(Json(entry))
}

// ── Toggle milestone ──

pub async fn toggle_milestone(
    State(state): State<AppState>,
    Path((id, mil_id)): Path<(String, String)>,
) -> Result<Json<PortfolioEntry>, (StatusCode, String)> {
    let entry = crate::db::db_op_status(&state.db, move |conn| {
        // Read current milestones JSON
        let milestones_str: String = conn.query_row(
            "SELECT milestones FROM portfolio_entries WHERE id = ?1",
            params![id],
            |row| row.get(0),
        ).map_err(|_| (StatusCode::NOT_FOUND, "Entrada no encontrada".to_string()))?;

        let mut milestones: Vec<Milestone> = serde_json::from_str(&milestones_str)
            .unwrap_or_default();

        let mil = milestones.iter_mut().find(|m| m.id == mil_id)
            .ok_or((StatusCode::NOT_FOUND, "Hito no encontrado".to_string()))?;
        mil.completed = !mil.completed;

        let new_json = serde_json::to_string(&milestones)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        conn.execute(
            "UPDATE portfolio_entries SET milestones = ?1 WHERE id = ?2",
            params![new_json, id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Return updated full entry
        let entry = conn.query_row(
            &format!("{} WHERE id = ?1", ENTRY_SELECT),
            params![id],
            |row| row_to_entry(row),
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(entry)
    }).await?;

    Ok(Json(entry))
}
