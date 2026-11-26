//! Reciclaje (idea 36), en Inventario: kilos de plastico triturado, filamento recuperado y
//! desecho por material, para metricas de sostenibilidad. Se anota cada movimiento y se ve el
//! total por mes y por material. Ver docs/INVENTARIO.md.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::{internal, ApiError};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

pub const KINDS: &[&str] = &["triturado", "recuperado", "desecho"];

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub id: String,
    /// `YYYY-MM-DD`
    pub date: String,
    pub material: String,
    /// triturado | recuperado (filamento hecho de vuelta) | desecho (no reciclable)
    pub kind: String,
    pub kg: f64,
    pub notes: String,
    pub created_by: String,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub entries: Vec<Entry>,
    /// (mes `YYYY-MM`, tipo, kg) de los ultimos 12 meses
    pub by_month: Vec<(String, String, f64)>,
    /// (material, tipo, kg) de los ultimos 12 meses
    pub by_material: Vec<(String, String, f64)>,
    /// Totales de los ultimos 12 meses
    pub total_triturado: f64,
    pub total_recuperado: f64,
    pub total_desecho: f64,
}

/// GET /api/inventory/recycling — ultimos 200 movimientos y metricas de 12 meses
pub async fn summary(State(state): State<AppState>) -> Result<Json<Summary>, ApiError> {
    db_op(&state.db, |conn| {
        let since = (chrono::Local::now().date_naive() - chrono::Duration::days(365)).format("%Y-%m-%d").to_string();
        let mut stmt = conn
            .prepare("SELECT id, date, material, kind, kg, notes, created_by FROM recycling_log ORDER BY date DESC, created_at DESC LIMIT 200")
            .map_err(|e| e.to_string())?;
        let entries: Vec<Entry> = stmt
            .query_map([], |r| {
                Ok(Entry { id: r.get(0)?, date: r.get(1)?, material: r.get(2)?, kind: r.get(3)?, kg: r.get(4)?, notes: r.get(5)?, created_by: r.get(6)? })
            })
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<_>>()
            .map_err(|e| e.to_string())?;
        let grouped = |sql: &str| -> Result<Vec<(String, String, f64)>, String> {
            let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
            let rows = stmt.query_map([&since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(|e| e.to_string())?;
            rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())
        };
        let by_month = grouped("SELECT substr(date, 1, 7), kind, SUM(kg) FROM recycling_log WHERE date >= ?1 GROUP BY 1, 2 ORDER BY 1")?;
        let by_material = grouped("SELECT material, kind, SUM(kg) FROM recycling_log WHERE date >= ?1 GROUP BY 1, 2 ORDER BY 3 DESC")?;
        let total = |k: &str| by_month.iter().filter(|(_, kind, _)| kind == k).map(|(_, _, kg)| kg).sum::<f64>();
        Ok(Summary {
            total_triturado: total("triturado"),
            total_recuperado: total("recuperado"),
            total_desecho: total("desecho"),
            entries,
            by_month,
            by_material,
        })
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct EntryReq {
    pub date: String,
    pub material: String,
    pub kind: String,
    pub kg: f64,
    #[serde(default)]
    pub notes: String,
}

/// POST /api/inventory/recycling
pub async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<EntryReq>,
) -> Result<(StatusCode, Json<Entry>), ApiError> {
    let bad = |m: &str| (StatusCode::BAD_REQUEST, m.to_string());
    let date = chrono::NaiveDate::parse_from_str(req.date.trim(), "%Y-%m-%d").map_err(|_| bad("Fecha invalida"))?;
    if date > chrono::Local::now().date_naive() {
        return Err(bad("La fecha no puede ser futura"));
    }
    let material = req.material.trim().to_string();
    if material.is_empty() || material.chars().count() > 60 {
        return Err(bad("Indica el material (maximo 60 caracteres)"));
    }
    if !KINDS.contains(&req.kind.as_str()) {
        return Err(bad("Tipo invalido: triturado, recuperado o desecho"));
    }
    if !(req.kg > 0.0 && req.kg <= 10000.0) {
        return Err(bad("Kilos invalidos"));
    }
    let notes = req.notes.trim().chars().take(500).collect::<String>();
    let user = session.username.clone();
    let e = db_op_status(&state.db, move |conn| {
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let d = date.format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT INTO recycling_log (id, date, material, kind, kg, notes, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, d, material, req.kind, req.kg, notes, user, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(internal)?;
        Ok(Entry { id, date: d, material, kind: req.kind, kg: req.kg, notes, created_by: user })
    })
    .await?;
    state.log_activity("Inventario", &format!("Reciclaje: {} kg de {} ({})", e.kg, e.material, e.kind), &session.username).await;
    Ok((StatusCode::CREATED, Json(e)))
}

/// DELETE /api/inventory/recycling/{id} — quien lo anoto o quien gestiona Inventario
pub async fn delete(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let staff = session.can("inventory", Level::Gestionar);
    let user = session.username.clone();
    db_op_status(&state.db, move |conn| {
        let by: String = conn
            .query_row("SELECT created_by FROM recycling_log WHERE id = ?1", [&id], |r| r.get(0))
            .optional()
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Registro no encontrado".to_string()))?;
        if by != user && !staff {
            return Err((StatusCode::FORBIDDEN, "Lo borra quien lo anoto o quien gestiona Inventario".into()));
        }
        conn.execute("DELETE FROM recycling_log WHERE id = ?1", [&id]).map_err(internal)?;
        Ok(())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
