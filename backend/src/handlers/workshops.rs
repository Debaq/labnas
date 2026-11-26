//! Talleres con inscripcion, cupos, asistencia y certificados (idea 40).
//! Se montan sobre las fichas del Portafolio de tipo taller o curso. La inscripcion publica
//! (enlace secreto) y la verificacion de certificados no necesitan cuenta, pero el modulo
//! Portafolio tiene que estar encendido (ver `modules.rs`).

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{internal, ApiError};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

const MAX_SESSIONS: usize = 60;
const MAX_REGISTRATIONS: i64 = 1000;
const MAX_PER_PERSON: i64 = 3;
const AUDIT: &str = "Portafolio";

struct Settings {
    capacity: i64,
    open: bool,
    token: Option<String>,
    sessions: Vec<String>,
    min_pct: i64,
}

struct Workshop {
    title: String,
    description: String,
    hours: Option<i64>,
    start_date: String,
    end_date: Option<String>,
    modality: String,
}

fn not_found() -> ApiError {
    (StatusCode::NOT_FOUND, "No encontrado".into())
}

fn bad(msg: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.into())
}

fn workshop(conn: &Connection, id: &str) -> Result<Workshop, ApiError> {
    conn.query_row(
        "SELECT title, description, hours, start_date, end_date, modality FROM portfolio_entries
         WHERE id = ?1 AND entry_type IN ('workshop', 'course')",
        params![id],
        |r| {
            Ok(Workshop {
                title: r.get(0)?,
                description: r.get(1)?,
                hours: r.get(2)?,
                start_date: r.get(3)?,
                end_date: r.get(4)?,
                modality: r.get(5)?,
            })
        },
    )
    .optional()
    .map_err(internal)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Taller no encontrado".to_string()))
}

fn settings(conn: &Connection, id: &str) -> Result<Settings, ApiError> {
    let row = conn
        .query_row(
            "SELECT capacity, registration_open, public_token, sessions, min_attendance FROM workshop_settings WHERE entry_id = ?1",
            params![id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?)),
        )
        .optional()
        .map_err(internal)?;
    Ok(match row {
        Some((capacity, open, token, sessions, min_pct)) => Settings {
            capacity,
            open: open != 0,
            token,
            sessions: serde_json::from_str(&sessions).unwrap_or_default(),
            min_pct,
        },
        None => Settings { capacity: 0, open: false, token: None, sessions: vec![], min_pct: 75 },
    })
}

fn reg_json(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let attended: String = r.get(7)?;
    let attended: Vec<String> = serde_json::from_str(&attended).unwrap_or_default();
    Ok(json!({
        "id": r.get::<_, i64>(0)?,
        "name": r.get::<_, String>(1)?,
        "email": r.get::<_, String>(2)?,
        "username": r.get::<_, Option<String>>(3)?,
        "affiliation": r.get::<_, String>(4)?,
        "registered_at": r.get::<_, String>(5)?,
        "status": r.get::<_, String>(6)?,
        "attended": attended,
        "certificate_code": r.get::<_, Option<String>>(8)?,
        "certificate_at": r.get::<_, Option<String>>(9)?,
    }))
}

const REG_COLS: &str = "id, name, email, username, affiliation, registered_at, status, attended, certificate_code, certificate_at";

fn registrations(conn: &Connection, entry: &str) -> Result<Vec<Value>, ApiError> {
    let mut stmt = conn
        .prepare(&format!("SELECT {REG_COLS} FROM workshop_registrations WHERE entry_id = ?1 ORDER BY id"))
        .map_err(internal)?;
    let rows = stmt.query_map(params![entry], reg_json).map_err(internal)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(internal)
}

fn count(conn: &Connection, entry: &str, status: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM workshop_registrations WHERE entry_id = ?1 AND status = ?2",
        params![entry, status],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// Sube de la lista de espera mientras haya cupo
fn rebalance(conn: &Connection, entry: &str, capacity: i64) -> Result<(), ApiError> {
    if capacity <= 0 {
        conn.execute(
            "UPDATE workshop_registrations SET status = 'inscrito' WHERE entry_id = ?1 AND status = 'espera'",
            params![entry],
        )
        .map_err(internal)?;
        return Ok(());
    }
    loop {
        if count(conn, entry, "inscrito") >= capacity {
            return Ok(());
        }
        let next: Option<i64> = conn
            .query_row(
                "SELECT id FROM workshop_registrations WHERE entry_id = ?1 AND status = 'espera' ORDER BY id LIMIT 1",
                params![entry],
                |r| r.get(0),
            )
            .optional()
            .map_err(internal)?;
        let Some(id) = next else { return Ok(()) };
        conn.execute("UPDATE workshop_registrations SET status = 'inscrito' WHERE id = ?1", params![id])
            .map_err(internal)?;
    }
}

/// Ejecuta `f` en una transaccion inmediata (el cupo no se pasa con inscripciones simultaneas)
fn tx<T>(conn: &Connection, f: impl FnOnce() -> Result<T, ApiError>) -> Result<T, ApiError> {
    conn.execute_batch("BEGIN IMMEDIATE").map_err(internal)?;
    match f() {
        Ok(v) => {
            conn.execute_batch("COMMIT").map_err(internal)?;
            Ok(v)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

pub(crate) fn valid_email(e: &str) -> bool {
    if e.len() > 120 || e.chars().any(|c| c.is_whitespace() || c.is_control() || c == ',' || c == ';' || c == '"') {
        return false;
    }
    let mut parts = e.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else { return false };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
}

fn clean(s: &str, max: usize) -> String {
    s.trim().chars().filter(|c| !c.is_control()).take(max).collect()
}

fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn new_code() -> String {
    let h = uuid::Uuid::new_v4().simple().to_string().to_uppercase();
    format!("{}-{}-{}-{}", &h[0..4], &h[4..8], &h[8..12], &h[12..16])
}

struct Enroll {
    name: String,
    email: String,
    username: Option<String>,
    affiliation: String,
}

fn enroll(conn: &Connection, entry: &str, e: Enroll) -> Result<Value, ApiError> {
    tx(conn, || {
        let s = settings(conn, entry)?;
        if !s.open {
            return Err((StatusCode::CONFLICT, "La inscripcion esta cerrada".into()));
        }
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM workshop_registrations WHERE entry_id = ?1 AND status != 'cancelado'",
                params![entry],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if total >= MAX_REGISTRATIONS {
            return Err((StatusCode::CONFLICT, "El taller no admite mas inscripciones".into()));
        }
        // Misma persona (correo o cuenta): una inscripcion vigente y pocos reintentos
        let (active, all): (i64, i64) = conn
            .query_row(
                "SELECT COALESCE(SUM(status != 'cancelado'), 0), COUNT(*) FROM workshop_registrations
                 WHERE entry_id = ?1 AND ((?2 != '' AND email = ?2) OR (?3 IS NOT NULL AND username = ?3))",
                params![entry, e.email, e.username],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(internal)?;
        if active > 0 {
            return Err((StatusCode::CONFLICT, "Ya estas inscrito en este taller".into()));
        }
        if all >= MAX_PER_PERSON {
            return Err((StatusCode::TOO_MANY_REQUESTS, "Demasiados intentos de inscripcion para esta persona".into()));
        }
        let status = if s.capacity <= 0 || count(conn, entry, "inscrito") < s.capacity { "inscrito" } else { "espera" };
        conn.execute(
            "INSERT INTO workshop_registrations (entry_id, name, email, username, affiliation, registered_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![entry, e.name, e.email, e.username, e.affiliation, chrono::Utc::now().to_rfc3339(), status],
        )
        .map_err(internal)?;
        let position = if status == "espera" { count(conn, entry, "espera") } else { 0 };
        Ok(json!({ "status": status, "waitlist_position": position }))
    })
}

fn eligible(s: &Settings, attended: &[String]) -> bool {
    let total = s.sessions.len() as i64;
    let done = attended.iter().filter(|a| s.sessions.contains(a)).count() as i64;
    total > 0 && done * 100 >= s.min_pct * total
}

fn branding(conn: &Connection) -> Value {
    conn.query_row("SELECT lab_name, institution, logo_url FROM branding WHERE id = 1", [], |r| {
        Ok(json!({ "lab_name": r.get::<_, String>(0)?, "institution": r.get::<_, String>(1)?, "logo_url": r.get::<_, String>(2)? }))
    })
    .unwrap_or_else(|_| json!({ "lab_name": "LabNAS", "institution": "", "logo_url": "" }))
}

fn certificate_json(conn: &Connection, entry: &str, reg_id: i64) -> Result<Value, ApiError> {
    let w = workshop(conn, entry)?;
    let s = settings(conn, entry)?;
    let (name, attended, code, at, status): (String, String, Option<String>, Option<String>, String) = conn
        .query_row(
            "SELECT name, attended, certificate_code, certificate_at, status FROM workshop_registrations WHERE id = ?1 AND entry_id = ?2",
            params![reg_id, entry],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let attended: Vec<String> = serde_json::from_str(&attended).unwrap_or_default();
    let done: Vec<&String> = s.sessions.iter().filter(|x| attended.contains(x)).collect();
    Ok(json!({
        "code": code, "issued_at": at, "valid": status != "cancelado" && code.is_some(),
        "name": name, "workshop": w.title, "hours": w.hours, "modality": w.modality,
        "start_date": w.start_date, "end_date": w.end_date,
        "sessions": s.sessions, "attended": done, "min_attendance": s.min_pct,
        "lab": branding(conn),
    }))
}

/// Emite (o devuelve) el certificado si cumple la asistencia minima
fn issue(conn: &Connection, entry: &str, reg_id: i64) -> Result<(Value, bool), ApiError> {
    let s = settings(conn, entry)?;
    let (attended, status, code): (String, String, Option<String>) = conn
        .query_row(
            "SELECT attended, status, certificate_code FROM workshop_registrations WHERE id = ?1 AND entry_id = ?2",
            params![reg_id, entry],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let attended: Vec<String> = serde_json::from_str(&attended).unwrap_or_default();
    let mut fresh = false;
    if code.is_none() {
        if status != "inscrito" {
            return Err((StatusCode::CONFLICT, "Solo las personas inscritas reciben certificado".into()));
        }
        if !eligible(&s, &attended) {
            return Err((StatusCode::CONFLICT, format!("No alcanza la asistencia minima ({}%)", s.min_pct)));
        }
        conn.execute(
            "UPDATE workshop_registrations SET certificate_code = ?1, certificate_at = ?2 WHERE id = ?3",
            params![new_code(), chrono::Utc::now().to_rfc3339(), reg_id],
        )
        .map_err(internal)?;
        fresh = true;
    }
    Ok((certificate_json(conn, entry, reg_id)?, fresh))
}

fn require(session: &SessionInfo, level: Level) -> Result<(), ApiError> {
    if session.can("portfolio", level) {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Sin permiso".into()))
    }
}

// ── Con sesion ──

/// GET /api/portfolio/{id}/workshop
pub async fn get_workshop(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let manage = session.can("portfolio", Level::Gestionar);
    let user = session.username.clone();
    let v = crate::db::db_op_status(&state.db, move |conn| {
        workshop(conn, &id)?;
        let s = settings(conn, &id)?;
        let regs = registrations(conn, &id)?;
        let mine = regs
            .iter()
            .find(|r| r["username"] == user.as_str() && r["status"] != "cancelado")
            .cloned();
        let mut out = json!({
            "capacity": s.capacity, "registration_open": s.open, "sessions": s.sessions,
            "min_attendance": s.min_pct, "can_manage": manage, "mine": mine,
            "enrolled": count(conn, &id, "inscrito"), "waitlist": count(conn, &id, "espera"),
        });
        if manage {
            out["public_token"] = json!(s.token);
            out["registrations"] = json!(regs);
        }
        Ok(out)
    })
    .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct SettingsReq {
    #[serde(default)]
    capacity: i64,
    #[serde(default)]
    registration_open: bool,
    #[serde(default)]
    sessions: Vec<String>,
    min_attendance: Option<i64>,
    #[serde(default)]
    rotate_token: bool,
}

/// PUT /api/portfolio/{id}/workshop — Gestionar
pub async fn put_workshop(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<SettingsReq>,
) -> Result<Json<Value>, ApiError> {
    require(&session, Level::Gestionar)?;
    if !(0..=10000).contains(&req.capacity) {
        return Err(bad("Cupo invalido"));
    }
    let pct = req.min_attendance.unwrap_or(75);
    if !(1..=100).contains(&pct) {
        return Err(bad("La asistencia minima va de 1 a 100"));
    }
    let mut sessions: Vec<String> = req.sessions.iter().map(|s| clean(s, 40)).filter(|s| !s.is_empty()).collect();
    sessions.dedup();
    if sessions.len() > MAX_SESSIONS {
        return Err(bad("Demasiadas sesiones"));
    }
    let (open, capacity, rotate) = (req.registration_open, req.capacity, req.rotate_token);
    let n_sessions = sessions.len();
    let sess_json = serde_json::to_string(&sessions).map_err(internal)?;
    let id2 = id.clone();
    let (title, was_open) = crate::db::db_op_status(&state.db, move |conn| {
        let w = workshop(conn, &id2)?;
        tx(conn, || {
            let old = settings(conn, &id2)?;
            let token = match (&old.token, rotate) {
                (Some(t), false) => t.clone(),
                _ => new_token(),
            };
            conn.execute(
                "INSERT INTO workshop_settings (entry_id, capacity, registration_open, public_token, sessions, min_attendance)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(entry_id) DO UPDATE SET capacity = ?2, registration_open = ?3, public_token = ?4, sessions = ?5, min_attendance = ?6",
                params![id2, capacity, open as i64, token, sess_json, pct],
            )
            .map_err(internal)?;
            // Sesiones quitadas: se olvidan en la asistencia
            let kept: std::collections::HashSet<&str> = sessions.iter().map(String::as_str).collect();
            let mut stmt = conn
                .prepare("SELECT id, attended FROM workshop_registrations WHERE entry_id = ?1")
                .map_err(internal)?;
            let rows: Vec<(i64, String)> = stmt
                .query_map(params![id2], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(internal)?
                .filter_map(Result::ok)
                .collect();
            drop(stmt);
            for (rid, att) in rows {
                let list: Vec<String> = serde_json::from_str(&att).unwrap_or_default();
                let filtered: Vec<&String> = list.iter().filter(|a| kept.contains(a.as_str())).collect();
                if filtered.len() != list.len() {
                    conn.execute(
                        "UPDATE workshop_registrations SET attended = ?1 WHERE id = ?2",
                        params![serde_json::to_string(&filtered).unwrap_or_default(), rid],
                    )
                    .map_err(internal)?;
                }
            }
            rebalance(conn, &id2, capacity)?;
            Ok((w.title, old.open))
        })
    })
    .await?;
    if open != was_open {
        let what = if open { "Inscripcion abierta" } else { "Inscripcion cerrada" };
        state.log_activity(AUDIT, &format!("{what}: taller «{title}»"), &session.username).await;
    } else {
        state.log_activity(AUDIT, &format!("Taller «{title}»: cupo {capacity}, {n_sessions} sesiones"), &session.username).await;
    }
    get_workshop(State(state), Extension(session), Path(id)).await
}

/// POST /api/portfolio/{id}/workshop/enroll — la persona con sesion se inscribe
pub async fn enroll_self(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let body = body.map(|b| b.0).unwrap_or(Value::Null);
    let field = |k: &str, max: usize| clean(body[k].as_str().unwrap_or(""), max);
    let email = field("email", 120).to_lowercase();
    if !email.is_empty() && !valid_email(&email) {
        return Err(bad("Correo invalido"));
    }
    let name = match field("name", 100) {
        n if n.is_empty() => session.username.clone(),
        n => n,
    };
    let e = Enroll { name, email, username: Some(session.username.clone()), affiliation: field("affiliation", 120) };
    let v = crate::db::db_op_status(&state.db, move |conn| {
        workshop(conn, &id)?;
        enroll(conn, &id, e)
    })
    .await?;
    Ok(Json(v))
}

/// DELETE /api/portfolio/{id}/workshop/enroll — cancela la inscripcion propia
pub async fn cancel_self(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user = session.username.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        tx(conn, || {
            let n = conn
                .execute(
                    "UPDATE workshop_registrations SET status = 'cancelado' WHERE entry_id = ?1 AND username = ?2 AND status != 'cancelado'",
                    params![id, user],
                )
                .map_err(internal)?;
            if n == 0 {
                return Err(not_found());
            }
            rebalance(conn, &id, settings(conn, &id)?.capacity)
        })
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/portfolio/{id}/workshop/my-certificate — certificado propio si cumple la asistencia
pub async fn my_certificate(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = session.username.clone();
    let (cert, fresh, title) = crate::db::db_op_status(&state.db, move |conn| {
        let reg: i64 = conn
            .query_row(
                "SELECT id FROM workshop_registrations WHERE entry_id = ?1 AND username = ?2 AND status != 'cancelado'",
                params![id, user],
                |r| r.get(0),
            )
            .optional()
            .map_err(internal)?
            .ok_or_else(not_found)?;
        let (c, fresh) = issue(conn, &id, reg)?;
        let title = c["workshop"].as_str().unwrap_or("").to_string();
        Ok((c, fresh, title))
    })
    .await?;
    if fresh {
        state.log_activity(AUDIT, &format!("Certificado emitido: «{title}» a {}", session.username), &session.username).await;
    }
    Ok(Json(cert))
}

#[derive(Deserialize)]
pub struct AttendanceReq {
    attended: Vec<String>,
}

/// PUT /api/portfolio/{id}/workshop/registrations/{rid}/attendance — Gestionar
pub async fn set_attendance(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path((id, rid)): Path<(String, i64)>,
    Json(req): Json<AttendanceReq>,
) -> Result<Json<Value>, ApiError> {
    require(&session, Level::Gestionar)?;
    let (title, name, n) = crate::db::db_op_status(&state.db, move |conn| {
        let w = workshop(conn, &id)?;
        let s = settings(conn, &id)?;
        let att: Vec<&String> = s.sessions.iter().filter(|x| req.attended.contains(x)).collect();
        let name: String = conn
            .query_row("SELECT name FROM workshop_registrations WHERE id = ?1 AND entry_id = ?2", params![rid, id], |r| r.get(0))
            .optional()
            .map_err(internal)?
            .ok_or_else(not_found)?;
        conn.execute(
            "UPDATE workshop_registrations SET attended = ?1 WHERE id = ?2",
            params![serde_json::to_string(&att).map_err(internal)?, rid],
        )
        .map_err(internal)?;
        Ok((w.title, name, att.len()))
    })
    .await?;
    state.log_activity(AUDIT, &format!("Asistencia: «{title}», {name}: {n} sesiones"), &session.username).await;
    Ok(Json(json!({ "ok": true, "attended": n })))
}

/// POST /api/portfolio/{id}/workshop/registrations/{rid}/cancel — Gestionar
pub async fn cancel_registration(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path((id, rid)): Path<(String, i64)>,
) -> Result<StatusCode, ApiError> {
    require(&session, Level::Gestionar)?;
    let (title, name) = crate::db::db_op_status(&state.db, move |conn| {
        let w = workshop(conn, &id)?;
        tx(conn, || {
            let name: String = conn
                .query_row("SELECT name FROM workshop_registrations WHERE id = ?1 AND entry_id = ?2", params![rid, id], |r| r.get(0))
                .optional()
                .map_err(internal)?
                .ok_or_else(not_found)?;
            conn.execute("UPDATE workshop_registrations SET status = 'cancelado' WHERE id = ?1", params![rid]).map_err(internal)?;
            rebalance(conn, &id, settings(conn, &id)?.capacity)?;
            Ok((w.title, name))
        })
    })
    .await?;
    state.log_activity(AUDIT, &format!("Inscripcion cancelada: «{title}», {name}"), &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/portfolio/{id}/workshop/registrations/{rid}/certificate — Gestionar
pub async fn issue_certificate(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path((id, rid)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    require(&session, Level::Gestionar)?;
    let (cert, fresh) = crate::db::db_op_status(&state.db, move |conn| issue(conn, &id, rid)).await?;
    if fresh {
        let msg = format!("Certificado emitido: «{}» a {}", cert["workshop"].as_str().unwrap_or(""), cert["name"].as_str().unwrap_or(""));
        state.log_activity(AUDIT, &msg, &session.username).await;
    }
    Ok(Json(cert))
}

fn csv_cell(s: &str) -> String {
    let mut v = s.replace('"', "\"\"");
    // Evita que una hoja de calculo ejecute como formula lo que escribio un visitante
    if v.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        v.insert(0, '\'');
    }
    format!("\"{v}\"")
}

/// GET /api/portfolio/{id}/workshop/export.csv — Gestionar
pub async fn export_csv(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require(&session, Level::Gestionar)?;
    let (title, s, regs) = crate::db::db_op_status(&state.db, move |conn| {
        let w = workshop(conn, &id)?;
        Ok((w.title, settings(conn, &id)?, registrations(conn, &id)?))
    })
    .await?;
    let mut out = String::from("\u{feff}Nombre,Correo,Usuario,Afiliacion,Inscrito el,Estado,Sesiones asistidas,Sesiones totales,Certificado\n");
    for r in &regs {
        let att = r["attended"].as_array().map(|a| a.len()).unwrap_or(0);
        let cells = [
            csv_cell(r["name"].as_str().unwrap_or("")),
            csv_cell(r["email"].as_str().unwrap_or("")),
            csv_cell(r["username"].as_str().unwrap_or("")),
            csv_cell(r["affiliation"].as_str().unwrap_or("")),
            csv_cell(r["registered_at"].as_str().unwrap_or("")),
            csv_cell(r["status"].as_str().unwrap_or("")),
            att.to_string(),
            s.sessions.len().to_string(),
            csv_cell(r["certificate_code"].as_str().unwrap_or("")),
        ];
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    state.log_activity(AUDIT, &format!("Exportada la lista de inscritos: «{title}»"), &session.username).await;
    let safe: String = title.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"inscritos_{safe}.csv\"")),
        ],
        out,
    ))
}

/// Al borrar la ficha se borran sus inscripciones
pub fn forget(conn: &Connection, entry: &str) {
    let _ = conn.execute("DELETE FROM workshop_registrations WHERE entry_id = ?1", params![entry]);
    let _ = conn.execute("DELETE FROM workshop_settings WHERE entry_id = ?1", params![entry]);
}

// ── Publicas (sin cuenta) ──

fn by_token(conn: &Connection, token: &str) -> Result<(String, Workshop, Settings), ApiError> {
    if token.len() != 32 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(not_found());
    }
    let id: String = conn
        .query_row(
            "SELECT entry_id FROM workshop_settings WHERE public_token = ?1 AND registration_open = 1",
            params![token],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let w = workshop(conn, &id).map_err(|_| not_found())?;
    let s = settings(conn, &id)?;
    Ok((id, w, s))
}

/// GET /api/portfolio/taller/{token}
pub async fn public_info(State(state): State<AppState>, Path(token): Path<String>) -> Result<Json<Value>, ApiError> {
    let v = crate::db::db_op_status(&state.db, move |conn| {
        let (id, w, s) = by_token(conn, &token)?;
        let enrolled = count(conn, &id, "inscrito");
        Ok(json!({
            "title": w.title, "description": w.description, "hours": w.hours, "modality": w.modality,
            "start_date": w.start_date, "end_date": w.end_date, "sessions": s.sessions,
            "capacity": s.capacity, "enrolled": enrolled, "full": s.capacity > 0 && enrolled >= s.capacity,
            "lab": branding(conn),
        }))
    })
    .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct PublicEnrollReq {
    #[serde(default)]
    name: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    affiliation: String,
}

/// POST /api/portfolio/taller/{token}
pub async fn public_enroll(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(req): Json<PublicEnrollReq>,
) -> Result<Json<Value>, ApiError> {
    let name = clean(&req.name, 100);
    let email = clean(&req.email, 120).to_lowercase();
    let affiliation = clean(&req.affiliation, 120);
    if name.is_empty() {
        return Err(bad("Falta el nombre"));
    }
    if !valid_email(&email) {
        return Err(bad("Correo invalido"));
    }
    let v = crate::db::db_op_status(&state.db, move |conn| {
        let (id, _, _) = by_token(conn, &token)?;
        enroll(conn, &id, Enroll { name, email, username: None, affiliation })
    })
    .await?;
    Ok(Json(v))
}

/// GET /api/portfolio/certificado/{codigo} — verificacion publica, solo lectura
pub async fn public_certificate(State(state): State<AppState>, Path(code): Path<String>) -> Result<Json<Value>, ApiError> {
    let code = code.trim().to_uppercase();
    if code.len() != 19 || !code.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(not_found());
    }
    let v = crate::db::db_op_status(&state.db, move |conn| {
        let (entry, rid): (String, i64) = conn
            .query_row(
                "SELECT entry_id, id FROM workshop_registrations WHERE certificate_code = ?1",
                params![code],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(internal)?
            .ok_or_else(not_found)?;
        certificate_json(conn, &entry, rid)
    })
    .await?;
    Ok(Json(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correos() {
        assert!(valid_email("ana@uach.cl"));
        assert!(!valid_email("ana@uach"));
        assert!(!valid_email("a b@uach.cl"));
        assert!(!valid_email("@uach.cl"));
        assert!(!valid_email("a@@uach.cl"));
    }

    #[test]
    fn celdas_csv() {
        assert_eq!(csv_cell("=1+1"), "\"'=1+1\"");
        assert_eq!(csv_cell("a\"b"), "\"a\"\"b\"");
    }
}
