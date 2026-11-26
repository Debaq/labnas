//! Horas dedicadas por persona y proyecto (idea 62): registro mensual con su contrato o
//! boleta de honorarios como respaldo y el monto. Lo aprueba quien gestiona Finanzas o el
//! responsable del proyecto, nunca la misma persona. Al aprobar cuenta como gastado
//! (`finance::set_commitment` con origen "honorario").

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{bad, clean, conflict, db, forbidden, not_found, opt_text};
use crate::db::db_op_status;
use crate::error::ApiError;
use crate::finance::FileRef;
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

const MAX_HOURS: f64 = 744.0;
const MAX_AMOUNT: f64 = 1.0e10;

#[derive(Debug, Clone, Serialize)]
pub struct HourRow {
    pub id: String,
    pub project_id: String,
    pub project: String,
    pub username: String,
    pub month: String,
    pub hours: f64,
    pub amount: f64,
    pub budget_item_id: Option<String>,
    pub notes: String,
    pub file: Option<FileRef>,
    pub status: String,
    pub created_by: String,
    pub created_at: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<String>,
    pub decision_note: String,
}

const SELECT: &str = "SELECT h.id, h.project_id, COALESCE(pe.title, ''), h.username, h.month, h.hours, h.amount, h.budget_item_id, h.notes,
        h.file_id, h.status, h.created_by, h.created_at, h.decided_by, h.decided_at, h.decision_note
    FROM fin_hours h LEFT JOIN portfolio_entries pe ON pe.id = h.project_id";

fn read(conn: &Connection, r: &rusqlite::Row) -> rusqlite::Result<HourRow> {
    let file_id: Option<String> = r.get(9)?;
    Ok(HourRow {
        id: r.get(0)?,
        project_id: r.get(1)?,
        project: r.get(2)?,
        username: r.get(3)?,
        month: r.get(4)?,
        hours: r.get(5)?,
        amount: r.get(6)?,
        budget_item_id: r.get(7)?,
        notes: r.get(8)?,
        file: match file_id {
            Some(id) => crate::finance::file_ref(conn, &id)?,
            None => None,
        },
        status: r.get(10)?,
        created_by: r.get(11)?,
        created_at: r.get(12)?,
        decided_by: r.get(13)?,
        decided_at: r.get(14)?,
        decision_note: r.get(15)?,
    })
}

fn get_row(conn: &Connection, id: &str) -> Result<Option<HourRow>, ApiError> {
    conn.query_row(&format!("{} WHERE h.id = ?1", SELECT), [id], |r| read(conn, r)).optional().map_err(db)
}

fn publish(state: &AppState) {
    state.events.publish("finance", (), crate::events::Audience::All, Some("finanzas"));
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    project_id: Option<String>,
    month: Option<String>,
    username: Option<String>,
    status: Option<String>,
}

/// GET /api/finance/hours
pub async fn list(State(state): State<AppState>, Query(q): Query<ListQuery>) -> Result<Json<Vec<HourRow>>, ApiError> {
    let (p, m, u, s) = (opt_text(&q.project_id), opt_text(&q.month), opt_text(&q.username), opt_text(&q.status));
    Ok(Json(
        db_op_status(&state.db, move |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "{} WHERE (?1 IS NULL OR h.project_id = ?1) AND (?2 IS NULL OR h.month = ?2) AND (?3 IS NULL OR h.username = ?3)
                     AND (?4 IS NULL OR h.status = ?4) ORDER BY h.month DESC, h.username, h.created_at DESC",
                    SELECT
                ))
                .map_err(db)?;
            let rows = stmt.query_map(params![p, m, u, s], |r| read(conn, r)).map_err(db)?.collect::<Result<Vec<_>, _>>().map_err(db)?;
            Ok(rows)
        })
        .await?,
    ))
}

#[derive(Debug, Serialize)]
pub struct SummaryRow {
    pub project_id: String,
    pub project: String,
    pub username: String,
    pub approved_hours: f64,
    pub approved_amount: f64,
    pub pending_hours: f64,
    pub pending_amount: f64,
}

/// GET /api/finance/hours/summary — horas y montos por proyecto y persona
pub async fn summary(State(state): State<AppState>, Query(q): Query<ListQuery>) -> Result<Json<Vec<SummaryRow>>, ApiError> {
    let p = opt_text(&q.project_id);
    Ok(Json(
        db_op_status(&state.db, move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT h.project_id, COALESCE(pe.title, ''), h.username,
                        COALESCE(SUM(CASE WHEN h.status = 'aprobada' THEN h.hours END), 0),
                        COALESCE(SUM(CASE WHEN h.status = 'aprobada' THEN h.amount END), 0),
                        COALESCE(SUM(CASE WHEN h.status = 'pendiente' THEN h.hours END), 0),
                        COALESCE(SUM(CASE WHEN h.status = 'pendiente' THEN h.amount END), 0)
                     FROM fin_hours h LEFT JOIN portfolio_entries pe ON pe.id = h.project_id
                     WHERE (?1 IS NULL OR h.project_id = ?1) AND h.status IN ('aprobada', 'pendiente')
                     GROUP BY h.project_id, h.username ORDER BY 2, h.username",
                )
                .map_err(db)?;
            let rows = stmt
                .query_map(params![p], |r| {
                    Ok(SummaryRow { project_id: r.get(0)?, project: r.get(1)?, username: r.get(2)?, approved_hours: r.get(3)?, approved_amount: r.get(4)?, pending_hours: r.get(5)?, pending_amount: r.get(6)? })
                })
                .map_err(db)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db)?;
            Ok(rows)
        })
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct UploadIn {
    name: String,
    data: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    project_id: String,
    username: Option<String>,
    month: String,
    hours: f64,
    amount: Option<f64>,
    budget_item_id: Option<String>,
    notes: Option<String>,
    file: Option<UploadIn>,
}

fn check_item(conn: &Connection, item: &str, project: &str) -> Result<(), ApiError> {
    let ok: bool = conn
        .query_row("SELECT 1 FROM fin_budget_items WHERE id = ?1 AND project_id = ?2", params![item, project], |_| Ok(true))
        .optional()
        .map_err(db)?
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(bad("El item de presupuesto no pertenece a ese proyecto"))
    }
}

/// POST /api/finance/hours — registra las horas de un mes (cada quien las suyas; quien gestiona, las de otros)
pub async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<CreateRequest>,
) -> Result<(StatusCode, Json<HourRow>), ApiError> {
    let me = session.username.clone();
    let username = opt_text(&req.username).unwrap_or_else(|| me.clone());
    if username != me && !session.can("finanzas", Level::Gestionar) {
        return Err(forbidden("Solo quien gestiona Finanzas registra horas de otra persona"));
    }
    let month = req.month.trim().to_string();
    if month.len() != 7 || NaiveDate::parse_from_str(&format!("{}-01", month), "%Y-%m-%d").is_err() {
        return Err(bad("Mes invalido (AAAA-MM)"));
    }
    if !req.hours.is_finite() || req.hours <= 0.0 || req.hours > MAX_HOURS {
        return Err(bad("Las horas deben ser mayores que 0 y caber en un mes"));
    }
    let amount = req.amount.unwrap_or(0.0);
    if !amount.is_finite() || !(0.0..=MAX_AMOUNT).contains(&amount) {
        return Err(bad("Monto invalido"));
    }
    let notes = clean(req.notes.as_deref().unwrap_or(""), 1000, "Notas")?;
    let upload = match &req.file {
        Some(f) => {
            let (bytes, mime) = crate::finance::decode_upload(&f.data)?;
            Some((f.name.clone(), bytes, mime))
        }
        None => None,
    };
    if amount > 0.0 && upload.is_none() {
        return Err(bad("Adjunte el contrato o la boleta de honorarios que respalda el monto"));
    }
    let (project_id, item) = (req.project_id.trim().to_string(), opt_text(&req.budget_item_id));
    let (u2, me2) = (username.clone(), me.clone());
    let row = db_op_status(&state.db, move |conn| {
        crate::finance::project_title(conn, &project_id).map_err(db)?.ok_or_else(|| bad("El proyecto no existe"))?;
        let closed: Option<String> = conn
            .query_row("SELECT closed_at FROM fin_projects WHERE project_id = ?1", [&project_id], |r| r.get(0))
            .optional()
            .map_err(db)?
            .flatten();
        if closed.is_some() {
            return Err(conflict("El proyecto esta cerrado"));
        }
        let exists: bool = conn.query_row("SELECT 1 FROM web_users WHERE username = ?1", [&u2], |_| Ok(true)).optional().map_err(db)?.unwrap_or(false);
        if !exists {
            return Err(bad("La persona no existe"));
        }
        if let Some(i) = &item {
            check_item(conn, i, &project_id)?;
        }
        let file_id = match &upload {
            Some((name, bytes, mime)) => Some(crate::finance::store_file(conn, name, bytes, mime, &me2).map_err(db)?.id),
            None => None,
        };
        let id = crate::finance::new_id();
        conn.execute(
            "INSERT INTO fin_hours (id, project_id, username, month, hours, amount, budget_item_id, notes, file_id, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, project_id, u2, month, req.hours, amount, item, notes, file_id, me2, crate::finance::now()],
        )
        .map_err(|_| conflict("Ya hay horas de esa persona en ese proyecto y mes (pendientes o aprobadas)"))?;
        get_row(conn, &id)?.ok_or_else(|| not_found("Registro no encontrado"))
    })
    .await?;
    state.log_activity("Finanzas", &format!("Horas registradas: {} {} {} h", row.username, row.month, row.hours), &me).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Debug, Deserialize, Default)]
pub struct DecideRequest {
    budget_item_id: Option<String>,
    note: Option<String>,
}

/// Quien decide: Gestionar en Finanzas o el responsable del proyecto, y nunca la misma
/// persona cuyas horas son ni quien las registro
fn can_decide(conn: &Connection, session: &SessionInfo, h: &HourRow) -> Result<(), ApiError> {
    let me = &session.username;
    let responsible: Option<String> =
        conn.query_row("SELECT responsible FROM fin_projects WHERE project_id = ?1", [&h.project_id], |r| r.get(0)).optional().map_err(db)?;
    let allowed = session.can("finanzas", Level::Gestionar) || responsible.as_deref().is_some_and(|r| !r.is_empty() && r == me);
    if !allowed {
        return Err(forbidden("Solo quien gestiona Finanzas o el responsable del proyecto decide sobre las horas"));
    }
    if *me == h.username || *me == h.created_by {
        return Err(forbidden("Separacion de funciones: no puede decidir sobre horas propias ni sobre las que usted registro"));
    }
    Ok(())
}

/// POST /api/finance/hours/{id}/approve — cuenta como gastado en el item de presupuesto
pub async fn approve(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    body: Option<Json<DecideRequest>>,
) -> Result<Json<HourRow>, ApiError> {
    let req = body.map(|b| b.0).unwrap_or_default();
    let note = clean(req.note.as_deref().unwrap_or(""), 500, "Nota")?;
    let (me, sess) = (session.username.clone(), session.clone());
    let override_item = opt_text(&req.budget_item_id);
    let id2 = id.clone();
    let row = db_op_status(&state.db, move |conn| {
        let h = get_row(conn, &id2)?.ok_or_else(|| not_found("Registro de horas no encontrado"))?;
        can_decide(conn, &sess, &h)?;
        if h.status != "pendiente" {
            return Err(conflict("Solo se aprueban horas pendientes"));
        }
        let item = override_item.or(h.budget_item_id.clone());
        if let Some(i) = &item {
            check_item(conn, i, &h.project_id)?;
        }
        conn.execute(
            "UPDATE fin_hours SET status = 'aprobada', decided_by = ?1, decided_at = ?2, decision_note = ?3, budget_item_id = ?4 WHERE id = ?5",
            params![me, crate::finance::now(), note, item, h.id],
        )
        .map_err(db)?;
        crate::finance::set_commitment(
            conn,
            "honorario",
            &h.id,
            &h.project_id,
            item.as_deref(),
            &format!("Honorarios {} {}", h.username, h.month),
            h.amount,
            h.amount,
        )
        .map_err(db)?;
        get_row(conn, &h.id)?.ok_or_else(|| not_found("Registro no encontrado"))
    })
    .await?;
    let seq = crate::finance::ledger(
        &state,
        &session.username,
        "horas",
        &format!("horas {} {}", row.username, row.month),
        json!({
            "accion": "aprobada", "id": row.id, "project_id": row.project_id, "persona": row.username, "mes": row.month,
            "horas": row.hours, "monto": row.amount, "item": row.budget_item_id, "respaldo_sha256": row.file.as_ref().map(|f| f.sha256.clone()),
        }),
    )
    .await?;
    let rid = row.id.clone();
    db_op_status(&state.db, move |conn| conn.execute("UPDATE fin_hours SET ledger_seq = ?1 WHERE id = ?2", params![seq, rid]).map(|_| ()).map_err(db)).await?;
    state.log_activity("Finanzas", &format!("Horas aprobadas: {} {} ({} h)", row.username, row.month, row.hours), &session.username).await;
    publish(&state);
    Ok(Json(row))
}

/// POST /api/finance/hours/{id}/reject — rechaza con motivo
pub async fn reject(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<DecideRequest>,
) -> Result<Json<HourRow>, ApiError> {
    let note = clean(req.note.as_deref().unwrap_or(""), 500, "Motivo")?;
    if note.is_empty() {
        return Err(bad("Indique el motivo del rechazo"));
    }
    let (me, sess) = (session.username.clone(), session.clone());
    let row = db_op_status(&state.db, move |conn| {
        let h = get_row(conn, &id)?.ok_or_else(|| not_found("Registro de horas no encontrado"))?;
        can_decide(conn, &sess, &h)?;
        if h.status != "pendiente" {
            return Err(conflict("Solo se rechazan horas pendientes"));
        }
        conn.execute(
            "UPDATE fin_hours SET status = 'rechazada', decided_by = ?1, decided_at = ?2, decision_note = ?3 WHERE id = ?4",
            params![me, crate::finance::now(), note, h.id],
        )
        .map_err(db)?;
        get_row(conn, &h.id)?.ok_or_else(|| not_found("Registro no encontrado"))
    })
    .await?;
    finish(&state, &session.username, &row, "rechazada").await?;
    Ok(Json(row))
}

/// POST /api/finance/hours/{id}/annul — anula unas horas (Gestionar), con motivo; deja de contar
pub async fn annul(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<DecideRequest>,
) -> Result<Json<HourRow>, ApiError> {
    if !session.can("finanzas", Level::Gestionar) {
        return Err(forbidden("Solo quien gestiona Finanzas anula horas"));
    }
    let note = clean(req.note.as_deref().unwrap_or(""), 500, "Motivo")?;
    if note.is_empty() {
        return Err(bad("Indique el motivo de la anulacion"));
    }
    let me = session.username.clone();
    let row = db_op_status(&state.db, move |conn| {
        let h = get_row(conn, &id)?.ok_or_else(|| not_found("Registro de horas no encontrado"))?;
        if !matches!(h.status.as_str(), "pendiente" | "aprobada") {
            return Err(conflict("Esas horas ya estan cerradas"));
        }
        if h.status == "aprobada" {
            crate::finance::set_commitment(conn, "honorario", &h.id, &h.project_id, h.budget_item_id.as_deref(), &format!("Honorarios {} {} (anulado)", h.username, h.month), 0.0, 0.0)
                .map_err(db)?;
        }
        conn.execute(
            "UPDATE fin_hours SET status = 'anulada', decided_by = ?1, decided_at = ?2, decision_note = ?3 WHERE id = ?4",
            params![me, crate::finance::now(), note, h.id],
        )
        .map_err(db)?;
        get_row(conn, &h.id)?.ok_or_else(|| not_found("Registro no encontrado"))
    })
    .await?;
    finish(&state, &session.username, &row, "anulada").await?;
    Ok(Json(row))
}

async fn finish(state: &AppState, user: &str, row: &HourRow, action: &str) -> Result<(), ApiError> {
    let seq = crate::finance::ledger(
        state,
        user,
        "horas",
        &format!("horas {} {}", row.username, row.month),
        json!({ "accion": action, "id": row.id, "project_id": row.project_id, "persona": row.username, "mes": row.month, "horas": row.hours, "monto": row.amount, "motivo": row.decision_note }),
    )
    .await?;
    let rid = row.id.clone();
    db_op_status(&state.db, move |conn| conn.execute("UPDATE fin_hours SET ledger_seq = ?1 WHERE id = ?2", params![seq, rid]).map(|_| ()).map_err(db)).await?;
    state.log_activity("Finanzas", &format!("Horas {}: {} {}", action, row.username, row.month), user).await;
    publish(state);
    Ok(())
}
