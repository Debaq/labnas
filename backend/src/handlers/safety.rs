//! Seguridad laboral (idea 17): capacitaciones generales con vigencia, fichas de seguridad
//! (hojas de datos de materiales y procedimientos) y registro de incidentes y casi-accidentes.
//! Las capacitaciones que vencen aparecen en Plazos. Los incidentes no se borran: se cierran
//! con una nota. Ver docs/SEGURIDAD.md.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chrono::{Local, Months, NaiveDate, NaiveDateTime};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::db_op_status;
use crate::error::ApiError;
use crate::events::{Audience, Level};
use crate::handlers::machines::{bad, conflict, db_err, forbidden, md};
use crate::roles::Level as Lv;
use crate::state::{AppState, SessionInfo};

const MODULE: Option<&str> = Some("seguridad");
const TIME_FMT: &str = "%Y-%m-%dT%H:%M";
const SEVERITIES: &[&str] = &["casi_accidente", "leve", "grave"];
const STATUSES: &[&str] = &["abierto", "en_revision", "cerrado"];
const SHEET_KINDS: &[&str] = &["material", "procedimiento", "equipo", "otro"];
const MAX_PHOTO: usize = 2 * 1024 * 1024;
const MAX_FILE: usize = 5 * 1024 * 1024;

fn not_found(what: &str) -> ApiError {
    (StatusCode::NOT_FOUND, format!("{} no encontrado", what))
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

fn today() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn publish(state: &AppState) {
    state.events.publish("safety", (), Audience::All, MODULE);
}

fn is_staff(s: &SessionInfo) -> bool {
    s.can("seguridad", Lv::Gestionar)
}

fn staff_users(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    crate::roles::users_with_level(conn, "seguridad", Lv::Gestionar)
}

fn tell(state: &AppState, users: Vec<String>, level: Level, emoji: &str, title: &str, body: &str) {
    for u in &users {
        crate::events::notify(state, Audience::User(u.clone()), MODULE, level, title, body);
    }
    let text = format!("{} *{}*\n{}", emoji, md(title), md(body));
    let st = state.clone();
    tokio::spawn(async move {
        crate::handlers::notifications::notify_users(&st, &users, &text).await;
    });
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
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map(|d| d.format("%Y-%m-%d").to_string())
        .map_err(|_| bad("Fecha invalida (AAAA-MM-DD)"))
}

fn opt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// Decodifica un adjunto en base64 y lo valida por su contenido (no por el nombre)
fn decode_blob(data: &str, max: usize, allow_pdf: bool) -> Result<(Vec<u8>, &'static str), ApiError> {
    let b64 = data.split_once("base64,").map(|(_, b)| b).unwrap_or(data).trim();
    let too_big = || bad(format!("El archivo pesa mas de {} MB", max / 1024 / 1024));
    if b64.len() > max * 4 / 3 + 8 {
        return Err(too_big());
    }
    let bytes = B64.decode(b64).map_err(|_| bad("Archivo invalido"))?;
    if bytes.len() > max {
        return Err(too_big());
    }
    let mime = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if allow_pdf && bytes.starts_with(b"%PDF") {
        "application/pdf"
    } else if allow_pdf {
        return Err(bad("El archivo debe ser PDF, JPEG o PNG"));
    } else {
        return Err(bad("La foto debe ser JPEG o PNG"));
    };
    Ok((bytes, mime))
}

fn blob_response(data: Vec<u8>, mime: String, name: Option<String>) -> axum::response::Response {
    use axum::response::IntoResponse;
    let mut headers = axum::http::HeaderMap::new();
    if let Ok(v) = mime.parse() {
        headers.insert(axum::http::header::CONTENT_TYPE, v);
    }
    headers.insert(axum::http::header::CACHE_CONTROL, "private, max-age=3600".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    if let Some(n) = name {
        let safe: String = n.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ')).collect();
        if let Ok(v) = format!("inline; filename=\"{}\"", safe).parse() {
            headers.insert(axum::http::header::CONTENT_DISPOSITION, v);
        }
    }
    (headers, data).into_response()
}

// ─── Capacitaciones ───

#[derive(Debug, Clone, Serialize)]
pub struct Training {
    pub id: String,
    pub name: String,
    pub description: String,
    pub valid_months: Option<i64>,
    pub mandatory: bool,
    pub created_by: String,
    pub created_at: String,
    /// Personas con la capacitacion vigente
    pub holders: i64,
}

const TRAINING_SELECT: &str = "SELECT t.id, t.name, t.description, t.valid_months, t.mandatory, t.created_by, t.created_at,
    (SELECT COUNT(*) FROM safety_training_records r WHERE r.training_id = t.id AND (r.expires_on IS NULL OR r.expires_on >= date('now','localtime')))
    FROM safety_trainings t";

fn row_training(r: &rusqlite::Row) -> rusqlite::Result<Training> {
    Ok(Training {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        valid_months: r.get(3)?,
        mandatory: r.get::<_, i64>(4)? != 0,
        created_by: r.get(5)?,
        created_at: r.get(6)?,
        holders: r.get(7)?,
    })
}

fn load_training(conn: &Connection, id: &str) -> rusqlite::Result<Option<Training>> {
    conn.query_row(&format!("{} WHERE t.id = ?1", TRAINING_SELECT), [id], row_training).optional()
}

/// GET /api/safety/trainings
pub async fn list_trainings(State(state): State<AppState>) -> Result<Json<Vec<Training>>, ApiError> {
    let list = db_op_status(&state.db, |conn| {
        let mut stmt = conn.prepare(&format!("{} ORDER BY t.mandatory DESC, t.name", TRAINING_SELECT)).map_err(db_err)?;
        let rows = stmt.query_map([], row_training).map_err(db_err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
    })
    .await?;
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct TrainingReq {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub valid_months: Option<i64>,
    #[serde(default)]
    pub mandatory: bool,
}

fn check_training(req: &TrainingReq) -> Result<(String, String, Option<i64>), ApiError> {
    let name = required(&req.name, 120, "El nombre")?;
    let desc = clean(&req.description, 2000, "La descripcion")?;
    if req.valid_months.is_some_and(|m| !(1..=240).contains(&m)) {
        return Err(bad("La vigencia debe estar entre 1 y 240 meses"));
    }
    Ok((name, desc, req.valid_months))
}

/// POST /api/safety/trainings (Gestionar)
pub async fn create_training(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<TrainingReq>,
) -> Result<(StatusCode, Json<Training>), ApiError> {
    let (name, desc, months) = check_training(&req)?;
    let (id, user, mandatory) = (new_id(), session.username.clone(), req.mandatory as i64);
    let t = db_op_status(&state.db, move |conn| {
        let dup: bool = conn
            .query_row("SELECT 1 FROM safety_trainings WHERE name = ?1 COLLATE NOCASE", [&name], |_| Ok(()))
            .optional()
            .map_err(db_err)?
            .is_some();
        if dup {
            return Err(conflict("Ya existe una capacitacion con ese nombre"));
        }
        conn.execute(
            "INSERT INTO safety_trainings (id, name, description, valid_months, mandatory, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, name, desc, months, mandatory, user, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
        load_training(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Capacitacion"))
    })
    .await?;
    state.log_activity("Seguridad", &format!("Capacitacion creada: {}", t.name), &session.username).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(t)))
}

/// PUT /api/safety/trainings/{id} (Gestionar)
pub async fn update_training(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<TrainingReq>,
) -> Result<Json<Training>, ApiError> {
    let (name, desc, months) = check_training(&req)?;
    let mandatory = req.mandatory as i64;
    let t = db_op_status(&state.db, move |conn| {
        let n = conn
            .execute(
                "UPDATE safety_trainings SET name = ?2, description = ?3, valid_months = ?4, mandatory = ?5 WHERE id = ?1",
                params![id, name, desc, months, mandatory],
            )
            .map_err(db_err)?;
        if n == 0 {
            return Err(not_found("Capacitacion"));
        }
        load_training(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Capacitacion"))
    })
    .await?;
    state.log_activity("Seguridad", &format!("Capacitacion editada: {}", t.name), &session.username).await;
    publish(&state);
    Ok(Json(t))
}

/// DELETE /api/safety/trainings/{id} (Gestionar): quita la capacitacion y sus registros
pub async fn delete_training(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let t = db_op_status(&state.db, move |conn| {
        let t = load_training(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Capacitacion"))?;
        conn.execute("DELETE FROM safety_training_records WHERE training_id = ?1", [&id]).map_err(db_err)?;
        conn.execute("DELETE FROM safety_trainings WHERE id = ?1", [&id]).map_err(db_err)?;
        Ok(t)
    })
    .await?;
    state.log_activity("Seguridad", &format!("Capacitacion eliminada: {}", t.name), &session.username).await;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Clone, Serialize)]
pub struct TrainingRecord {
    pub training_id: String,
    pub training_name: String,
    pub username: String,
    pub granted_by: String,
    pub granted_on: String,
    pub expires_on: Option<String>,
    pub notes: String,
    /// vigente | vencida
    pub status: String,
}

const RECORD_SELECT: &str = "SELECT r.training_id, COALESCE(t.name, ''), r.username, r.granted_by, r.granted_on, r.expires_on, r.notes
    FROM safety_training_records r LEFT JOIN safety_trainings t ON t.id = r.training_id";

fn row_record(r: &rusqlite::Row) -> rusqlite::Result<TrainingRecord> {
    let expires: Option<String> = r.get(5)?;
    let status = if expires.as_deref().is_some_and(|e| e < today().as_str()) { "vencida" } else { "vigente" };
    Ok(TrainingRecord {
        training_id: r.get(0)?,
        training_name: r.get(1)?,
        username: r.get(2)?,
        granted_by: r.get(3)?,
        granted_on: r.get(4)?,
        expires_on: expires,
        notes: r.get(6)?,
        status: status.into(),
    })
}

fn load_record(conn: &Connection, training_id: &str, username: &str) -> rusqlite::Result<Option<TrainingRecord>> {
    conn.query_row(&format!("{} WHERE r.training_id = ?1 AND r.username = ?2", RECORD_SELECT), params![training_id, username], row_record)
        .optional()
}

#[derive(Deserialize)]
pub struct RecordsQuery {
    #[serde(default)]
    pub training_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

/// GET /api/safety/records?training_id=&username= (quien no gestiona solo ve lo propio)
pub async fn list_records(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<RecordsQuery>,
) -> Result<Json<Vec<TrainingRecord>>, ApiError> {
    let staff = is_staff(&session);
    let me = session.username.clone();
    let list = db_op_status(&state.db, move |conn| {
        let mut stmt = conn.prepare(&format!("{} ORDER BY t.name, r.username", RECORD_SELECT)).map_err(db_err)?;
        let rows = stmt.query_map([], row_record).map_err(db_err)?;
        let mut list = rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)?;
        list.retain(|r| {
            (staff || r.username == me)
                && q.training_id.as_ref().is_none_or(|t| &r.training_id == t)
                && q.username.as_ref().is_none_or(|u| &r.username == u)
        });
        Ok(list)
    })
    .await?;
    Ok(Json(list))
}

#[derive(Debug, Serialize)]
pub struct MyTraining {
    pub training: Training,
    /// vigente | por_vencer | vencida | pendiente
    pub status: String,
    pub granted_on: Option<String>,
    pub expires_on: Option<String>,
}

/// GET /api/safety/trainings/mine: el catalogo con el estado de la persona en cada una
pub async fn my_trainings(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<Vec<MyTraining>>, ApiError> {
    let me = session.username.clone();
    let list = db_op_status(&state.db, move |conn| {
        let mut stmt = conn.prepare(&format!("{} ORDER BY t.mandatory DESC, t.name", TRAINING_SELECT)).map_err(db_err)?;
        let trainings = stmt
            .query_map([], row_training)
            .map_err(db_err)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_err)?;
        let soon = (Local::now().date_naive() + chrono::Duration::days(30)).format("%Y-%m-%d").to_string();
        let mut out = Vec::new();
        for t in trainings {
            let rec = load_record(conn, &t.id, &me).map_err(db_err)?;
            let (status, granted_on, expires_on) = match rec {
                None => ("pendiente".to_string(), None, None),
                Some(r) => {
                    let st = if r.status == "vencida" {
                        "vencida"
                    } else if r.expires_on.as_deref().is_some_and(|e| e <= soon.as_str()) {
                        "por_vencer"
                    } else {
                        "vigente"
                    };
                    (st.to_string(), Some(r.granted_on), r.expires_on)
                }
            };
            out.push(MyTraining { training: t, status, granted_on, expires_on });
        }
        Ok(out)
    })
    .await?;
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct RecordReq {
    pub training_id: String,
    pub username: String,
    /// Fecha en que la recibio (por defecto hoy)
    #[serde(default)]
    pub granted_on: Option<String>,
    /// Vencimiento; si falta y la capacitacion tiene vigencia, se calcula
    #[serde(default)]
    pub expires_on: Option<String>,
    #[serde(default)]
    pub notes: String,
}

/// POST /api/safety/records (Gestionar): dar o renovar una capacitacion a una persona
pub async fn grant_record(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<RecordReq>,
) -> Result<Json<TrainingRecord>, ApiError> {
    let username = req.username.trim().to_string();
    let notes = clean(&req.notes, 500, "La nota")?;
    let granted_on = match opt(&req.granted_on) {
        Some(d) => parse_date(&d)?,
        None => today(),
    };
    if granted_on > today() {
        return Err(bad("La fecha de la capacitacion no puede ser futura"));
    }
    let explicit_expiry = opt(&req.expires_on).map(|d| parse_date(&d)).transpose()?;
    let by = session.username.clone();
    let tid = req.training_id.clone();
    let rec = db_op_status(&state.db, move |conn| {
        let t = load_training(conn, &tid).map_err(db_err)?.ok_or_else(|| not_found("Capacitacion"))?;
        let role: Option<String> = conn
            .query_row("SELECT role FROM web_users WHERE username = ?1", [&username], |r| r.get(0))
            .optional()
            .map_err(db_err)?;
        match role.as_deref() {
            None => return Err(bad("Ese usuario no existe")),
            Some("pendiente") => return Err(bad("La cuenta de ese usuario aun no esta aprobada")),
            _ => {}
        }
        let expires = match (explicit_expiry, t.valid_months) {
            (Some(e), _) => Some(e),
            (None, Some(m)) => NaiveDate::parse_from_str(&granted_on, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.checked_add_months(Months::new(m as u32)))
                .map(|d| d.format("%Y-%m-%d").to_string()),
            (None, None) => None,
        };
        if expires.as_deref().is_some_and(|e| e < granted_on.as_str()) {
            return Err(bad("El vencimiento es anterior a la fecha de la capacitacion"));
        }
        conn.execute(
            "INSERT INTO safety_training_records (training_id, username, granted_by, granted_on, expires_on, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(training_id, username) DO UPDATE SET granted_by = ?3, granted_on = ?4, expires_on = ?5, notes = ?6",
            params![tid, username, by, granted_on, expires, notes],
        )
        .map_err(db_err)?;
        load_record(conn, &tid, &username).map_err(db_err)?.ok_or_else(|| not_found("Registro"))
    })
    .await?;
    state
        .log_activity("Seguridad", &format!("Capacitacion {} registrada a {}", rec.training_name, rec.username), &session.username)
        .await;
    if rec.username != session.username {
        let until = rec.expires_on.as_deref().map(|e| format!(" (vigente hasta el {})", e)).unwrap_or_default();
        tell(&state, vec![rec.username.clone()], Level::Info, "🎓", "Capacitacion registrada", &format!("{}{}", rec.training_name, until));
    }
    publish(&state);
    Ok(Json(rec))
}

/// DELETE /api/safety/records/{training_id}/{username} (Gestionar)
pub async fn revoke_record(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path((tid, username)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let rec = db_op_status(&state.db, move |conn| {
        let r = load_record(conn, &tid, &username).map_err(db_err)?.ok_or_else(|| not_found("Registro"))?;
        conn.execute("DELETE FROM safety_training_records WHERE training_id = ?1 AND username = ?2", params![tid, username])
            .map_err(db_err)?;
        Ok(r)
    })
    .await?;
    state
        .log_activity("Seguridad", &format!("Capacitacion {} quitada a {}", rec.training_name, rec.username), &session.username)
        .await;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

/// Capacitaciones con vencimiento (para Plazos): (capacitacion, nombre, persona, fecha)
pub(crate) fn expiring(conn: &Connection) -> rusqlite::Result<Vec<(String, String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT r.training_id, t.name, r.username, r.expires_on FROM safety_training_records r
         JOIN safety_trainings t ON t.id = r.training_id WHERE r.expires_on IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    rows.collect()
}

// ─── Fichas de seguridad ───

#[derive(Debug, Clone, Serialize)]
pub struct Sheet {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub content: String,
    pub url: String,
    pub machine_id: Option<String>,
    pub machine_name: Option<String>,
    pub inventory_item_id: Option<String>,
    pub inventory_item_name: Option<String>,
    pub file_name: Option<String>,
    pub file_mime: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

const SHEET_SELECT: &str = "SELECT s.id, s.title, s.kind, s.content, s.url, s.machine_id, m.name, s.inventory_item_id, i.name,
    f.name, f.mime, s.created_by, s.created_at, s.updated_at
    FROM safety_sheets s
    LEFT JOIN machines m ON m.id = s.machine_id
    LEFT JOIN inventory_items i ON i.id = s.inventory_item_id
    LEFT JOIN safety_sheet_files f ON f.sheet_id = s.id";

fn row_sheet(r: &rusqlite::Row) -> rusqlite::Result<Sheet> {
    Ok(Sheet {
        id: r.get(0)?,
        title: r.get(1)?,
        kind: r.get(2)?,
        content: r.get(3)?,
        url: r.get(4)?,
        machine_id: r.get(5)?,
        machine_name: r.get(6)?,
        inventory_item_id: r.get(7)?,
        inventory_item_name: r.get(8)?,
        file_name: r.get(9)?,
        file_mime: r.get(10)?,
        created_by: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
    })
}

fn load_sheet(conn: &Connection, id: &str) -> rusqlite::Result<Option<Sheet>> {
    conn.query_row(&format!("{} WHERE s.id = ?1", SHEET_SELECT), [id], row_sheet).optional()
}

/// GET /api/safety/sheets
pub async fn list_sheets(State(state): State<AppState>) -> Result<Json<Vec<Sheet>>, ApiError> {
    let list = db_op_status(&state.db, |conn| {
        let mut stmt = conn.prepare(&format!("{} ORDER BY s.title COLLATE NOCASE", SHEET_SELECT)).map_err(db_err)?;
        let rows = stmt.query_map([], row_sheet).map_err(db_err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
    })
    .await?;
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct SheetFile {
    pub name: String,
    /// base64, con o sin prefijo `data:...;base64,`
    pub data: String,
}

#[derive(Deserialize)]
pub struct SheetReq {
    pub title: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub machine_id: Option<String>,
    #[serde(default)]
    pub inventory_item_id: Option<String>,
    /// Archivo nuevo (al editar, si falta se conserva el actual)
    #[serde(default)]
    pub file: Option<SheetFile>,
    /// Al editar: quitar el archivo actual
    #[serde(default)]
    pub remove_file: bool,
}

struct SheetData {
    title: String,
    kind: String,
    content: String,
    url: String,
    machine: Option<String>,
    item: Option<String>,
    file: Option<(String, Vec<u8>, &'static str)>,
}

fn check_sheet(req: &SheetReq) -> Result<SheetData, ApiError> {
    let title = required(&req.title, 160, "El titulo")?;
    let kind = if req.kind.is_empty() { "material".to_string() } else { req.kind.clone() };
    if !SHEET_KINDS.contains(&kind.as_str()) {
        return Err(bad("Tipo de ficha invalido"));
    }
    let content = clean(&req.content, 20000, "El texto")?;
    let url = clean(&req.url, 500, "El enlace")?;
    if !url.is_empty() && !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with('/')) {
        return Err(bad("El enlace debe empezar con http://, https:// o ser una ruta del NAS (/...)"));
    }
    let file = match &req.file {
        Some(f) => {
            let (bytes, mime) = decode_blob(&f.data, MAX_FILE, true)?;
            Some((required(&f.name, 120, "El nombre del archivo")?, bytes, mime))
        }
        None => None,
    };
    Ok(SheetData { title, kind, content, url, machine: opt(&req.machine_id), item: opt(&req.inventory_item_id), file })
}

fn check_links(conn: &Connection, d: &SheetData) -> Result<(), ApiError> {
    if let Some(m) = &d.machine {
        conn.query_row("SELECT 1 FROM machines WHERE id = ?1", [m], |_| Ok(())).optional().map_err(db_err)?.ok_or_else(|| bad("Esa maquina no existe"))?;
    }
    if let Some(i) = &d.item {
        conn.query_row("SELECT 1 FROM inventory_items WHERE id = ?1", [i], |_| Ok(())).optional().map_err(db_err)?.ok_or_else(|| bad("Ese item de inventario no existe"))?;
    }
    Ok(())
}

fn put_file(conn: &Connection, sheet_id: &str, f: &(String, Vec<u8>, &'static str)) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO safety_sheet_files (sheet_id, name, mime, data) VALUES (?1, ?2, ?3, ?4)",
        params![sheet_id, f.0, f.2, f.1],
    )?;
    Ok(())
}

/// POST /api/safety/sheets (Gestionar)
pub async fn create_sheet(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<SheetReq>,
) -> Result<(StatusCode, Json<Sheet>), ApiError> {
    let d = check_sheet(&req)?;
    if d.content.is_empty() && d.url.is_empty() && d.file.is_none() {
        return Err(bad("Agrega texto, un enlace o un archivo"));
    }
    let (id, user) = (new_id(), session.username.clone());
    let s = db_op_status(&state.db, move |conn| {
        check_links(conn, &d)?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO safety_sheets (id, title, kind, content, url, machine_id, inventory_item_id, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![id, d.title, d.kind, d.content, d.url, d.machine, d.item, user, now],
        )
        .map_err(db_err)?;
        if let Some(f) = &d.file {
            put_file(conn, &id, f).map_err(db_err)?;
        }
        load_sheet(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Ficha"))
    })
    .await?;
    state.log_activity("Seguridad", &format!("Ficha creada: {}", s.title), &session.username).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(s)))
}

/// PUT /api/safety/sheets/{id} (Gestionar)
pub async fn update_sheet(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<SheetReq>,
) -> Result<Json<Sheet>, ApiError> {
    let d = check_sheet(&req)?;
    let remove = req.remove_file;
    let s = db_op_status(&state.db, move |conn| {
        let old = load_sheet(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Ficha"))?;
        check_links(conn, &d)?;
        let keeps_file = old.file_name.is_some() && !remove && d.file.is_none();
        if d.content.is_empty() && d.url.is_empty() && d.file.is_none() && !keeps_file {
            return Err(bad("Agrega texto, un enlace o un archivo"));
        }
        conn.execute(
            "UPDATE safety_sheets SET title = ?2, kind = ?3, content = ?4, url = ?5, machine_id = ?6, inventory_item_id = ?7, updated_at = ?8 WHERE id = ?1",
            params![id, d.title, d.kind, d.content, d.url, d.machine, d.item, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
        if let Some(f) = &d.file {
            put_file(conn, &id, f).map_err(db_err)?;
        } else if remove {
            conn.execute("DELETE FROM safety_sheet_files WHERE sheet_id = ?1", [&id]).map_err(db_err)?;
        }
        load_sheet(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Ficha"))
    })
    .await?;
    state.log_activity("Seguridad", &format!("Ficha editada: {}", s.title), &session.username).await;
    publish(&state);
    Ok(Json(s))
}

/// DELETE /api/safety/sheets/{id} (Gestionar)
pub async fn delete_sheet(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let s = db_op_status(&state.db, move |conn| {
        let s = load_sheet(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Ficha"))?;
        conn.execute("DELETE FROM safety_sheet_files WHERE sheet_id = ?1", [&id]).map_err(db_err)?;
        conn.execute("DELETE FROM safety_sheets WHERE id = ?1", [&id]).map_err(db_err)?;
        Ok(s)
    })
    .await?;
    state.log_activity("Seguridad", &format!("Ficha eliminada: {}", s.title), &session.username).await;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/safety/sheets/{id}/file
pub async fn get_sheet_file(State(state): State<AppState>, Path(id): Path<String>) -> Result<axum::response::Response, ApiError> {
    let (data, mime, name) = db_op_status(&state.db, move |conn| {
        conn.query_row("SELECT data, mime, name FROM safety_sheet_files WHERE sheet_id = ?1", [&id], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })
        .optional()
        .map_err(db_err)?
        .ok_or_else(|| not_found("Archivo"))
    })
    .await?;
    Ok(blob_response(data, mime, Some(name)))
}

// ─── Incidentes ───

#[derive(Debug, Clone, Serialize)]
pub struct Incident {
    pub id: String,
    pub reported_by: String,
    pub reported_at: String,
    pub occurred_at: String,
    pub place: String,
    pub machine_id: Option<String>,
    pub machine_name: Option<String>,
    /// casi_accidente | leve | grave
    pub severity: String,
    pub description: String,
    pub has_photo: bool,
    pub actions: String,
    /// abierto | en_revision | cerrado
    pub status: String,
    pub closed_by: Option<String>,
    pub closed_at: Option<String>,
    pub closing_note: String,
}

const INCIDENT_SELECT: &str = "SELECT i.id, i.reported_by, i.reported_at, i.occurred_at, i.place, i.machine_id, m.name, i.severity, i.description,
    EXISTS(SELECT 1 FROM safety_incident_photos p WHERE p.incident_id = i.id), i.actions, i.status, i.closed_by, i.closed_at, i.closing_note
    FROM safety_incidents i LEFT JOIN machines m ON m.id = i.machine_id";

fn row_incident(r: &rusqlite::Row) -> rusqlite::Result<Incident> {
    Ok(Incident {
        id: r.get(0)?,
        reported_by: r.get(1)?,
        reported_at: r.get(2)?,
        occurred_at: r.get(3)?,
        place: r.get(4)?,
        machine_id: r.get(5)?,
        machine_name: r.get(6)?,
        severity: r.get(7)?,
        description: r.get(8)?,
        has_photo: r.get::<_, i64>(9)? != 0,
        actions: r.get(10)?,
        status: r.get(11)?,
        closed_by: r.get(12)?,
        closed_at: r.get(13)?,
        closing_note: r.get(14)?,
    })
}

fn load_incident(conn: &Connection, id: &str) -> rusqlite::Result<Option<Incident>> {
    conn.query_row(&format!("{} WHERE i.id = ?1", INCIDENT_SELECT), [id], row_incident).optional()
}

fn severity_label(s: &str) -> &'static str {
    match s {
        "grave" => "grave",
        "leve" => "leve",
        _ => "casi accidente",
    }
}

#[derive(Deserialize)]
pub struct IncidentQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub machine_id: Option<String>,
}

/// GET /api/safety/incidents: quien gestiona ve todos; el resto, solo los que reporto
pub async fn list_incidents(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<IncidentQuery>,
) -> Result<Json<Vec<Incident>>, ApiError> {
    let staff = is_staff(&session);
    let me = session.username.clone();
    let list = db_op_status(&state.db, move |conn| {
        let mut stmt = conn
            .prepare(&format!("{} ORDER BY (i.status = 'cerrado'), i.occurred_at DESC, i.reported_at DESC", INCIDENT_SELECT))
            .map_err(db_err)?;
        let rows = stmt.query_map([], row_incident).map_err(db_err)?;
        let mut list = rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)?;
        list.retain(|i| {
            (staff || i.reported_by == me)
                && q.status.as_ref().is_none_or(|s| &i.status == s)
                && q.severity.as_ref().is_none_or(|s| &i.severity == s)
                && q.machine_id.as_ref().is_none_or(|m| i.machine_id.as_ref() == Some(m))
        });
        Ok(list)
    })
    .await?;
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct IncidentReq {
    pub severity: String,
    pub description: String,
    #[serde(default)]
    pub place: String,
    /// `AAAA-MM-DDTHH:MM`; por defecto, ahora
    #[serde(default)]
    pub occurred_at: Option<String>,
    #[serde(default)]
    pub machine_id: Option<String>,
    /// Foto en base64 (JPEG o PNG, hasta 2 MB)
    #[serde(default)]
    pub photo: Option<String>,
    /// Acciones tomadas en el momento
    #[serde(default)]
    pub actions: String,
}

/// POST /api/safety/incidents: cualquiera con Usar reporta
pub async fn report_incident(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<IncidentReq>,
) -> Result<(StatusCode, Json<Incident>), ApiError> {
    if !SEVERITIES.contains(&req.severity.as_str()) {
        return Err(bad("Gravedad invalida (casi_accidente, leve o grave)"));
    }
    let description = required(&req.description, 4000, "La descripcion")?;
    let place = clean(&req.place, 200, "El lugar")?;
    let actions = clean(&req.actions, 4000, "Las acciones")?;
    let now = Local::now().naive_local();
    let occurred = match opt(&req.occurred_at) {
        Some(s) => {
            let t = NaiveDateTime::parse_from_str(&s, TIME_FMT).map_err(|_| bad("Fecha u hora invalida (AAAA-MM-DDTHH:MM)"))?;
            if t > now + chrono::Duration::minutes(5) {
                return Err(bad("El incidente no puede ser en el futuro"));
            }
            t
        }
        None => now,
    }
    .format(TIME_FMT)
    .to_string();
    let photo = match req.photo.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => Some(decode_blob(p, MAX_PHOTO, false)?),
        None => None,
    };
    let machine = opt(&req.machine_id);
    let (id, user, severity) = (new_id(), session.username.clone(), req.severity.clone());
    let (inc, staff) = db_op_status(&state.db, move |conn| {
        if let Some(m) = &machine {
            conn.query_row("SELECT 1 FROM machines WHERE id = ?1", [m], |_| Ok(())).optional().map_err(db_err)?.ok_or_else(|| bad("Esa maquina no existe"))?;
        }
        let tx = conn.unchecked_transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO safety_incidents (id, reported_by, reported_at, occurred_at, place, machine_id, severity, description, actions, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'abierto')",
            params![id, user, chrono::Utc::now().to_rfc3339(), occurred, place, machine, severity, description, actions],
        )
        .map_err(db_err)?;
        if let Some((bytes, mime)) = &photo {
            tx.execute("INSERT INTO safety_incident_photos (incident_id, mime, data) VALUES (?1, ?2, ?3)", params![id, mime, bytes]).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        let inc = load_incident(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Incidente"))?;
        Ok((inc, staff_users(conn).map_err(db_err)?))
    })
    .await?;
    state
        .log_activity("Seguridad", &format!("Incidente {} reportado ({})", severity_label(&inc.severity), inc.id), &session.username)
        .await;
    if inc.severity == "grave" {
        let mut to = staff;
        to.retain(|u| u != &session.username);
        let place = if inc.place.is_empty() { String::new() } else { format!(" en {}", inc.place) };
        let machine = inc.machine_name.as_deref().map(|m| format!(" ({})", m)).unwrap_or_default();
        tell(
            &state,
            to,
            Level::Error,
            "🚨",
            "Incidente grave",
            &format!("{} reporta un incidente grave{}{}: {}", inc.reported_by, place, machine, inc.description),
        );
    }
    publish(&state);
    Ok((StatusCode::CREATED, Json(inc)))
}

#[derive(Deserialize)]
pub struct IncidentUpdate {
    pub status: String,
    #[serde(default)]
    pub actions: String,
    /// Obligatoria al cerrar
    #[serde(default)]
    pub closing_note: String,
}

/// PUT /api/safety/incidents/{id} (Gestionar): revisar, anotar acciones y cerrar con nota
pub async fn update_incident(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<IncidentUpdate>,
) -> Result<Json<Incident>, ApiError> {
    if !STATUSES.contains(&req.status.as_str()) {
        return Err(bad("Estado invalido (abierto, en_revision o cerrado)"));
    }
    let actions = clean(&req.actions, 4000, "Las acciones")?;
    let note = clean(&req.closing_note, 2000, "La nota de cierre")?;
    if req.status == "cerrado" && note.is_empty() {
        return Err(bad("Para cerrar un incidente escribe una nota de cierre"));
    }
    let (status, by) = (req.status.clone(), session.username.clone());
    let (inc, before) = db_op_status(&state.db, move |conn| {
        let before = load_incident(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Incidente"))?;
        let closing = status == "cerrado";
        let (closed_by, closed_at, note) = if closing {
            // Si ya estaba cerrado, se conserva quien y cuando lo cerro
            if before.status == "cerrado" {
                (before.closed_by.clone(), before.closed_at.clone(), note)
            } else {
                (Some(by), Some(chrono::Utc::now().to_rfc3339()), note)
            }
        } else {
            (None, None, String::new())
        };
        conn.execute(
            "UPDATE safety_incidents SET status = ?2, actions = ?3, closed_by = ?4, closed_at = ?5, closing_note = ?6 WHERE id = ?1",
            params![id, status, actions, closed_by, closed_at, note],
        )
        .map_err(db_err)?;
        let inc = load_incident(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Incidente"))?;
        Ok((inc, before))
    })
    .await?;
    state
        .log_activity("Seguridad", &format!("Incidente {}: {} -> {}", inc.id, before.status, inc.status), &session.username)
        .await;
    if before.status != inc.status && inc.reported_by != session.username {
        let label = match inc.status.as_str() {
            "cerrado" => "cerrado",
            "en_revision" => "en revision",
            _ => "reabierto",
        };
        tell(&state, vec![inc.reported_by.clone()], Level::Info, "🦺", "Tu reporte de seguridad cambio", &format!("Incidente {}", label));
    }
    publish(&state);
    Ok(Json(inc))
}

/// GET /api/safety/incidents/{id}/photo: quien gestiona o quien lo reporto
pub async fn get_incident_photo(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let staff = is_staff(&session);
    let me = session.username.clone();
    let (data, mime) = db_op_status(&state.db, move |conn| {
        let inc = load_incident(conn, &id).map_err(db_err)?.ok_or_else(|| not_found("Incidente"))?;
        if !staff && inc.reported_by != me {
            return Err(forbidden("Solo quien gestiona Seguridad o quien reporto ve esta foto"));
        }
        conn.query_row("SELECT data, mime FROM safety_incident_photos WHERE incident_id = ?1", [&id], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
        })
        .optional()
        .map_err(db_err)?
        .ok_or_else(|| not_found("Foto"))
    })
    .await?;
    Ok(blob_response(data, mime, None))
}

/// Resumen para el panel: incidentes sin cerrar y capacitaciones obligatorias que me faltan
#[derive(Serialize)]
pub struct Summary {
    pub open_incidents: i64,
    pub my_missing_mandatory: i64,
}

/// GET /api/safety/summary
pub async fn summary(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<Summary>, ApiError> {
    let staff = is_staff(&session);
    let me = session.username.clone();
    let s = db_op_status(&state.db, move |conn| {
        let open_incidents: i64 = if staff {
            conn.query_row("SELECT COUNT(*) FROM safety_incidents WHERE status != 'cerrado'", [], |r| r.get(0)).map_err(db_err)?
        } else {
            conn.query_row("SELECT COUNT(*) FROM safety_incidents WHERE status != 'cerrado' AND reported_by = ?1", [&me], |r| r.get(0)).map_err(db_err)?
        };
        let my_missing_mandatory: i64 = conn.query_row(
            "SELECT COUNT(*) FROM safety_trainings t WHERE t.mandatory = 1 AND NOT EXISTS (
                SELECT 1 FROM safety_training_records r WHERE r.training_id = t.id AND r.username = ?1
                AND (r.expires_on IS NULL OR r.expires_on >= date('now','localtime')))",
            [&me],
            |r| r.get(0),
        )
        .map_err(db_err)?;
        Ok(Summary { open_incidents, my_missing_mandatory })
    })
    .await?;
    Ok(Json(s))
}
