//! Maquinas del laboratorio: catalogo con ficha y QR, reservas sin choques de horario
//! y reporte de fallas que deja la maquina fuera de servicio hasta que un operador
//! la repara. Ver docs/MAQUINAS.md.
//!
//! Las horas se guardan como texto local `YYYY-MM-DDTHH:MM` (mismo criterio que el
//! calendario): comparar como texto equivale a comparar fechas.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chrono::{Duration, Local, NaiveDateTime};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::events::{Audience, Level};
use crate::models::notifications::UserRole;
use crate::state::{AppState, SessionInfo};

type ApiError = (StatusCode, String);

pub(crate) const MODULE: Option<&str> = Some("machines");
const TIME_FMT: &str = "%Y-%m-%dT%H:%M";
pub const KINDS: &[&str] = &["laser", "cnc", "impresora3d", "electronica", "otra"];
const STATUSES: &[&str] = &["disponible", "fuera_servicio", "retirada"];
const MAX_PHOTO: usize = 2 * 1024 * 1024;
/// Reservas hasta este numero de dias adelante
const MAX_DAYS_AHEAD: i64 = 90;
/// Aviso por Telegram antes de que empiece una reserva
const REMIND_MIN: i64 = 15;

pub(crate) fn bad(msg: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.into())
}
pub(crate) fn forbidden(msg: &str) -> ApiError {
    (StatusCode::FORBIDDEN, msg.into())
}
pub(crate) fn not_found() -> ApiError {
    (StatusCode::NOT_FOUND, "Maquina no encontrada".into())
}
pub(crate) fn conflict(msg: impl Into<String>) -> ApiError {
    (StatusCode::CONFLICT, msg.into())
}
pub(crate) fn db_err(e: rusqlite::Error) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

pub(crate) fn is_staff(s: &SessionInfo) -> bool {
    matches!(s.role, UserRole::Admin | UserRole::Operador)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

fn now_local() -> NaiveDateTime {
    Local::now().naive_local()
}

fn fmt_time(t: NaiveDateTime) -> String {
    t.format(TIME_FMT).to_string()
}

fn parse_time(s: &str) -> Result<NaiveDateTime, ApiError> {
    NaiveDateTime::parse_from_str(s.trim(), TIME_FMT).map_err(|_| bad("Fecha u hora invalida (formato AAAA-MM-DDTHH:MM)"))
}

/// Texto de usuario dentro de un mensaje Markdown de Telegram
pub(crate) fn md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '_' | '*' | '`' | '[') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `2026-11-23T10:00` -> `23-11 10:00`
fn short_time(s: &str) -> String {
    NaiveDateTime::parse_from_str(s, TIME_FMT).map(|t| t.format("%d-%m %H:%M").to_string()).unwrap_or_else(|_| s.to_string())
}

pub(crate) fn publish(state: &AppState) {
    state.events.publish("machines", (), Audience::All, MODULE);
}

// ─── Modelos ───

#[derive(Debug, Clone, Serialize)]
pub struct Machine {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub location: String,
    pub description: String,
    pub instructions: String,
    pub responsible: Option<String>,
    pub status: String,
    pub requires_approval: bool,
    /// Solo reserva quien tiene la induccion vigente (el staff no la necesita)
    pub requires_certification: bool,
    pub max_hours: Option<f64>,
    pub inventory_item_id: Option<String>,
    pub printer3d_id: Option<String>,
    pub photo_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
    /// Reserva en curso (confirmada)
    pub current: Option<Booking>,
    /// Proxima reserva (confirmada o pendiente)
    pub next: Option<Booking>,
    pub open_faults: Vec<Fault>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Booking {
    pub id: String,
    pub machine_id: String,
    pub machine_name: String,
    pub username: String,
    pub kind: String,
    pub start: String,
    pub end: String,
    pub purpose: String,
    pub status: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Fault {
    pub id: String,
    pub machine_id: String,
    pub reported_by: String,
    pub description: String,
    pub photo_id: Option<String>,
    pub created_at: String,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<String>,
    pub resolution_note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MachineDetail {
    #[serde(flatten)]
    pub machine: Machine,
    /// Todas las fallas (abiertas y resueltas), la mas nueva primero
    pub faults: Vec<Fault>,
}

const MACHINE_COLS: &str = "id, name, kind, location, description, instructions, responsible, status, requires_approval, max_hours, inventory_item_id, printer3d_id, photo_id, created_by, created_at, requires_certification";
const BOOKING_SELECT: &str = "SELECT b.id, b.machine_id, COALESCE(m.name, ''), b.username, b.kind, b.start, b.end, b.purpose, b.status, b.decided_by, b.decided_at, b.created_at
     FROM machine_bookings b LEFT JOIN machines m ON m.id = b.machine_id";
const FAULT_COLS: &str = "id, machine_id, reported_by, description, photo_id, created_at, resolved_by, resolved_at, resolution_note";

fn row_to_booking(r: &rusqlite::Row) -> rusqlite::Result<Booking> {
    Ok(Booking {
        id: r.get(0)?,
        machine_id: r.get(1)?,
        machine_name: r.get(2)?,
        username: r.get(3)?,
        kind: r.get(4)?,
        start: r.get(5)?,
        end: r.get(6)?,
        purpose: r.get(7)?,
        status: r.get(8)?,
        decided_by: r.get(9)?,
        decided_at: r.get(10)?,
        created_at: r.get(11)?,
    })
}

fn row_to_fault(r: &rusqlite::Row) -> rusqlite::Result<Fault> {
    Ok(Fault {
        id: r.get(0)?,
        machine_id: r.get(1)?,
        reported_by: r.get(2)?,
        description: r.get(3)?,
        photo_id: r.get(4)?,
        created_at: r.get(5)?,
        resolved_by: r.get(6)?,
        resolved_at: r.get(7)?,
        resolution_note: r.get(8)?,
    })
}

fn load_faults(conn: &Connection, machine_id: &str, only_open: bool) -> rusqlite::Result<Vec<Fault>> {
    let filter = if only_open { " AND resolved_at IS NULL" } else { "" };
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM machine_faults WHERE machine_id = ?1{} ORDER BY created_at DESC",
        FAULT_COLS, filter
    ))?;
    let rows = stmt.query_map([machine_id], row_to_fault)?;
    rows.collect()
}

pub(crate) fn load_machine(conn: &Connection, id: &str) -> rusqlite::Result<Option<Machine>> {
    let base = conn
        .query_row(&format!("SELECT {} FROM machines WHERE id = ?1", MACHINE_COLS), [id], |r| {
            Ok(Machine {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                location: r.get(3)?,
                description: r.get(4)?,
                instructions: r.get(5)?,
                responsible: r.get(6)?,
                status: r.get(7)?,
                requires_approval: r.get::<_, i64>(8)? != 0,
                max_hours: r.get(9)?,
                inventory_item_id: r.get(10)?,
                printer3d_id: r.get(11)?,
                photo_id: r.get(12)?,
                created_by: r.get(13)?,
                created_at: r.get(14)?,
                requires_certification: r.get::<_, i64>(15)? != 0,
                current: None,
                next: None,
                open_faults: Vec::new(),
            })
        })
        .optional()?;
    let Some(mut m) = base else { return Ok(None) };
    let now = fmt_time(now_local());
    m.current = conn
        .query_row(
            &format!("{} WHERE b.machine_id = ?1 AND b.status = 'confirmada' AND b.start <= ?2 AND b.end > ?2 ORDER BY b.start LIMIT 1", BOOKING_SELECT),
            params![id, now],
            row_to_booking,
        )
        .optional()?;
    m.next = conn
        .query_row(
            &format!("{} WHERE b.machine_id = ?1 AND b.status IN ('pendiente', 'confirmada') AND b.start > ?2 ORDER BY b.start LIMIT 1", BOOKING_SELECT),
            params![id, now],
            row_to_booking,
        )
        .optional()?;
    m.open_faults = load_faults(conn, id, true)?;
    Ok(Some(m))
}

fn load_booking(conn: &Connection, id: &str) -> rusqlite::Result<Option<Booking>> {
    conn.query_row(&format!("{} WHERE b.id = ?1", BOOKING_SELECT), [id], row_to_booking).optional()
}

/// Personas con reservas vigentes o futuras en la maquina (para avisarles), sin `except`
fn future_booking_users(conn: &Connection, machine_id: &str, except: &str) -> rusqlite::Result<Vec<String>> {
    let now = fmt_time(now_local());
    let mut stmt = conn.prepare(
        "SELECT DISTINCT username FROM machine_bookings
         WHERE machine_id = ?1 AND kind = 'reserva' AND status IN ('pendiente', 'confirmada') AND end > ?2 AND username != ?3",
    )?;
    let rows = stmt.query_map(params![machine_id, now, except], |r| r.get(0))?;
    rows.collect()
}

fn operator_usernames(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT username FROM web_users WHERE role = 'operador'")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    rows.collect()
}

// ─── Fotos ───

/// Decodifica una foto en base64 (acepta prefijo `data:...;base64,`) y valida que sea JPEG o PNG
fn decode_photo(data: &str) -> Result<(Vec<u8>, &'static str), ApiError> {
    let b64 = data.split_once("base64,").map(|(_, b)| b).unwrap_or(data).trim();
    if b64.len() > MAX_PHOTO * 4 / 3 + 8 {
        return Err(bad("La foto pesa mas de 2 MB"));
    }
    let bytes = B64.decode(b64).map_err(|_| bad("Foto invalida"))?;
    if bytes.len() > MAX_PHOTO {
        return Err(bad("La foto pesa mas de 2 MB"));
    }
    let mime = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else {
        return Err(bad("La foto debe ser JPEG o PNG"));
    };
    Ok((bytes, mime))
}

fn insert_photo(conn: &Connection, machine_id: &str, bytes: &[u8], mime: &str, user: &str) -> rusqlite::Result<String> {
    let id = new_id();
    conn.execute(
        "INSERT INTO machine_photos (id, machine_id, data, mime, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, machine_id, bytes, mime, user, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(id)
}

// ─── Maquinas ───

/// GET /api/machines
pub async fn list_machines(State(state): State<AppState>) -> Result<Json<Vec<Machine>>, ApiError> {
    let list = db_op(&state.db, |conn| {
        let ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM machines ORDER BY name COLLATE NOCASE").map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
            rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())?
        };
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(m) = load_machine(conn, &id).map_err(|e| e.to_string())? {
                out.push(m);
            }
        }
        Ok(out)
    })
    .await?;
    Ok(Json(list))
}

/// GET /api/machines/{id}
pub async fn get_machine(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<MachineDetail>, ApiError> {
    let detail = db_op_status(&state.db, move |conn| {
        let machine = load_machine(conn, &id).map_err(db_err)?.ok_or_else(not_found)?;
        let faults = load_faults(conn, &id, false).map_err(db_err)?;
        Ok(MachineDetail { machine, faults })
    })
    .await?;
    Ok(Json(detail))
}

#[derive(Debug, Deserialize)]
pub struct MachineReq {
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub responsible: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub requires_approval: bool,
    #[serde(default)]
    pub requires_certification: bool,
    #[serde(default)]
    pub max_hours: Option<f64>,
    #[serde(default)]
    pub inventory_item_id: Option<String>,
    #[serde(default)]
    pub printer3d_id: Option<String>,
}

fn default_kind() -> String {
    "otra".into()
}

fn opt_trim(v: Option<String>) -> Option<String> {
    v.map(|s| crate::textutil::take_chars(s.trim(), 100)).filter(|s| !s.is_empty())
}

fn validate_machine(req: &mut MachineReq) -> Result<(), ApiError> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() || req.name.chars().count() > 100 {
        return Err(bad("El nombre es obligatorio (maximo 100 caracteres)"));
    }
    if !KINDS.contains(&req.kind.as_str()) {
        return Err(bad("Tipo de maquina invalido"));
    }
    if let Some(s) = &req.status {
        if !STATUSES.contains(&s.as_str()) {
            return Err(bad("Estado invalido"));
        }
    }
    if req.location.chars().count() > 200 || req.description.chars().count() > 5000 || req.instructions.chars().count() > 5000 {
        return Err(bad("Texto demasiado largo"));
    }
    if let Some(h) = req.max_hours {
        if !(h > 0.0 && h <= 24.0 * 14.0) {
            return Err(bad("El maximo por reserva debe estar entre 0 y 336 horas"));
        }
    }
    req.responsible = opt_trim(req.responsible.take());
    req.inventory_item_id = opt_trim(req.inventory_item_id.take());
    req.printer3d_id = opt_trim(req.printer3d_id.take());
    Ok(())
}

/// POST /api/machines (operador)
pub async fn create_machine(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(mut req): Json<MachineReq>,
) -> Result<(StatusCode, Json<Machine>), ApiError> {
    validate_machine(&mut req)?;
    let id = new_id();
    let user = session.username.clone();
    let m = db_op_status(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO machines (id, name, kind, location, description, instructions, responsible, status, requires_approval, max_hours, inventory_item_id, printer3d_id, created_by, created_at, requires_certification)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                id, req.name, req.kind, req.location.trim(), req.description.trim(), req.instructions.trim(), req.responsible,
                req.status.as_deref().unwrap_or("disponible"), req.requires_approval as i64, req.max_hours,
                req.inventory_item_id, req.printer3d_id, user, chrono::Utc::now().to_rfc3339(), req.requires_certification as i64
            ],
        )
        .map_err(db_err)?;
        load_machine(conn, &id).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    state.log_activity("Maquinas", &format!("Nueva maquina: {}", m.name), &session.username).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(m)))
}

/// PUT /api/machines/{id} (operador)
pub async fn update_machine(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(mut req): Json<MachineReq>,
) -> Result<Json<Machine>, ApiError> {
    validate_machine(&mut req)?;
    let m = db_op_status(&state.db, move |conn| {
        let n = conn
            .execute(
                "UPDATE machines SET name = ?2, kind = ?3, location = ?4, description = ?5, instructions = ?6, responsible = ?7,
                 status = COALESCE(?8, status), requires_approval = ?9, max_hours = ?10, inventory_item_id = ?11, printer3d_id = ?12,
                 requires_certification = ?13
                 WHERE id = ?1",
                params![
                    id, req.name, req.kind, req.location.trim(), req.description.trim(), req.instructions.trim(), req.responsible,
                    req.status, req.requires_approval as i64, req.max_hours, req.inventory_item_id, req.printer3d_id,
                    req.requires_certification as i64
                ],
            )
            .map_err(db_err)?;
        if n == 0 {
            return Err(not_found());
        }
        load_machine(conn, &id).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    state.log_activity("Maquinas", &format!("Maquina editada: {} ({})", m.name, m.status), &session.username).await;
    publish(&state);
    Ok(Json(m))
}

/// DELETE /api/machines/{id} (admin): borra tambien reservas, fallas y fotos
pub async fn delete_machine(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let name = db_op_status(&state.db, move |conn| {
        let name: String = conn
            .query_row("SELECT name FROM machines WHERE id = ?1", [&id], |r| r.get(0))
            .optional()
            .map_err(db_err)?
            .ok_or_else(not_found)?;
        for table in ["machine_bookings", "machine_faults", "machine_photos", "machine_certifications"] {
            conn.execute(&format!("DELETE FROM {} WHERE machine_id = ?1", table), [&id]).map_err(db_err)?;
        }
        conn.execute("DELETE FROM machines WHERE id = ?1", [&id]).map_err(db_err)?;
        Ok(name)
    })
    .await?;
    state.log_activity("Maquinas", &format!("Maquina eliminada: {}", name), &session.username).await;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct PhotoReq {
    pub data: String,
}

/// POST /api/machines/{id}/photo (operador): foto principal de la ficha
pub async fn set_photo(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<PhotoReq>,
) -> Result<Json<Machine>, ApiError> {
    let (bytes, mime) = decode_photo(&req.data)?;
    let user = session.username.clone();
    let m = db_op_status(&state.db, move |conn| {
        let old: Option<String> = conn
            .query_row("SELECT photo_id FROM machines WHERE id = ?1", [&id], |r| r.get(0))
            .optional()
            .map_err(db_err)?
            .ok_or_else(not_found)?;
        let pid = insert_photo(conn, &id, &bytes, mime, &user).map_err(db_err)?;
        conn.execute("UPDATE machines SET photo_id = ?2 WHERE id = ?1", params![id, pid]).map_err(db_err)?;
        if let Some(old) = old {
            conn.execute("DELETE FROM machine_photos WHERE id = ?1", [old]).map_err(db_err)?;
        }
        load_machine(conn, &id).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    publish(&state);
    Ok(Json(m))
}

/// GET /api/machines/photos/{pid}
pub async fn get_photo(State(state): State<AppState>, Path(pid): Path<String>) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    let (data, mime) = db_op_status(&state.db, move |conn| {
        conn.query_row("SELECT data, mime FROM machine_photos WHERE id = ?1", [&pid], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
        })
        .optional()
        .map_err(db_err)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Foto no encontrada".to_string()))
    })
    .await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, mime),
            (axum::http::header::CACHE_CONTROL, "private, max-age=86400".to_string()),
        ],
        data,
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
pub struct QrQuery {
    pub base: String,
}

#[derive(Debug, Serialize)]
pub struct QrResp {
    pub url: String,
    pub svg: String,
}

/// Origen `http(s)://host[:puerto]` tal como lo ve el navegador (sin ruta ni credenciales)
fn clean_origin(base: &str) -> Result<String, ApiError> {
    let u = reqwest::Url::parse(base.trim()).map_err(|_| bad("Direccion invalida"))?;
    let ok = matches!(u.scheme(), "http" | "https")
        && u.host_str().is_some()
        && u.username().is_empty()
        && u.password().is_none()
        && u.query().is_none()
        && u.fragment().is_none();
    if !ok {
        return Err(bad("Direccion invalida"));
    }
    let mut origin = u.origin().ascii_serialization();
    // conserva una subcarpeta (p.ej. el puente PHP en https://dominio/labnas)
    let path = u.path().trim_end_matches('/');
    if !path.is_empty() {
        origin.push_str(path);
    }
    Ok(origin)
}

/// GET /api/machines/{id}/qr?base=<origen>: QR que lleva a la ficha
pub async fn machine_qr(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<QrQuery>,
) -> Result<Json<QrResp>, ApiError> {
    let origin = clean_origin(&q.base)?;
    let id2 = id.clone();
    let exists = db_op(&state.db, move |conn| {
        conn.query_row("SELECT 1 FROM machines WHERE id = ?1", [&id2], |_| Ok(())).optional().map_err(|e| e.to_string())
    })
    .await?;
    if exists.is_none() {
        return Err(not_found());
    }
    let url = format!("{}/machines/{}", origin, id);
    let svg = crate::totp::qr_svg(&url).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(QrResp { url, svg }))
}

// ─── Reservas ───

#[derive(Debug, Deserialize, Default)]
pub struct BookingQuery {
    /// Desde (fecha `YYYY-MM-DD` u hora `YYYY-MM-DDTHH:MM`): reservas que terminan despues
    pub from: Option<String>,
    /// Hasta: reservas que empiezan antes
    pub to: Option<String>,
    pub machine: Option<String>,
    #[serde(default)]
    pub mine: bool,
    pub status: Option<String>,
}

/// GET /api/machines/bookings
pub async fn list_bookings(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<BookingQuery>,
) -> Result<Json<Vec<Booking>>, ApiError> {
    let user = session.username.clone();
    let list = db_op(&state.db, move |conn| {
        let mut sql = format!("{} WHERE 1 = 1", BOOKING_SELECT);
        let mut args: Vec<String> = Vec::new();
        let mut push = |cond: &str, v: String, sql: &mut String| {
            args.push(v);
            sql.push_str(&format!(" AND {} ?{}", cond, args.len()));
        };
        if let Some(f) = q.from.filter(|s| !s.is_empty()) {
            push("b.end >", f, &mut sql);
        }
        if let Some(t) = q.to.filter(|s| !s.is_empty()) {
            push("b.start <", t, &mut sql);
        }
        if let Some(m) = q.machine.filter(|s| !s.is_empty()) {
            push("b.machine_id =", m, &mut sql);
        }
        if q.mine {
            push("b.username =", user, &mut sql);
        }
        match q.status.as_deref() {
            Some("") | None => sql.push_str(" AND b.status IN ('pendiente', 'confirmada')"),
            Some("todas") => {}
            Some(s) => push("b.status =", s.to_string(), &mut sql),
        }
        sql.push_str(" ORDER BY b.start LIMIT 500");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_booking).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())
    })
    .await?;
    Ok(Json(list))
}

#[derive(Debug, Deserialize)]
pub struct BookingReq {
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default = "default_booking_kind")]
    pub kind: String,
}

fn default_booking_kind() -> String {
    "reserva".into()
}

/// POST /api/machines/{id}/bookings
pub async fn create_booking(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(machine_id): Path<String>,
    Json(req): Json<BookingReq>,
) -> Result<(StatusCode, Json<Booking>), ApiError> {
    let staff = is_staff(&session);
    if !matches!(req.kind.as_str(), "reserva" | "bloqueo") {
        return Err(bad("Tipo de reserva invalido"));
    }
    if req.kind == "bloqueo" && !staff {
        return Err(forbidden("Solo administradores y operadores bloquean horarios"));
    }
    let purpose = req.purpose.trim().to_string();
    if purpose.chars().count() > 500 {
        return Err(bad("Proposito demasiado largo"));
    }
    let (start, end) = (parse_time(&req.start)?, parse_time(&req.end)?);
    if end <= start {
        return Err(bad("La hora de termino debe ser posterior a la de inicio"));
    }
    let now = now_local();
    if end <= now || (start < now - Duration::minutes(5) && req.kind == "reserva") {
        return Err(bad("No se puede reservar en el pasado"));
    }
    if start > now + Duration::days(MAX_DAYS_AHEAD) {
        return Err(bad(format!("Se puede reservar hasta {} dias adelante", MAX_DAYS_AHEAD)));
    }
    let (start_s, end_s) = (fmt_time(start), fmt_time(end));
    let user = session.username.clone();
    let kind = req.kind.clone();
    let hours = (end - start).num_minutes() as f64 / 60.0;

    let booking = db_op_status(&state.db, move |conn| {
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate).map_err(db_err)?;
        let m = load_machine(&tx, &machine_id).map_err(db_err)?.ok_or_else(not_found)?;
        if kind == "reserva" {
            match m.status.as_str() {
                "fuera_servicio" => return Err(conflict("La maquina esta fuera de servicio hasta que la reparen")),
                "retirada" => return Err(conflict("La maquina fue retirada")),
                _ => {}
            }
            if m.requires_certification && !staff && !crate::handlers::machine_certs::is_certified(&tx, &machine_id, &user).map_err(db_err)? {
                return Err(forbidden(&format!("Para reservar {} necesitas la induccion. Pidela desde la ficha de la maquina.", m.name)));
            }
            if let Some(max) = m.max_hours {
                if hours > max + 1e-9 && !staff {
                    return Err(bad(format!("Maximo {} horas por reserva en esta maquina", max)));
                }
            }
        }
        let clash = tx
            .query_row(
                "SELECT username, kind, start, end FROM machine_bookings
                 WHERE machine_id = ?1 AND status IN ('pendiente', 'confirmada') AND start < ?3 AND end > ?2
                 ORDER BY start LIMIT 1",
                params![machine_id, start_s, end_s],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)),
            )
            .optional()
            .map_err(db_err)?;
        if let Some((who, k, s, e)) = clash {
            let what = if k == "bloqueo" { "un bloqueo".to_string() } else { format!("la reserva de {}", who) };
            return Err(conflict(format!("Choca con {} ({} a {})", what, short_time(&s), short_time(&e))));
        }
        let status = if kind == "reserva" && m.requires_approval && !staff { "pendiente" } else { "confirmada" };
        let id = new_id();
        tx.execute(
            "INSERT INTO machine_bookings (id, machine_id, username, kind, start, end, purpose, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, machine_id, user, kind, start_s, end_s, purpose, status, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        load_booking(conn, &id).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;

    state
        .log_activity(
            "Maquinas",
            &format!("{} {}: {} a {}", if booking.kind == "bloqueo" { "Bloqueo" } else { "Reserva" }, booking.machine_name, booking.start, booking.end),
            &session.username,
        )
        .await;
    if booking.status == "pendiente" {
        let text = format!(
            "📅 *Reserva por aprobar*\n{} pide *{}* el {} a {}\n{}",
            md(&booking.username), md(&booking.machine_name), short_time(&booking.start), short_time(&booking.end), md(&booking.purpose)
        );
        let st = state.clone();
        tokio::spawn(async move {
            crate::handlers::notifications::notify_staff(&st, &text).await;
        });
        crate::events::notify(&state, Audience::Admins, MODULE, Level::Info, "Reserva por aprobar", &format!("{}: {}", booking.username, booking.machine_name));
    }
    publish(&state);
    Ok((StatusCode::CREATED, Json(booking)))
}

/// Cambia el estado de una reserva (si `check` lo permite) y la devuelve
async fn change_booking(state: &AppState, id: String, check: impl FnOnce(&Booking) -> Result<(), ApiError> + Send + 'static, new_status: &'static str, by: String) -> Result<Booking, ApiError> {
    db_op_status(&state.db, move |conn| {
        let b = load_booking(conn, &id).map_err(db_err)?.ok_or_else(|| (StatusCode::NOT_FOUND, "Reserva no encontrada".to_string()))?;
        check(&b)?;
        conn.execute(
            "UPDATE machine_bookings SET status = ?2, decided_by = ?3, decided_at = ?4 WHERE id = ?1",
            params![id, new_status, by, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
        load_booking(conn, &id).map_err(db_err)?.ok_or_else(not_found)
    })
    .await
}

fn alive(b: &Booking) -> Result<(), ApiError> {
    if matches!(b.status.as_str(), "pendiente" | "confirmada") {
        Ok(())
    } else {
        Err(conflict(format!("La reserva ya esta {}", b.status)))
    }
}

/// Avisa a la persona dueña de la reserva (en la web y por Telegram)
fn tell_owner(state: &AppState, b: &Booking, title: &str, by: &str) {
    if b.username == by || b.kind == "bloqueo" {
        return;
    }
    let body = format!("{} · {} a {}", b.machine_name, short_time(&b.start), short_time(&b.end));
    crate::events::notify(state, Audience::User(b.username.clone()), MODULE, Level::Info, title, &body);
    let text = format!("📅 *{}*\n{}", md(title), md(&body));
    let (st, user) = (state.clone(), b.username.clone());
    tokio::spawn(async move {
        crate::handlers::notifications::notify_users(&st, &[user], &text).await;
    });
}

/// POST /api/machines/bookings/{id}/cancel (dueño u operador)
pub async fn cancel_booking(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<Booking>, ApiError> {
    let (staff, user) = (is_staff(&session), session.username.clone());
    let b = change_booking(
        &state,
        id,
        move |b| {
            if b.username != user && !staff {
                return Err(forbidden("Solo puedes cancelar tus reservas"));
            }
            alive(b)
        },
        "cancelada",
        session.username.clone(),
    )
    .await?;
    state.log_activity("Maquinas", &format!("Reserva cancelada: {} {}", b.machine_name, b.start), &session.username).await;
    tell_owner(&state, &b, "Cancelaron tu reserva", &session.username);
    publish(&state);
    Ok(Json(b))
}

/// POST /api/machines/bookings/{id}/approve (operador)
pub async fn approve_booking(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<Booking>, ApiError> {
    let b = change_booking(&state, id, pending_only, "confirmada", session.username.clone()).await?;
    state.log_activity("Maquinas", &format!("Reserva aprobada: {} de {}", b.machine_name, b.username), &session.username).await;
    tell_owner(&state, &b, "Aprobaron tu reserva", &session.username);
    publish(&state);
    Ok(Json(b))
}

/// POST /api/machines/bookings/{id}/reject (operador)
pub async fn reject_booking(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<Booking>, ApiError> {
    let b = change_booking(&state, id, pending_only, "rechazada", session.username.clone()).await?;
    state.log_activity("Maquinas", &format!("Reserva rechazada: {} de {}", b.machine_name, b.username), &session.username).await;
    tell_owner(&state, &b, "Rechazaron tu reserva", &session.username);
    publish(&state);
    Ok(Json(b))
}

fn pending_only(b: &Booking) -> Result<(), ApiError> {
    if b.status == "pendiente" {
        Ok(())
    } else {
        Err(conflict(format!("La reserva ya esta {}", b.status)))
    }
}

// ─── Fallas ───

#[derive(Debug, Deserialize)]
pub struct FaultReq {
    pub description: String,
    #[serde(default)]
    pub photo: Option<String>,
}

/// POST /api/machines/{id}/faults: cualquiera reporta; la maquina queda fuera de servicio
pub async fn report_fault(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(machine_id): Path<String>,
    Json(req): Json<FaultReq>,
) -> Result<(StatusCode, Json<Fault>), ApiError> {
    let description = req.description.trim().to_string();
    if description.is_empty() || description.chars().count() > 2000 {
        return Err(bad("Describe la falla (maximo 2000 caracteres)"));
    }
    let photo = match req.photo.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => Some(decode_photo(p)?),
        None => None,
    };
    let jpeg = photo.as_ref().filter(|(_, mime)| *mime == "image/jpeg").map(|(b, _)| b.clone());
    let user = session.username.clone();
    let (fault, name, owners, operators) = db_op_status(&state.db, move |conn| {
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate).map_err(db_err)?;
        let (name, status): (String, String) = tx
            .query_row("SELECT name, status FROM machines WHERE id = ?1", [&machine_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(db_err)?
            .ok_or_else(not_found)?;
        if status == "retirada" {
            return Err(conflict("La maquina fue retirada"));
        }
        let photo_id = match &photo {
            Some((bytes, mime)) => Some(insert_photo(&tx, &machine_id, bytes, mime, &user).map_err(db_err)?),
            None => None,
        };
        let id = new_id();
        tx.execute(
            "INSERT INTO machine_faults (id, machine_id, reported_by, description, photo_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, machine_id, user, description, photo_id, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
        tx.execute("UPDATE machines SET status = 'fuera_servicio' WHERE id = ?1", [&machine_id]).map_err(db_err)?;
        let owners = future_booking_users(&tx, &machine_id, &user).map_err(db_err)?;
        let operators = operator_usernames(&tx).map_err(db_err)?;
        let fault = tx
            .query_row(&format!("SELECT {} FROM machine_faults WHERE id = ?1", FAULT_COLS), [&id], row_to_fault)
            .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok((fault, name, owners, operators))
    })
    .await?;

    state.log_activity("Maquinas", &format!("Falla en {}: {}", name, crate::textutil::ellipsize(&fault.description, 120)), &session.username).await;
    crate::events::notify(&state, Audience::Admins, MODULE, Level::Warning, &format!("Falla en {}", name), &fault.description);
    let staff_text = format!("🔧 *Falla en {}* (fuera de servicio)\nReporta {}: {}", md(&name), md(&fault.reported_by), md(&fault.description));
    let owner_text = format!(
        "🔧 *{}* quedo fuera de servicio por una falla. Tu reserva sigue anotada, pero no se podra usar hasta que la reparen.",
        md(&name)
    );
    let caption = format!("🔧 Falla en {} (fuera de servicio)\nReporta {}: {}", name, fault.reported_by, fault.description);
    let st = state.clone();
    tokio::spawn(async move {
        let photo_sent = match jpeg {
            Some(bytes) => crate::handlers::notifications::notify_admins_photo(&st, bytes, &caption, &operators).await.is_ok(),
            None => false,
        };
        if !photo_sent {
            crate::handlers::notifications::notify_staff(&st, &staff_text).await;
        }
        crate::handlers::notifications::notify_users(&st, &owners, &owner_text).await;
    });
    publish(&state);
    Ok((StatusCode::CREATED, Json(fault)))
}

#[derive(Debug, Deserialize, Default)]
pub struct ResolveReq {
    #[serde(default)]
    pub note: String,
}

/// POST /api/machines/faults/{id}/resolve (operador): si no quedan fallas abiertas la
/// maquina vuelve a estar disponible
pub async fn resolve_fault(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<ResolveReq>,
) -> Result<Json<Fault>, ApiError> {
    let note = req.note.trim().to_string();
    if note.chars().count() > 2000 {
        return Err(bad("Nota demasiado larga"));
    }
    let user = session.username.clone();
    let (fault, name, available, owners) = db_op_status(&state.db, move |conn| {
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate).map_err(db_err)?;
        let f = tx
            .query_row(&format!("SELECT {} FROM machine_faults WHERE id = ?1", FAULT_COLS), [&id], row_to_fault)
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Falla no encontrada".to_string()))?;
        if f.resolved_at.is_some() {
            return Err(conflict("La falla ya estaba resuelta"));
        }
        tx.execute(
            "UPDATE machine_faults SET resolved_by = ?2, resolved_at = ?3, resolution_note = ?4 WHERE id = ?1",
            params![id, user, chrono::Utc::now().to_rfc3339(), note],
        )
        .map_err(db_err)?;
        let open: i64 = tx
            .query_row("SELECT COUNT(*) FROM machine_faults WHERE machine_id = ?1 AND resolved_at IS NULL", [&f.machine_id], |r| r.get(0))
            .map_err(db_err)?;
        let available = open == 0
            && tx
                .execute("UPDATE machines SET status = 'disponible' WHERE id = ?1 AND status = 'fuera_servicio'", [&f.machine_id])
                .map_err(db_err)?
                > 0;
        let name: String = tx.query_row("SELECT name FROM machines WHERE id = ?1", [&f.machine_id], |r| r.get(0)).unwrap_or_default();
        let mut owners = if available { future_booking_users(&tx, &f.machine_id, &user).map_err(db_err)? } else { Vec::new() };
        if f.reported_by != user && !owners.contains(&f.reported_by) {
            owners.push(f.reported_by.clone());
        }
        let fault = tx
            .query_row(&format!("SELECT {} FROM machine_faults WHERE id = ?1", FAULT_COLS), [&id], row_to_fault)
            .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok((fault, name, available, owners))
    })
    .await?;

    state.log_activity("Maquinas", &format!("Falla resuelta en {}", name), &session.username).await;
    let text = if available {
        format!("✅ *{}* ya esta reparada y disponible.", md(&name))
    } else {
        format!("✅ Se resolvio una falla de *{}* (aun quedan otras abiertas).", md(&name))
    };
    let st = state.clone();
    tokio::spawn(async move {
        crate::handlers::notifications::notify_users(&st, &owners, &text).await;
    });
    publish(&state);
    Ok(Json(fault))
}

// ─── Recordatorios ───

/// Tarea de fondo del modulo: avisa por Telegram unos minutos antes de cada reserva
pub async fn reminder_loop(state: AppState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        tick.tick().await;
        let due = db_op(&state.db, |conn| {
            let now = now_local();
            let (from, to) = (fmt_time(now), fmt_time(now + Duration::minutes(REMIND_MIN)));
            let mut stmt = conn
                .prepare(&format!(
                    "{} WHERE b.kind = 'reserva' AND b.status = 'confirmada' AND b.reminded = 0 AND b.start >= ?1 AND b.start <= ?2",
                    BOOKING_SELECT
                ))
                .map_err(|e| e.to_string())?;
            let list: Vec<Booking> = stmt
                .query_map(params![from, to], row_to_booking)
                .map_err(|e| e.to_string())?
                .collect::<rusqlite::Result<_>>()
                .map_err(|e| e.to_string())?;
            for b in &list {
                conn.execute("UPDATE machine_bookings SET reminded = 1 WHERE id = ?1", [&b.id]).map_err(|e| e.to_string())?;
            }
            Ok(list)
        })
        .await
        .unwrap_or_default();
        for b in due {
            let text = format!("⏰ Tu reserva de *{}* empieza a las {}.", md(&b.machine_name), short_time(&b.start));
            crate::handlers::notifications::notify_users(&state, std::slice::from_ref(&b.username), &text).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origen_del_qr() {
        assert_eq!(clean_origin("https://192.168.50.114:3443").unwrap(), "https://192.168.50.114:3443");
        assert_eq!(clean_origin("http://labnas.local:3000/").unwrap(), "http://labnas.local:3000");
        assert_eq!(clean_origin("https://lab.ejemplo.cl/labnas/").unwrap(), "https://lab.ejemplo.cl/labnas");
        assert!(clean_origin("javascript:alert(1)").is_err());
        assert!(clean_origin("https://user:pw@host").is_err());
        assert!(clean_origin("https://host/?x=1").is_err());
        assert!(clean_origin("ftp://host").is_err());
    }

    #[test]
    fn markdown_escapado() {
        assert_eq!(md("cnc_1 *x* `y` [z]"), "cnc\\_1 \\*x\\* \\`y\\` \\[z]");
    }

    #[test]
    fn foto_valida() {
        let jpeg = B64.encode([0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]);
        assert_eq!(decode_photo(&format!("data:image/jpeg;base64,{}", jpeg)).unwrap().1, "image/jpeg");
        assert!(decode_photo(&B64.encode(b"<svg></svg>")).is_err());
        assert!(decode_photo("no es base64!!").is_err());
    }
}
