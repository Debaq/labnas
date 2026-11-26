//! Registro de uso real de maquinas (idea 2 de la hoja de ruta). Ver docs/MAQUINAS.md.
//!
//! Desde la ficha de la maquina (la del QR) una persona toca «Empezar a usar» y despues
//! «Terminar». Queda una sesion con quien, cuando y en que proyecto; de ahi salen las horas
//! reales que cuentan para la mantencion y el informe de horas por maquina, persona y proyecto.
//!
//! - Una sola sesion abierta por maquina.
//! - No se empieza si la maquina esta fuera de servicio, si exige induccion y no se tiene, o
//!   si hay una mantencion vencida que bloquea (el staff si puede).
//! - Si la persona tiene una reserva en curso, la sesion se liga a ella.
//! - Terminar: quien la empezo o el staff.
//! - Las sesiones abiertas mas de 12 h se cierran solas (`auto_close_loop`) y se avisa por Telegram.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Duration, Local, NaiveDateTime};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::ApiError;
use crate::handlers::machines::{bad, conflict, db_err, forbidden, is_staff, load_machine, md, not_found, publish};
use crate::state::{AppState, SessionInfo};

const TIME_FMT: &str = "%Y-%m-%dT%H:%M";
/// Una sesion abierta mas de estas horas se cierra sola
const MAX_OPEN_HOURS: i64 = 12;

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

fn now_str() -> String {
    Local::now().naive_local().format(TIME_FMT).to_string()
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageSession {
    pub id: String,
    pub machine_id: String,
    pub machine_name: String,
    pub username: String,
    pub booking_id: Option<String>,
    pub project: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub note: String,
    /// Horas de la sesion (solo terminadas)
    pub hours: Option<f64>,
}

const SELECT: &str = "SELECT u.id, u.machine_id, COALESCE(m.name, ''), u.username, u.booking_id, u.project, u.started_at, u.ended_at, u.note,
    CASE WHEN u.ended_at IS NULL THEN NULL ELSE (julianday(u.ended_at) - julianday(u.started_at)) * 24.0 END
    FROM machine_usage u LEFT JOIN machines m ON m.id = u.machine_id";

fn row_to_session(r: &rusqlite::Row) -> rusqlite::Result<UsageSession> {
    Ok(UsageSession {
        id: r.get(0)?,
        machine_id: r.get(1)?,
        machine_name: r.get(2)?,
        username: r.get(3)?,
        booking_id: r.get(4)?,
        project: r.get(5)?,
        started_at: r.get(6)?,
        ended_at: r.get(7)?,
        note: r.get(8)?,
        hours: r.get::<_, Option<f64>>(9)?.map(|h| round1(h.max(0.0))),
    })
}

/// Sesion abierta de la maquina, si hay (la usa `machines::load_machine`)
pub fn open_session(conn: &Connection, machine_id: &str) -> rusqlite::Result<Option<UsageSession>> {
    conn.query_row(&format!("{} WHERE u.machine_id = ?1 AND u.ended_at IS NULL", SELECT), [machine_id], row_to_session).optional()
}

fn load_session(conn: &Connection, id: &str) -> rusqlite::Result<Option<UsageSession>> {
    conn.query_row(&format!("{} WHERE u.id = ?1", SELECT), [id], row_to_session).optional()
}

fn clean(s: Option<String>, max: usize, what: &str) -> Result<String, ApiError> {
    let s = s.unwrap_or_default().trim().to_string();
    if s.chars().count() > max {
        return Err(bad(format!("{} demasiado largo (maximo {} caracteres)", what, max)));
    }
    Ok(s)
}

#[derive(Debug, Serialize)]
pub struct UsageList {
    pub open: Option<UsageSession>,
    /// Sesiones terminadas, la mas nueva primero
    pub recent: Vec<UsageSession>,
}

/// GET /api/machines/{id}/usage — sesion abierta y las ultimas terminadas
pub async fn list_usage(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<UsageList>, ApiError> {
    db_op_status(&state.db, move |conn| {
        load_machine(conn, &id).map_err(db_err)?.ok_or_else(not_found)?;
        let open = open_session(conn, &id).map_err(db_err)?;
        let mut stmt = conn
            .prepare(&format!("{} WHERE u.machine_id = ?1 AND u.ended_at IS NOT NULL ORDER BY u.ended_at DESC LIMIT 30", SELECT))
            .map_err(db_err)?;
        let recent = stmt.query_map([&id], row_to_session).map_err(db_err)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)?;
        Ok(UsageList { open, recent })
    })
    .await
    .map(Json)
}

#[derive(Debug, Deserialize, Default)]
pub struct StartReq {
    #[serde(default)]
    pub project: Option<String>,
}

/// POST /api/machines/{id}/usage/start
pub async fn start_usage(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(machine_id): Path<String>,
    body: Option<Json<StartReq>>,
) -> Result<(StatusCode, Json<UsageSession>), ApiError> {
    let staff = is_staff(&session);
    let project = clean(body.and_then(|Json(b)| b.project), 120, "Proyecto")?;
    let user = session.username.clone();
    let s = db_op_status(&state.db, move |conn| {
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate).map_err(db_err)?;
        let m = load_machine(&tx, &machine_id).map_err(db_err)?.ok_or_else(not_found)?;
        match m.status.as_str() {
            "fuera_servicio" => return Err(conflict("La maquina esta fuera de servicio hasta que la reparen")),
            "retirada" => return Err(conflict("La maquina fue retirada")),
            _ => {}
        }
        if let Some(open) = open_session(&tx, &machine_id).map_err(db_err)? {
            return Err(conflict(format!("{} ya la esta usando desde las {}", open.username, open.started_at.get(11..).unwrap_or(""))));
        }
        if !staff {
            if let Some(t) = crate::handlers::machine_maintenance::blocking_task(&tx, &machine_id).map_err(db_err)? {
                return Err(conflict(format!("La mantencion «{}» de {} esta vencida: no se puede usar hasta que la registren", t, m.name)));
            }
            if m.requires_certification && !crate::handlers::machine_certs::is_certified(&tx, &machine_id, &user).map_err(db_err)? {
                return Err(forbidden(&format!("Para usar {} necesitas la induccion. Pidela desde la ficha de la maquina.", m.name)));
            }
        }
        let now = now_str();
        // Reserva en curso de esta persona que aun no tenga sesion
        let booking: Option<String> = tx
            .query_row(
                "SELECT b.id FROM machine_bookings b
                 WHERE b.machine_id = ?1 AND b.username = ?2 AND b.kind = 'reserva' AND b.status = 'confirmada'
                   AND b.start <= ?3 AND b.end > ?3
                   AND NOT EXISTS (SELECT 1 FROM machine_usage u WHERE u.booking_id = b.id)
                 ORDER BY b.start LIMIT 1",
                params![machine_id, user, now],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        let id = new_id();
        tx.execute(
            "INSERT INTO machine_usage (id, machine_id, username, booking_id, project, started_at, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '')",
            params![id, machine_id, user, booking, project, now],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        load_session(conn, &id).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    state.log_activity("Maquinas", &format!("Empezo a usar {}", s.machine_name), &session.username).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(s)))
}

#[derive(Debug, Deserialize, Default)]
pub struct StopReq {
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
}

/// POST /api/machines/usage/{sid}/stop — quien la empezo o el staff
pub async fn stop_usage(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(sid): Path<String>,
    body: Option<Json<StopReq>>,
) -> Result<Json<UsageSession>, ApiError> {
    let staff = is_staff(&session);
    let req = body.map(|Json(b)| b).unwrap_or_default();
    let note = clean(req.note, 500, "Nota")?;
    let project = req.project.map(|p| clean(Some(p), 120, "Proyecto")).transpose()?;
    let user = session.username.clone();
    let s = db_op_status(&state.db, move |conn| {
        let cur = load_session(conn, &sid).map_err(db_err)?.ok_or((StatusCode::NOT_FOUND, "Sesion no encontrada".to_string()))?;
        if cur.ended_at.is_some() {
            return Err(conflict("La sesion ya termino"));
        }
        if cur.username != user && !staff {
            return Err(forbidden("Solo quien empezo la sesion o el staff puede terminarla"));
        }
        let project = project.unwrap_or(cur.project);
        // Nunca antes del inicio
        let end = now_str().max(cur.started_at);
        conn.execute(
            "UPDATE machine_usage SET ended_at = ?2, note = ?3, project = ?4 WHERE id = ?1",
            params![sid, end, note, project],
        )
        .map_err(db_err)?;
        load_session(conn, &sid).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    state
        .log_activity("Maquinas", &format!("Termino de usar {} ({} h, {})", s.machine_name, s.hours.unwrap_or(0.0), s.username), &session.username)
        .await;
    publish(&state);
    Ok(Json(s))
}

#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReportRow {
    pub key: String,
    pub label: String,
    pub hours: f64,
    pub sessions: i64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub from: String,
    pub to: String,
    pub total_hours: f64,
    pub sessions: i64,
    pub by_machine: Vec<ReportRow>,
    pub by_user: Vec<ReportRow>,
    pub by_project: Vec<ReportRow>,
}

fn valid_day(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

fn group(conn: &Connection, expr: &str, label: &str, from: &str, to: &str) -> rusqlite::Result<Vec<ReportRow>> {
    let sql = format!(
        "SELECT {expr} AS k, {label}, COALESCE(SUM((julianday(u.ended_at) - julianday(u.started_at)) * 24.0), 0), COUNT(*)
         FROM machine_usage u LEFT JOIN machines m ON m.id = u.machine_id
         WHERE u.ended_at IS NOT NULL AND u.started_at >= ?1 AND u.started_at < ?2
         GROUP BY k ORDER BY 3 DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok(ReportRow { key: r.get(0)?, label: r.get(1)?, hours: round1(r.get::<_, f64>(2)?.max(0.0)), sessions: r.get(3)? })
    })?;
    rows.collect()
}

/// GET /api/machines/usage/report?from=YYYY-MM-DD&to=YYYY-MM-DD — solo staff (por defecto, los ultimos 30 dias)
pub async fn report(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<ReportQuery>,
) -> Result<Json<Report>, ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo administradores y operadores ven el informe de horas"));
    }
    let today = Local::now().date_naive();
    let to = q.to.unwrap_or_else(|| today.format("%Y-%m-%d").to_string());
    let from = q.from.unwrap_or_else(|| (today - Duration::days(30)).format("%Y-%m-%d").to_string());
    if !valid_day(&from) || !valid_day(&to) {
        return Err(bad("Fechas invalidas (AAAA-MM-DD)"));
    }
    // `to` inclusivo: hasta el fin de ese dia
    let (lo, hi) = (format!("{}T00:00", from), format!("{}T23:59:59", to));
    db_op_status(&state.db, move |conn| {
        let by_machine = group(conn, "u.machine_id", "COALESCE(m.name, u.machine_id)", &lo, &hi).map_err(db_err)?;
        let by_user = group(conn, "u.username", "u.username", &lo, &hi).map_err(db_err)?;
        let by_project = group(conn, "u.project", "CASE WHEN u.project = '' THEN '(sin proyecto)' ELSE u.project END", &lo, &hi).map_err(db_err)?;
        Ok(Report {
            from,
            to,
            total_hours: round1(by_machine.iter().map(|r| r.hours).sum()),
            sessions: by_machine.iter().map(|r| r.sessions).sum(),
            by_machine,
            by_user,
            by_project,
        })
    })
    .await
    .map(Json)
}

/// Cierra las sesiones abiertas hace mas de `MAX_OPEN_HOURS`; devuelve las cerradas
fn close_stale(conn: &Connection, now: NaiveDateTime) -> Result<Vec<UsageSession>, String> {
    let cutoff = (now - Duration::hours(MAX_OPEN_HOURS)).format(TIME_FMT).to_string();
    let mut stmt = conn.prepare(&format!("{} WHERE u.ended_at IS NULL AND u.started_at <= ?1", SELECT)).map_err(|e| e.to_string())?;
    let stale: Vec<UsageSession> =
        stmt.query_map([&cutoff], row_to_session).map_err(|e| e.to_string())?.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for mut s in stale {
        // Se cuentan solo las 12 h: no se sabe cuando termino de verdad
        let end = NaiveDateTime::parse_from_str(&s.started_at, TIME_FMT)
            .map(|t| (t + Duration::hours(MAX_OPEN_HOURS)).format(TIME_FMT).to_string())
            .unwrap_or_else(|_| cutoff.clone());
        conn.execute(
            "UPDATE machine_usage SET ended_at = ?2, note = 'cerrada automaticamente' WHERE id = ?1 AND ended_at IS NULL",
            params![s.id, end],
        )
        .map_err(|e| e.to_string())?;
        s.ended_at = Some(end);
        s.note = "cerrada automaticamente".into();
        out.push(s);
    }
    Ok(out)
}

/// Tarea de fondo del modulo: cierra las sesiones que se quedaron abiertas y avisa a quien las tenia
pub async fn auto_close_loop(state: AppState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(10 * 60));
    loop {
        tick.tick().await;
        let closed = db_op(&state.db, |conn| close_stale(conn, Local::now().naive_local())).await.unwrap_or_default();
        if closed.is_empty() {
            continue;
        }
        for s in &closed {
            let text = format!(
                "⏱ Cerre automaticamente tu sesion en *{}* (llevaba mas de {} h abierta). Si seguias usandola, empiezala de nuevo desde la ficha.",
                md(&s.machine_name),
                MAX_OPEN_HOURS
            );
            crate::handlers::notifications::notify_users(&state, std::slice::from_ref(&s.username), &text).await;
            state.log_activity("Maquinas", &format!("Sesion de {} en {} cerrada automaticamente", s.username, s.machine_name), "sistema").await;
        }
        publish(&state);
    }
}
