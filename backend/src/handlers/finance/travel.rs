//! Viajes y viaticos (idea 63). Solicitud (Usar), aprobacion o rechazo (Gestionar, nunca quien
//! pidio ni quien viaja), comprobantes con respaldo y monto, informe de comision obligatorio
//! para cerrar. Lo aprobado queda como comprometido; al cerrar, lo gastado es la suma de los
//! comprobantes. Ver docs/FINANZAS.md.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::db_op_status;
use crate::error::ApiError;
use crate::finance::{self, FileRef};
use crate::handlers::machines::{bad, conflict, db_err, forbidden};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

const RECEIPT_KINDS: &[&str] = &["pasaje", "alojamiento", "viatico", "otro", "informe"];
const MAX_AMOUNT: f64 = 1.0e10;
const MIN_REPORT: usize = 20;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/finance/trips", get(list).post(create))
        .route("/api/finance/trips/{id}", get(detail).put(update))
        .route("/api/finance/trips/{id}/approve", post(approve))
        .route("/api/finance/trips/{id}/reject", post(reject))
        .route("/api/finance/trips/{id}/cancel", post(cancel))
        .route("/api/finance/trips/{id}/receipts", post(add_receipt))
        .route("/api/finance/trips/{id}/receipts/{rid}", delete(remove_receipt))
        .route("/api/finance/trips/{id}/report", put(set_report))
        .route("/api/finance/trips/{id}/close", post(close))
}

fn not_found() -> ApiError {
    (StatusCode::NOT_FOUND, "Viaje no encontrado".into())
}

fn publish(state: &AppState) {
    state.events.publish("finance", (), crate::events::Audience::All, Some("finanzas"));
}

fn staff(s: &SessionInfo) -> bool {
    s.can("finanzas", Level::Gestionar)
}

fn clean(s: &str, max: usize, what: &str) -> Result<String, ApiError> {
    let s = s.trim().to_string();
    if s.chars().count() > max {
        return Err(bad(format!("{} demasiado largo (maximo {} caracteres)", what, max)));
    }
    Ok(s)
}

fn required(s: &str, max: usize, what: &str) -> Result<String, ApiError> {
    let s = clean(s, max, what)?;
    if s.is_empty() {
        return Err(bad(format!("{} es obligatorio", what)));
    }
    Ok(s)
}

fn parse_date(s: &str) -> Result<String, ApiError> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map(|d| d.format("%Y-%m-%d").to_string()).map_err(|_| bad("Fecha invalida (AAAA-MM-DD)"))
}

fn amount(v: f64, what: &str) -> Result<f64, ApiError> {
    if !v.is_finite() || !(0.0..=MAX_AMOUNT).contains(&v) {
        return Err(bad(format!("{}: monto invalido", what)));
    }
    Ok(v.round())
}

#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub id: String,
    pub kind: String,
    pub description: String,
    pub amount: f64,
    pub file: Option<FileRef>,
    pub added_by: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub step: String,
    pub username: String,
    pub at: String,
    pub note: String,
    pub ledger_seq: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Trip {
    pub id: String,
    pub folio: i64,
    pub project_id: String,
    pub project_title: String,
    pub budget_item_id: Option<String>,
    pub traveler: String,
    pub traveler_user: Option<String>,
    pub destination: String,
    pub start_date: String,
    pub end_date: String,
    pub purpose: String,
    pub est_transport: f64,
    pub est_lodging: f64,
    pub est_per_diem: f64,
    pub est_total: f64,
    pub status: String,
    pub requested_by: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<String>,
    pub decision_note: String,
    pub report_text: String,
    pub closed_by: Option<String>,
    pub closed_at: Option<String>,
    /// Suma de comprobantes (en vivo mientras esta abierto; fija al cerrar)
    pub spent: f64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipts: Option<Vec<Receipt>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<Step>>,
}

const SELECT: &str = "SELECT t.id, t.folio, t.project_id, COALESCE(p.title, ''), t.budget_item_id, t.traveler, t.traveler_user, t.destination,
    t.start_date, t.end_date, t.purpose, t.est_transport, t.est_lodging, t.est_per_diem, t.status, t.requested_by, t.decided_by, t.decided_at,
    t.decision_note, t.report_text, t.closed_by, t.closed_at,
    COALESCE(t.total_spent, (SELECT COALESCE(SUM(amount), 0) FROM fin_trip_receipts r WHERE r.trip_id = t.id AND r.kind != 'informe')), t.created_at
    FROM fin_trips t LEFT JOIN portfolio_entries p ON p.id = t.project_id";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Trip> {
    let (a, b, c): (f64, f64, f64) = (r.get(11)?, r.get(12)?, r.get(13)?);
    Ok(Trip {
        id: r.get(0)?, folio: r.get(1)?, project_id: r.get(2)?, project_title: r.get(3)?, budget_item_id: r.get(4)?, traveler: r.get(5)?,
        traveler_user: r.get(6)?, destination: r.get(7)?, start_date: r.get(8)?, end_date: r.get(9)?, purpose: r.get(10)?,
        est_transport: a, est_lodging: b, est_per_diem: c, est_total: a + b + c, status: r.get(14)?, requested_by: r.get(15)?,
        decided_by: r.get(16)?, decided_at: r.get(17)?, decision_note: r.get(18)?, report_text: r.get(19)?, closed_by: r.get(20)?,
        closed_at: r.get(21)?, spent: r.get(22)?, created_at: r.get(23)?, receipts: None, steps: None,
    })
}

fn load(conn: &Connection, id: &str) -> Result<Option<Trip>, ApiError> {
    conn.query_row(&format!("{} WHERE t.id = ?1", SELECT), [id], row).optional().map_err(db_err)
}

fn load_full(conn: &Connection, id: &str) -> Result<Trip, ApiError> {
    let mut t = load(conn, id)?.ok_or_else(not_found)?;
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.kind, r.description, r.amount, r.added_by, r.added_at, f.id, f.name, f.mime, f.size, f.sha256
               FROM fin_trip_receipts r LEFT JOIN fin_files f ON f.id = r.file_id WHERE r.trip_id = ?1 ORDER BY r.added_at, r.rowid",
        )
        .map_err(db_err)?;
    let receipts = stmt
        .query_map([id], |r| {
            let file = match r.get::<_, Option<String>>(6)? {
                Some(fid) => Some(FileRef { id: fid, name: r.get(7)?, mime: r.get(8)?, size: r.get(9)?, sha256: r.get(10)? }),
                None => None,
            };
            Ok(Receipt { id: r.get(0)?, kind: r.get(1)?, description: r.get(2)?, amount: r.get(3)?, added_by: r.get(4)?, added_at: r.get(5)?, file })
        })
        .map_err(db_err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_err)?;
    let mut stmt = conn.prepare("SELECT step, username, at, note, ledger_seq FROM fin_trip_steps WHERE trip_id = ?1 ORDER BY at, rowid").map_err(db_err)?;
    let steps = stmt
        .query_map([id], |r| Ok(Step { step: r.get(0)?, username: r.get(1)?, at: r.get(2)?, note: r.get(3)?, ledger_seq: r.get(4)? }))
        .map_err(db_err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_err)?;
    t.receipts = Some(receipts);
    t.steps = Some(steps);
    Ok(t)
}

fn add_step(conn: &Connection, trip_id: &str, step: &str, user: &str, note: &str) -> Result<String, ApiError> {
    let id = finance::new_id();
    conn.execute(
        "INSERT INTO fin_trip_steps (id, trip_id, step, username, at, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, trip_id, step, user, finance::now(), note],
    )
    .map_err(db_err)?;
    Ok(id)
}

async fn stamp(state: &AppState, step_id: String, seq: i64) -> Result<(), ApiError> {
    db_op_status(&state.db, move |conn| {
        conn.execute("UPDATE fin_trip_steps SET ledger_seq = ?2 WHERE id = ?1", params![step_id, seq]).map(|_| ()).map_err(db_err)
    })
    .await
}

fn same(a: &str, b: &str) -> bool {
    !a.is_empty() && a.trim().eq_ignore_ascii_case(b.trim())
}

/// Es quien pidio o quien viaja
fn involved(t: &Trip, user: &str) -> bool {
    same(&t.requested_by, user) || t.traveler_user.as_deref().is_some_and(|u| same(u, user)) || same(&t.traveler, user)
}

/// Quien pidio, quien viaja o quien gestiona Finanzas
fn can_work_on(t: &Trip, s: &SessionInfo) -> Result<(), ApiError> {
    if involved(t, &s.username) || staff(s) {
        Ok(())
    } else {
        Err(forbidden("Solo quien pidio el viaje, quien viaja o quien gestiona Finanzas puede hacerlo"))
    }
}

fn need_status(t: &Trip, ok: &[&str], what: &str) -> Result<(), ApiError> {
    if ok.contains(&t.status.as_str()) {
        Ok(())
    } else {
        Err(conflict(format!("{} (el viaje esta en estado «{}»)", what, t.status)))
    }
}

fn sync_commitment(conn: &Connection, t: &Trip, committed: f64, spent: f64) -> Result<(), ApiError> {
    let label = format!("Viaje V-{} a {}", t.folio, t.destination);
    finance::set_commitment(conn, "viaje", &t.id, &t.project_id, t.budget_item_id.as_deref(), &label, committed, spent).map_err(db_err)
}

fn staff_users(conn: &Connection) -> Vec<String> {
    crate::roles::users_with_level(conn, "finanzas", Level::Gestionar).unwrap_or_default()
}

fn tell(state: &AppState, users: Vec<String>, text: String) {
    let st = state.clone();
    tokio::spawn(async move {
        crate::handlers::notifications::notify_users(&st, &users, &text).await;
    });
}

// ─── Lista y ficha ───

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// GET /api/finance/trips?project_id=&status=
pub async fn list(State(state): State<AppState>, Query(q): Query<ListQuery>) -> Result<Json<Vec<Trip>>, ApiError> {
    let list = db_op_status(&state.db, move |conn| {
        let sql = format!("{} WHERE (?1 IS NULL OR t.project_id = ?1) AND (?2 IS NULL OR t.status = ?2) ORDER BY t.folio DESC LIMIT 500", SELECT);
        let mut stmt = conn.prepare(&sql).map_err(db_err)?;
        let rows = stmt.query_map(params![q.project_id.filter(|p| !p.is_empty()), q.status.filter(|p| !p.is_empty())], row).map_err(db_err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
    })
    .await?;
    Ok(Json(list))
}

/// GET /api/finance/trips/{id}
pub async fn detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Trip>, ApiError> {
    Ok(Json(db_op_status(&state.db, move |conn| load_full(conn, &id)).await?))
}

// ─── Solicitud ───

#[derive(Deserialize)]
pub struct TripReq {
    pub project_id: String,
    #[serde(default)]
    pub budget_item_id: Option<String>,
    pub traveler: String,
    /// Cuenta de LabNAS de quien viaja, si tiene
    #[serde(default)]
    pub traveler_user: Option<String>,
    pub destination: String,
    pub start_date: String,
    pub end_date: String,
    pub purpose: String,
    #[serde(default)]
    pub est_transport: f64,
    #[serde(default)]
    pub est_lodging: f64,
    #[serde(default)]
    pub est_per_diem: f64,
}

struct Checked {
    project_id: String,
    budget_item_id: Option<String>,
    traveler: String,
    traveler_user: Option<String>,
    destination: String,
    start: String,
    end: String,
    purpose: String,
    est: [f64; 3],
}

fn check(req: &TripReq) -> Result<Checked, ApiError> {
    let (start, end) = (parse_date(&req.start_date)?, parse_date(&req.end_date)?);
    if end < start {
        return Err(bad("La fecha de regreso es anterior a la de salida"));
    }
    let est = [amount(req.est_transport, "Pasajes")?, amount(req.est_lodging, "Alojamiento")?, amount(req.est_per_diem, "Viatico")?];
    if est.iter().sum::<f64>() <= 0.0 {
        return Err(bad("Indica al menos un monto estimado"));
    }
    Ok(Checked {
        project_id: required(&req.project_id, 60, "El proyecto")?,
        budget_item_id: req.budget_item_id.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from),
        traveler: required(&req.traveler, 120, "Quien viaja")?,
        traveler_user: req.traveler_user.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from),
        destination: required(&req.destination, 160, "El destino")?,
        start,
        end,
        purpose: required(&req.purpose, 2000, "El motivo")?,
        est,
    })
}

fn check_refs(conn: &Connection, c: &Checked) -> Result<(), ApiError> {
    if finance::project_title(conn, &c.project_id).map_err(db_err)?.is_none() {
        return Err(bad("El proyecto no existe"));
    }
    if let Some(b) = &c.budget_item_id {
        let ok: bool = conn
            .query_row("SELECT 1 FROM fin_budget_items WHERE id = ?1 AND project_id = ?2", params![b, c.project_id], |_| Ok(()))
            .optional()
            .map_err(db_err)?
            .is_some();
        if !ok {
            return Err(bad("El item de presupuesto no es de ese proyecto"));
        }
    }
    if let Some(u) = &c.traveler_user {
        let ok: bool = conn.query_row("SELECT 1 FROM web_users WHERE username = ?1", [u], |_| Ok(())).optional().map_err(db_err)?.is_some();
        if !ok {
            return Err(bad("La cuenta de quien viaja no existe"));
        }
    }
    Ok(())
}

/// POST /api/finance/trips (Usar): pide un viaje
pub async fn create(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Json(req): Json<TripReq>) -> Result<(StatusCode, Json<Trip>), ApiError> {
    let c = check(&req)?;
    let (user, id) = (session.username.clone(), finance::new_id());
    let (trip, staff_list) = db_op_status(&state.db, move |conn| {
        check_refs(conn, &c)?;
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate).map_err(db_err)?;
        let folio: i64 = tx.query_row("SELECT COALESCE(MAX(folio), 0) + 1 FROM fin_trips", [], |r| r.get(0)).map_err(db_err)?;
        let now = finance::now();
        tx.execute(
            "INSERT INTO fin_trips (id, folio, project_id, budget_item_id, traveler, traveler_user, destination, start_date, end_date, purpose,
                 est_transport, est_lodging, est_per_diem, status, requested_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'solicitud', ?14, ?15, ?15)",
            params![id, folio, c.project_id, c.budget_item_id, c.traveler, c.traveler_user, c.destination, c.start, c.end, c.purpose, c.est[0], c.est[1], c.est[2], user, now],
        )
        .map_err(db_err)?;
        add_step(&tx, &id, "solicitud", &user, "")?;
        tx.commit().map_err(db_err)?;
        Ok((load_full(conn, &id)?, staff_users(conn)))
    })
    .await?;
    state.log_activity("Finanzas", &format!("Viaje solicitado: V-{} a {}", trip.folio, trip.destination), &session.username).await;
    let to: Vec<String> = staff_list.into_iter().filter(|u| !involved(&trip, u)).collect();
    tell(&state, to, format!("Nueva solicitud de viaje V-{} a {} ({}). Esta esperando aprobacion en Finanzas.", trip.folio, trip.destination, trip.project_title));
    publish(&state);
    Ok((StatusCode::CREATED, Json(trip)))
}

/// PUT /api/finance/trips/{id} (Usar): corrige la solicitud mientras no se decide
pub async fn update(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>, Json(req): Json<TripReq>) -> Result<Json<Trip>, ApiError> {
    let c = check(&req)?;
    let s = session.clone();
    let trip = db_op_status(&state.db, move |conn| {
        let t = load(conn, &id)?.ok_or_else(not_found)?;
        can_work_on(&t, &s)?;
        need_status(&t, &["solicitud"], "Solo se corrige una solicitud pendiente")?;
        check_refs(conn, &c)?;
        conn.execute(
            "UPDATE fin_trips SET project_id = ?2, budget_item_id = ?3, traveler = ?4, traveler_user = ?5, destination = ?6, start_date = ?7, end_date = ?8,
                 purpose = ?9, est_transport = ?10, est_lodging = ?11, est_per_diem = ?12, updated_at = ?13 WHERE id = ?1",
            params![id, c.project_id, c.budget_item_id, c.traveler, c.traveler_user, c.destination, c.start, c.end, c.purpose, c.est[0], c.est[1], c.est[2], finance::now()],
        )
        .map_err(db_err)?;
        load_full(conn, &id)
    })
    .await?;
    publish(&state);
    Ok(Json(trip))
}

// ─── Decision ───

#[derive(Deserialize, Default)]
pub struct NoteReq {
    #[serde(default)]
    pub note: String,
}

/// Quien decide: gestiona Finanzas y no es quien pidio ni quien viaja
fn check_decider(t: &Trip, s: &SessionInfo, verb: &str) -> Result<(), ApiError> {
    if !staff(s) {
        return Err(forbidden("Solo quien gestiona Finanzas decide los viajes"));
    }
    if involved(t, &s.username) {
        return Err(forbidden(&format!("No puedes {} un viaje que tu pediste o en el que viajas: debe hacerlo otra persona", verb)));
    }
    Ok(())
}

async fn decide(state: &AppState, session: &SessionInfo, id: String, approve: bool, note: String) -> Result<Json<Trip>, ApiError> {
    let note = clean(&note, 1000, "La nota")?;
    if !approve && note.is_empty() {
        return Err(bad("Indica el motivo del rechazo"));
    }
    let s = session.clone();
    let (trip, step_id) = db_op_status(&state.db, move |conn| {
        let t = load(conn, &id)?.ok_or_else(not_found)?;
        check_decider(&t, &s, if approve { "aprobar" } else { "rechazar" })?;
        need_status(&t, &["solicitud"], "Solo se decide una solicitud pendiente")?;
        let status = if approve { "aprobado" } else { "rechazado" };
        let now = finance::now();
        conn.execute(
            "UPDATE fin_trips SET status = ?2, decided_by = ?3, decided_at = ?4, decision_note = ?5, updated_at = ?4 WHERE id = ?1",
            params![id, status, s.username, now, note],
        )
        .map_err(db_err)?;
        let step = add_step(conn, &id, status, &s.username, &note)?;
        let t = load_full(conn, &id)?;
        // Aprobado: el estimado queda comprometido. Rechazado: nada.
        if approve {
            sync_commitment(conn, &t, t.est_total, 0.0)?;
        }
        Ok((t, step))
    })
    .await?;
    let seq = finance::ledger(
        state,
        &session.username,
        "viaje",
        &format!("viaje:V-{}", trip.folio),
        json!({
            "accion": trip.status, "proyecto": trip.project_title, "destino": trip.destination, "viaja": trip.traveler,
            "pidio": trip.requested_by, "estimado": { "pasajes": trip.est_transport, "alojamiento": trip.est_lodging, "viatico": trip.est_per_diem, "total": trip.est_total },
            "nota": note_of(&trip),
        }),
    )
    .await?;
    stamp(state, step_id, seq).await?;
    state
        .log_activity("Finanzas", &format!("Viaje V-{} {}", trip.folio, if approve { "aprobado" } else { "rechazado" }), &session.username)
        .await;
    let mut to = vec![trip.requested_by.clone()];
    to.extend(trip.traveler_user.clone());
    to.dedup();
    tell(state, to, format!("Tu viaje V-{} a {} fue {}.", trip.folio, trip.destination, if approve { "aprobado" } else { "rechazado" }));
    publish(state);
    Ok(Json(trip))
}

fn note_of(t: &Trip) -> String {
    t.decision_note.clone()
}

/// POST /api/finance/trips/{id}/approve (Gestionar; nunca quien pidio ni quien viaja)
pub async fn approve(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>, body: Option<Json<NoteReq>>) -> Result<Json<Trip>, ApiError> {
    decide(&state, &session, id, true, body.map(|b| b.0.note).unwrap_or_default()).await
}

/// POST /api/finance/trips/{id}/reject (Gestionar; con motivo)
pub async fn reject(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>, body: Option<Json<NoteReq>>) -> Result<Json<Trip>, ApiError> {
    decide(&state, &session, id, false, body.map(|b| b.0.note).unwrap_or_default()).await
}

#[derive(Deserialize)]
pub struct ReasonReq {
    #[serde(default)]
    pub reason: String,
}

/// POST /api/finance/trips/{id}/cancel (Usar): anula con motivo; lo comprometido vuelve a 0
pub async fn cancel(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>, Json(req): Json<ReasonReq>) -> Result<Json<Trip>, ApiError> {
    let reason = required(&req.reason, 1000, "El motivo")?;
    let (s, why) = (session.clone(), reason.clone());
    let (trip, step_id) = db_op_status(&state.db, move |conn| {
        let t = load(conn, &id)?.ok_or_else(not_found)?;
        can_work_on(&t, &s)?;
        need_status(&t, &["solicitud", "aprobado"], "Solo se anula un viaje pendiente o aprobado sin cerrar")?;
        conn.execute("UPDATE fin_trips SET status = 'anulado', updated_at = ?2 WHERE id = ?1", params![id, finance::now()]).map_err(db_err)?;
        let step = add_step(conn, &id, "anulado", &s.username, &why)?;
        if t.status == "aprobado" {
            sync_commitment(conn, &t, 0.0, 0.0)?;
        }
        Ok((load_full(conn, &id)?, step))
    })
    .await?;
    let seq = finance::ledger(&state, &session.username, "viaje", &format!("viaje:V-{}", trip.folio), json!({ "accion": "anulado", "motivo": reason })).await?;
    stamp(&state, step_id, seq).await?;
    state.log_activity("Finanzas", &format!("Viaje V-{} anulado", trip.folio), &session.username).await;
    publish(&state);
    Ok(Json(trip))
}

// ─── Comprobantes e informe ───

#[derive(Deserialize)]
pub struct UploadReq {
    pub name: String,
    pub data: String,
}

#[derive(Deserialize)]
pub struct ReceiptReq {
    /// pasaje | alojamiento | viatico | otro | informe (adjunto del informe de comision, sin monto)
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub amount: f64,
    #[serde(default)]
    pub file: Option<UploadReq>,
}

/// POST /api/finance/trips/{id}/receipts (Usar): comprobante con monto y respaldo
pub async fn add_receipt(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>, Json(req): Json<ReceiptReq>) -> Result<(StatusCode, Json<Trip>), ApiError> {
    let kind = req.kind.trim().to_string();
    if !RECEIPT_KINDS.contains(&kind.as_str()) {
        return Err(bad("Tipo de comprobante invalido"));
    }
    let description = clean(&req.description, 300, "La descripcion")?;
    let attachment = kind == "informe";
    let amt = if attachment { 0.0 } else { amount(req.amount, "Comprobante")? };
    if !attachment && amt <= 0.0 {
        return Err(bad("Indica el monto del comprobante"));
    }
    // El viatico se paga por dia y no siempre tiene boleta; el resto necesita su respaldo
    if req.file.is_none() && kind != "viatico" {
        return Err(bad("Adjunta el respaldo (boleta, factura, pasaje o el archivo del informe)"));
    }
    let upload = req.file.map(|f| finance::decode_upload(&f.data).map(|(b, m)| (f.name, b, m))).transpose()?;
    let s = session.clone();
    let (trip, sha) = db_op_status(&state.db, move |conn| {
        let t = load(conn, &id)?.ok_or_else(not_found)?;
        can_work_on(&t, &s)?;
        need_status(&t, &["aprobado"], "Los comprobantes se agregan a un viaje aprobado y sin cerrar")?;
        let file = upload.map(|(name, bytes, mime)| finance::store_file(conn, &name, &bytes, mime, &s.username)).transpose().map_err(db_err)?;
        conn.execute(
            "INSERT INTO fin_trip_receipts (id, trip_id, kind, description, amount, file_id, added_by, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![finance::new_id(), id, kind, description, amt, file.as_ref().map(|f| f.id.clone()), s.username, finance::now()],
        )
        .map_err(db_err)?;
        Ok((load_full(conn, &id)?, file.map(|f| f.sha256)))
    })
    .await?;
    state.log_activity("Finanzas", &format!("Comprobante agregado al viaje V-{} ({})", trip.folio, sha.as_deref().map(|h| &h[..12]).unwrap_or("sin archivo")), &session.username).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(trip)))
}

/// DELETE /api/finance/trips/{id}/receipts/{rid} (Usar): solo mientras el viaje no se cierra
/// (el respaldo queda guardado en el archivo)
pub async fn remove_receipt(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path((id, rid)): Path<(String, String)>) -> Result<Json<Trip>, ApiError> {
    let s = session.clone();
    let trip = db_op_status(&state.db, move |conn| {
        let t = load(conn, &id)?.ok_or_else(not_found)?;
        can_work_on(&t, &s)?;
        need_status(&t, &["aprobado"], "Solo se quitan comprobantes de un viaje abierto")?;
        let n = conn.execute("DELETE FROM fin_trip_receipts WHERE id = ?1 AND trip_id = ?2", params![rid, id]).map_err(db_err)?;
        if n == 0 {
            return Err((StatusCode::NOT_FOUND, "Comprobante no encontrado".into()));
        }
        load_full(conn, &id)
    })
    .await?;
    publish(&state);
    Ok(Json(trip))
}

#[derive(Deserialize)]
pub struct ReportReq {
    pub text: String,
}

/// PUT /api/finance/trips/{id}/report (Usar): texto del informe de comision
pub async fn set_report(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>, Json(req): Json<ReportReq>) -> Result<Json<Trip>, ApiError> {
    let text = clean(&req.text, 10000, "El informe")?;
    let s = session.clone();
    let trip = db_op_status(&state.db, move |conn| {
        let t = load(conn, &id)?.ok_or_else(not_found)?;
        can_work_on(&t, &s)?;
        need_status(&t, &["aprobado"], "El informe se escribe en un viaje aprobado y sin cerrar")?;
        conn.execute("UPDATE fin_trips SET report_text = ?2, updated_at = ?3 WHERE id = ?1", params![id, text, finance::now()]).map_err(db_err)?;
        load_full(conn, &id)
    })
    .await?;
    publish(&state);
    Ok(Json(trip))
}

/// POST /api/finance/trips/{id}/close (Usar): cierra con el total gastado; exige el informe
pub async fn close(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>) -> Result<Json<Trip>, ApiError> {
    let s = session.clone();
    let (trip, step_id, hashes) = db_op_status(&state.db, move |conn| {
        let t = load(conn, &id)?.ok_or_else(not_found)?;
        can_work_on(&t, &s)?;
        need_status(&t, &["aprobado"], "Solo se cierra un viaje aprobado")?;
        if t.report_text.trim().chars().count() < MIN_REPORT {
            return Err(bad(format!("Falta el informe de comision (al menos {} caracteres) para cerrar el viaje", MIN_REPORT)));
        }
        let spent: f64 = conn
            .query_row("SELECT COALESCE(SUM(amount), 0) FROM fin_trip_receipts WHERE trip_id = ?1 AND kind != 'informe'", [&id], |r| r.get(0))
            .map_err(db_err)?;
        let now = finance::now();
        conn.execute(
            "UPDATE fin_trips SET status = 'cerrado', closed_by = ?2, closed_at = ?3, total_spent = ?4, updated_at = ?3 WHERE id = ?1",
            params![id, s.username, now, spent],
        )
        .map_err(db_err)?;
        let step = add_step(conn, &id, "cerrado", &s.username, "")?;
        let closed = load_full(conn, &id)?;
        sync_commitment(conn, &closed, 0.0, spent)?;
        let hashes: Vec<serde_json::Value> = closed
            .receipts
            .iter()
            .flatten()
            .map(|r| json!({ "tipo": r.kind, "monto": r.amount, "sha256": r.file.as_ref().map(|f| f.sha256.clone()) }))
            .collect();
        Ok((closed, step, hashes))
    })
    .await?;
    let seq = finance::ledger(
        &state,
        &session.username,
        "viaje",
        &format!("viaje:V-{}", trip.folio),
        json!({
            "accion": "cerrado", "proyecto": trip.project_title, "destino": trip.destination, "estimado": trip.est_total, "gastado": trip.spent,
            "informe_sha256": crate::ledger::sha256_hex(trip.report_text.as_bytes()), "comprobantes": hashes,
        }),
    )
    .await?;
    stamp(&state, step_id, seq).await?;
    state.log_activity("Finanzas", &format!("Viaje V-{} cerrado: gastado {}", trip.folio, trip.spent), &session.username).await;
    publish(&state);
    Ok(Json(trip))
}
