//! Base colaborativa de parametros de corte (idea 29): por maquina, material, espesor y
//! operacion, la potencia, velocidad y pasadas que funcionaron, con foto del resultado. Cualquiera
//! que use Maquinas agrega los suyos; los demas marcan «me funciono». Lo edita o borra quien lo
//! subio o quien gestiona Maquinas. Ver docs/MAQUINAS.md.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::ApiError;
use crate::handlers::machines::{bad, db_err, decode_photo, forbidden, insert_photo, is_staff, load_machine, not_found, publish};
use crate::state::{AppState, SessionInfo};

pub const OPERATIONS: &[&str] = &["corte", "grabado", "marcado", "fresado", "otro"];
pub const RESULTS: &[&str] = &["bueno", "regular", "malo"];

#[derive(Debug, Clone, Serialize)]
pub struct CutParam {
    pub id: String,
    pub machine_id: String,
    pub machine_name: String,
    pub material: String,
    pub thickness_mm: Option<f64>,
    pub operation: String,
    /// Potencia en % (laser) o RPM (CNC), segun la maquina
    pub power: Option<f64>,
    /// mm/s (laser) o mm/min (CNC)
    pub speed: Option<f64>,
    pub passes: Option<i64>,
    pub notes: String,
    pub result: String,
    pub photo_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
    /// Cuantas personas marcaron «me funciono»
    pub votes: i64,
    /// Si la persona que pregunta lo marco
    pub voted: bool,
}

const SELECT: &str = "SELECT p.id, p.machine_id, COALESCE(m.name, ''), p.material, p.thickness_mm, p.operation, p.power, p.speed, p.passes,
    p.notes, p.result, p.photo_id, p.created_by, p.created_at,
    (SELECT COUNT(*) FROM cut_param_votes v WHERE v.param_id = p.id),
    EXISTS (SELECT 1 FROM cut_param_votes v WHERE v.param_id = p.id AND v.username = ?1)
    FROM cut_params p LEFT JOIN machines m ON m.id = p.machine_id";

fn row(r: &rusqlite::Row) -> rusqlite::Result<CutParam> {
    Ok(CutParam {
        id: r.get(0)?,
        machine_id: r.get(1)?,
        machine_name: r.get(2)?,
        material: r.get(3)?,
        thickness_mm: r.get(4)?,
        operation: r.get(5)?,
        power: r.get(6)?,
        speed: r.get(7)?,
        passes: r.get(8)?,
        notes: r.get(9)?,
        result: r.get(10)?,
        photo_id: r.get(11)?,
        created_by: r.get(12)?,
        created_at: r.get(13)?,
        votes: r.get(14)?,
        voted: r.get(15)?,
    })
}

fn load(conn: &Connection, id: &str, user: &str) -> rusqlite::Result<Option<CutParam>> {
    conn.query_row(&format!("{} WHERE p.id = ?2", SELECT), params![user, id], row).optional()
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    pub machine: Option<String>,
    /// Busca en material y notas
    pub q: Option<String>,
}

/// GET /api/machines/cut-params?machine=&q= — los que mas funcionaron primero
pub async fn list(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<CutParam>>, ApiError> {
    let user = session.username.clone();
    db_op(&state.db, move |conn| {
        let mut sql = format!("{} WHERE 1 = 1", SELECT);
        let mut args: Vec<String> = vec![user];
        if let Some(m) = q.machine.filter(|m| !m.is_empty()) {
            args.push(m);
            sql.push_str(&format!(" AND p.machine_id = ?{}", args.len()));
        }
        if let Some(text) = q.q.map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()) {
            args.push(format!("%{}%", text));
            sql.push_str(&format!(" AND (lower(p.material) LIKE ?{0} OR lower(p.notes) LIKE ?{0})", args.len()));
        }
        sql.push_str(
            " ORDER BY lower(p.material), p.thickness_mm, CASE p.result WHEN 'bueno' THEN 0 WHEN 'regular' THEN 1 ELSE 2 END,
              (SELECT COUNT(*) FROM cut_param_votes v WHERE v.param_id = p.id) DESC LIMIT 500",
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct ParamReq {
    pub material: String,
    pub thickness_mm: Option<f64>,
    #[serde(default = "default_op")]
    pub operation: String,
    pub power: Option<f64>,
    pub speed: Option<f64>,
    pub passes: Option<i64>,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "default_result")]
    pub result: String,
    /// Foto del resultado (data URL o base64)
    pub photo: Option<String>,
}

fn default_op() -> String {
    "corte".into()
}

fn default_result() -> String {
    "bueno".into()
}

fn validate(req: &mut ParamReq) -> Result<(), ApiError> {
    req.material = req.material.trim().to_string();
    req.notes = req.notes.trim().to_string();
    if req.material.is_empty() || req.material.chars().count() > 80 {
        return Err(bad("Indica el material (maximo 80 caracteres)"));
    }
    if req.notes.chars().count() > 2000 {
        return Err(bad("Notas demasiado largas"));
    }
    if !OPERATIONS.contains(&req.operation.as_str()) {
        return Err(bad("Operacion invalida"));
    }
    if !RESULTS.contains(&req.result.as_str()) {
        return Err(bad("Resultado invalido: bueno, regular o malo"));
    }
    let pos = |v: Option<f64>, max: f64| v.is_none_or(|x| (0.0..=max).contains(&x));
    if !pos(req.thickness_mm, 1000.0) || !pos(req.power, 100000.0) || !pos(req.speed, 1000000.0) {
        return Err(bad("Valores fuera de rango"));
    }
    if req.passes.is_some_and(|p| !(1..=100).contains(&p)) {
        return Err(bad("Pasadas: entre 1 y 100"));
    }
    if req.power.is_none() && req.speed.is_none() && req.notes.is_empty() {
        return Err(bad("Anota al menos potencia, velocidad o una nota"));
    }
    Ok(())
}

/// POST /api/machines/{id}/cut-params
pub async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(machine_id): Path<String>,
    Json(mut req): Json<ParamReq>,
) -> Result<(StatusCode, Json<CutParam>), ApiError> {
    validate(&mut req)?;
    let photo = req.photo.as_deref().filter(|p| !p.is_empty()).map(decode_photo).transpose()?;
    let user = session.username.clone();
    let p = db_op_status(&state.db, move |conn| {
        load_machine(conn, &machine_id).map_err(db_err)?.ok_or_else(not_found)?;
        let photo_id = match &photo {
            Some((bytes, mime)) => Some(insert_photo(conn, &machine_id, bytes, mime, &user).map_err(db_err)?),
            None => None,
        };
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        conn.execute(
            "INSERT INTO cut_params (id, machine_id, material, thickness_mm, operation, power, speed, passes, notes, result, photo_id, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id, machine_id, req.material, req.thickness_mm, req.operation, req.power, req.speed, req.passes, req.notes, req.result,
                photo_id, user, chrono::Local::now().naive_local().format("%Y-%m-%dT%H:%M").to_string()
            ],
        )
        .map_err(db_err)?;
        load(conn, &id, &user).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    state
        .log_activity("Maquinas", &format!("Parametros de {} en {}: {} {}", p.operation, p.machine_name, p.material, p.thickness_mm.map(|t| format!("{} mm", t)).unwrap_or_default()), &session.username)
        .await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(p)))
}

/// Solo quien lo subio o quien gestiona Maquinas
fn own_or_staff(conn: &Connection, id: &str, s: &SessionInfo) -> Result<CutParam, ApiError> {
    let p = load(conn, id, &s.username)
        .map_err(db_err)?
        .ok_or((StatusCode::NOT_FOUND, "Parametro no encontrado".to_string()))?;
    if p.created_by != s.username && !is_staff(s) {
        return Err(forbidden("Lo edita quien lo subio o quien gestiona Maquinas"));
    }
    Ok(p)
}

/// PUT /api/machines/cut-params/{id} (la foto no se cambia aqui)
pub async fn update(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(mut req): Json<ParamReq>,
) -> Result<Json<CutParam>, ApiError> {
    validate(&mut req)?;
    let s = session.clone();
    let p = db_op_status(&state.db, move |conn| {
        own_or_staff(conn, &id, &s)?;
        conn.execute(
            "UPDATE cut_params SET material = ?2, thickness_mm = ?3, operation = ?4, power = ?5, speed = ?6, passes = ?7, notes = ?8, result = ?9 WHERE id = ?1",
            params![id, req.material, req.thickness_mm, req.operation, req.power, req.speed, req.passes, req.notes, req.result],
        )
        .map_err(db_err)?;
        load(conn, &id, &s.username).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    publish(&state);
    Ok(Json(p))
}

/// DELETE /api/machines/cut-params/{id}
pub async fn delete(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let s = session.clone();
    let p = db_op_status(&state.db, move |conn| {
        let p = own_or_staff(conn, &id, &s)?;
        conn.execute("DELETE FROM cut_param_votes WHERE param_id = ?1", [&id]).map_err(db_err)?;
        conn.execute("DELETE FROM cut_params WHERE id = ?1", [&id]).map_err(db_err)?;
        Ok(p)
    })
    .await?;
    state.log_activity("Maquinas", &format!("Borro parametros de {}: {}", p.machine_name, p.material), &session.username).await;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/machines/cut-params/{id}/vote — «me funciono» (otra vez lo quita)
pub async fn vote(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<CutParam>, ApiError> {
    let user = session.username.clone();
    let p = db_op_status(&state.db, move |conn| {
        let p = load(conn, &id, &user).map_err(db_err)?.ok_or((StatusCode::NOT_FOUND, "Parametro no encontrado".to_string()))?;
        if p.voted {
            conn.execute("DELETE FROM cut_param_votes WHERE param_id = ?1 AND username = ?2", params![id, user]).map_err(db_err)?;
        } else {
            conn.execute("INSERT OR IGNORE INTO cut_param_votes (param_id, username) VALUES (?1, ?2)", params![id, user]).map_err(db_err)?;
        }
        load(conn, &id, &user).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    Ok(Json(p))
}
