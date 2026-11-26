//! Solicitudes de informacion (idea 71): formulario publico con folio y codigo secreto, plazo
//! en dias habiles, responsable, estados y respuesta visible con folio + codigo. Cada cambio
//! de estado queda en la bitacora inalterable. Ver docs/TRANSPARENCIA.md.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Datelike, Local, NaiveDate, Weekday};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::db_op_status;
use crate::error::ApiError;
use crate::handlers::machines::{bad, conflict, db_err, forbidden};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

pub const STATUSES: &[&str] = &["recibida", "en_curso", "respondida", "derivada", "rechazada"];
const OPEN: &[&str] = &["recibida", "en_curso"];
/// Cuantas solicitudes abiertas se aceptan antes de pedir que alguien las atienda
const MAX_OPEN: i64 = 500;
/// Dias habiles que faltan para avisar antes del vencimiento
pub const SOON_DAYS: i64 = 3;

fn not_found() -> ApiError {
    (StatusCode::NOT_FOUND, "Solicitud no encontrada".into())
}

fn is_open(status: &str) -> bool {
    OPEN.contains(&status)
}

// ─── Dias habiles ───

fn is_business_day(d: NaiveDate) -> bool {
    !matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

/// Fecha en que vence un plazo de `n` dias habiles (lunes a viernes) contado desde el dia
/// siguiente a `start`. No considera feriados.
pub fn add_business_days(start: NaiveDate, n: i64) -> NaiveDate {
    let mut d = start;
    let mut left = n.max(0);
    while left > 0 {
        d += chrono::Duration::days(1);
        if is_business_day(d) {
            left -= 1;
        }
    }
    d
}

/// Dias habiles que faltan de `today` a `due` (0 = vence hoy; negativo = atrasada)
pub fn business_days_left(today: NaiveDate, due: NaiveDate) -> i64 {
    let (from, to, sign) = if due >= today { (today, due, 1) } else { (due, today, -1) };
    let mut n = 0;
    let mut d = from;
    while d < to {
        d += chrono::Duration::days(1);
        if is_business_day(d) {
            n += 1;
        }
    }
    sign * n
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

// ─── Limites contra abuso del formulario publico ───

/// Ventana deslizante simple (en memoria)
#[derive(Default)]
struct Window(VecDeque<Instant>);

impl Window {
    /// Registra un intento; false si ya hubo `max` dentro de `span`
    fn allow(&mut self, max: usize, span: Duration) -> bool {
        let now = Instant::now();
        while self.0.front().is_some_and(|t| now.duration_since(*t) > span) {
            self.0.pop_front();
        }
        if self.0.len() >= max {
            return false;
        }
        self.0.push_back(now);
        true
    }
}

struct Limits {
    /// Solicitudes nuevas: tope global (el servidor no ve la IP real detras del puente)
    create: Window,
    /// Consultas fallidas en total
    lookup_fail: Window,
    /// Consultas fallidas por folio: pasado el tope, ese folio se bloquea un rato
    per_folio: HashMap<String, Window>,
}

fn limits() -> &'static Mutex<Limits> {
    static L: OnceLock<Mutex<Limits>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(Limits { create: Window::default(), lookup_fail: Window::default(), per_folio: HashMap::new() }))
}

const CREATE_MAX: usize = 30;
const CREATE_SPAN: Duration = Duration::from_secs(3600);
const FAIL_MAX: usize = 5;
const FAIL_SPAN: Duration = Duration::from_secs(15 * 60);
const FAIL_TOTAL_MAX: usize = 100;

fn too_many() -> ApiError {
    (StatusCode::TOO_MANY_REQUESTS, "Demasiados intentos. Prueba de nuevo mas tarde.".into())
}

// ─── Folio y codigo ───

const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Codigo secreto de 10 caracteres sin letras ambiguas (50 bits)
fn new_code() -> String {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    bytes.iter().take(10).map(|b| CODE_ALPHABET[(*b as usize) % CODE_ALPHABET.len()] as char).collect()
}

/// Mayusculas y sin espacios ni guiones, para que se pueda escribir como se quiera
fn norm_code(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

fn hash_code(code: &str) -> String {
    crate::ledger::sha256_hex(norm_code(code).as_bytes())
}

fn clean(s: &str, max: usize, what: &str) -> Result<String, ApiError> {
    let s: String = s.trim().chars().filter(|c| !c.is_control() || *c == '\n').collect();
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

fn managers(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    crate::roles::users_with_level(conn, "transparencia", Level::Gestionar)
}

fn tell(state: &AppState, users: Vec<String>, text: String) {
    let st = state.clone();
    tokio::spawn(async move {
        crate::handlers::notifications::notify_users(&st, &users, &text).await;
    });
}

fn publish(state: &AppState) {
    state.events.publish("transparency", (), crate::events::Audience::All, Some("transparencia"));
}

// ─── Publico ───

#[derive(Deserialize)]
pub struct NewRequest {
    pub name: String,
    #[serde(default)]
    pub email: String,
    pub body: String,
}

/// POST /api/transparency/public/solicitudes (sin cuenta): devuelve folio y codigo secreto
pub async fn create_public(State(state): State<AppState>, Json(req): Json<NewRequest>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let name = required(&req.name, 100, "El nombre")?;
    let email = clean(&req.email, 120, "El correo")?;
    if !email.is_empty() && (!email.contains('@') || email.contains(char::is_whitespace)) {
        return Err(bad("Correo invalido"));
    }
    let body = required(&req.body, 4000, "La solicitud")?;
    if body.chars().count() < 10 {
        return Err(bad("Describe la solicitud con un poco mas de detalle"));
    }
    if !limits().lock().map(|mut l| l.create.allow(CREATE_MAX, CREATE_SPAN)).unwrap_or(false) {
        return Err(too_many());
    }
    let code = new_code();
    let (hash, id) = (hash_code(&code), crate::finance::new_id());
    let day = today();
    let (folio, due, managers_list) = db_op_status(&state.db, move |conn| {
        let open: i64 = conn
            .query_row("SELECT COUNT(*) FROM transp_requests WHERE status IN ('recibida','en_curso')", [], |r| r.get(0))
            .map_err(db_err)?;
        if open >= MAX_OPEN {
            return Err((StatusCode::SERVICE_UNAVAILABLE, "No se reciben mas solicitudes por ahora".into()));
        }
        let days = crate::handlers::transparency::portal::load_config(conn).deadline_days;
        let due = add_business_days(day, days).format("%Y-%m-%d").to_string();
        let year = day.format("%Y").to_string();
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate).map_err(db_err)?;
        let seq: i64 = tx
            .query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM transp_requests WHERE folio LIKE ?1", [format!("SI-{}-%", year)], |r| r.get(0))
            .map_err(db_err)?;
        let folio = format!("SI-{}-{:04}", year, seq);
        let now = crate::finance::now();
        tx.execute(
            "INSERT INTO transp_requests (id, seq, folio, code_hash, name, email, body, status, due_date, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'recibida', ?8, ?9, ?9)",
            params![id, seq, folio, hash, name, email, body, due, now],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO transp_request_events (id, request_id, at, username, status, note) VALUES (?1, ?2, ?3, 'publico', 'recibida', '')",
            params![crate::finance::new_id(), id, now],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok((folio, due, managers(conn).map_err(db_err)?))
    })
    .await?;
    // Sin datos personales en la bitacora: solo folio y plazo
    let seq = crate::finance::ledger(&state, "publico", "transparencia", &format!("solicitud:{}", folio), json!({ "estado": "recibida", "plazo": due })).await?;
    let f = folio.clone();
    db_op_status(&state.db, move |conn| {
        conn.execute("UPDATE transp_request_events SET ledger_seq = ?2 WHERE request_id = (SELECT id FROM transp_requests WHERE folio = ?1) AND status = 'recibida'", params![f, seq]).map_err(db_err)
    })
    .await?;
    tell(&state, managers_list, format!("Nueva solicitud de informacion {} (vence el {}). Revisala en Transparencia.", folio, due));
    state.log_activity("Transparencia", &format!("Solicitud de informacion recibida: {}", folio), "publico").await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(json!({ "folio": folio, "code": code, "due_date": due }))))
}

#[derive(Deserialize)]
pub struct Lookup {
    pub folio: String,
    pub code: String,
}

/// POST /api/transparency/public/solicitudes/consulta: estado y respuesta con folio + codigo.
/// Folio o codigo malos dan el mismo 404 (no se revela si el folio existe).
pub async fn lookup_public(State(state): State<AppState>, Json(req): Json<Lookup>) -> Result<Json<serde_json::Value>, ApiError> {
    let folio: String = req.folio.trim().to_ascii_uppercase().chars().take(24).collect();
    {
        let mut l = limits().lock().map_err(|_| too_many())?;
        if l.per_folio.len() > 2000 {
            l.per_folio.clear();
        }
        let blocked = l.per_folio.get_mut(&folio).is_some_and(|w| {
            // Solo mira: no suma ni libera hasta que pase la ventana
            let now = Instant::now();
            while w.0.front().is_some_and(|t| now.duration_since(*t) > FAIL_SPAN) {
                w.0.pop_front();
            }
            w.0.len() >= FAIL_MAX
        });
        if blocked {
            return Err(too_many());
        }
    }
    let hash = hash_code(&req.code);
    let f = folio.clone();
    let found = db_op_status(&state.db, move |conn| {
        let row = conn
            .query_row(
                "SELECT id, folio, body, status, due_date, response, reason, derived_to, created_at, responded_at, code_hash
                   FROM transp_requests WHERE folio = ?1",
                [&f],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?, r.get::<_, String>(7)?,
                        r.get::<_, String>(8)?, r.get::<_, Option<String>>(9)?, r.get::<_, String>(10)?,
                    ))
                },
            )
            .optional()
            .map_err(db_err)?;
        let Some((id, folio, body, status, due, response, reason, derived, created, responded, code_hash)) = row else { return Ok(None) };
        if code_hash != hash {
            return Ok(None);
        }
        let mut stmt = conn.prepare("SELECT at, status FROM transp_request_events WHERE request_id = ?1 ORDER BY at, rowid").map_err(db_err)?;
        let history: Vec<serde_json::Value> = stmt
            .query_map([&id], |r| Ok(json!({ "at": r.get::<_, String>(0)?, "status": r.get::<_, String>(1)? })))
            .map_err(db_err)?
            .collect::<rusqlite::Result<_>>()
            .map_err(db_err)?;
        let open = is_open(&status);
        let left = parse_date(&due).map(|d| business_days_left(today(), d));
        Ok(Some(json!({
            "folio": folio, "body": body, "status": status, "created_at": created, "due_date": due,
            "days_left": if open { left } else { None },
            "response": if open { String::new() } else { response },
            "reason": reason, "derived_to": derived, "responded_at": responded, "history": history,
        })))
    })
    .await?;
    match found {
        Some(v) => Ok(Json(v)),
        None => {
            if let Ok(mut l) = limits().lock() {
                let global_ok = l.lookup_fail.allow(FAIL_TOTAL_MAX, FAIL_SPAN);
                l.per_folio.entry(folio).or_default().allow(FAIL_MAX, FAIL_SPAN);
                if !global_ok {
                    return Err(too_many());
                }
            }
            Err(not_found())
        }
    }
}

// ─── Gestion ───

#[derive(Debug, Serialize)]
pub struct RequestView {
    pub id: String,
    pub folio: String,
    pub name: String,
    pub email: String,
    pub body: String,
    pub status: String,
    pub assigned_to: Option<String>,
    pub due_date: String,
    /// Dias habiles que faltan (solo abiertas); negativo = atrasada
    pub days_left: Option<i64>,
    pub response: String,
    pub reason: String,
    pub derived_to: String,
    pub created_at: String,
    pub responded_at: Option<String>,
}

const SELECT: &str = "SELECT id, folio, name, email, body, status, assigned_to, due_date, response, reason, derived_to, created_at, responded_at FROM transp_requests";

fn row_view(r: &rusqlite::Row) -> rusqlite::Result<RequestView> {
    let status: String = r.get(5)?;
    let due: String = r.get(7)?;
    let days_left = if is_open(&status) { parse_date(&due).map(|d| business_days_left(today(), d)) } else { None };
    Ok(RequestView {
        id: r.get(0)?, folio: r.get(1)?, name: r.get(2)?, email: r.get(3)?, body: r.get(4)?, status, assigned_to: r.get(6)?,
        due_date: due, days_left, response: r.get(8)?, reason: r.get(9)?, derived_to: r.get(10)?, created_at: r.get(11)?, responded_at: r.get(12)?,
    })
}

fn need_manage(s: &SessionInfo) -> Result<(), ApiError> {
    if s.can("transparencia", Level::Gestionar) {
        Ok(())
    } else {
        Err(forbidden("Solo quien gestiona Transparencia ve las solicitudes"))
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// GET /api/transparency/requests?status= (Gestionar): las abiertas primero, por vencimiento
pub async fn list(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Query(q): Query<ListQuery>) -> Result<Json<Vec<RequestView>>, ApiError> {
    need_manage(&session)?;
    let status = q.status.filter(|s| STATUSES.contains(&s.as_str()));
    let list = db_op_status(&state.db, move |conn| {
        let sql = format!(
            "{} WHERE (?1 IS NULL OR status = ?1) ORDER BY (status IN ('recibida','en_curso')) DESC, due_date ASC, created_at DESC LIMIT 500",
            SELECT
        );
        let mut stmt = conn.prepare(&sql).map_err(db_err)?;
        let rows = stmt.query_map([status], row_view).map_err(db_err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
    })
    .await?;
    Ok(Json(list))
}

/// GET /api/transparency/requests/{id} (Gestionar): con su historial
pub async fn detail(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    need_manage(&session)?;
    let v = db_op_status(&state.db, move |conn| {
        let r = conn.query_row(&format!("{} WHERE id = ?1", SELECT), [&id], row_view).optional().map_err(db_err)?.ok_or_else(not_found)?;
        let mut stmt = conn
            .prepare("SELECT at, username, status, note, ledger_seq FROM transp_request_events WHERE request_id = ?1 ORDER BY at, rowid")
            .map_err(db_err)?;
        let events: Vec<serde_json::Value> = stmt
            .query_map([&id], |e| {
                Ok(json!({ "at": e.get::<_, String>(0)?, "username": e.get::<_, String>(1)?, "status": e.get::<_, String>(2)?, "note": e.get::<_, String>(3)?, "ledger_seq": e.get::<_, Option<i64>>(4)? }))
            })
            .map_err(db_err)?
            .collect::<rusqlite::Result<_>>()
            .map_err(db_err)?;
        Ok(json!({ "request": r, "events": events }))
    })
    .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct UpdateRequest {
    #[serde(default)]
    pub status: Option<String>,
    /// Responsable ("" lo quita)
    #[serde(default)]
    pub assigned_to: Option<String>,
    #[serde(default)]
    pub response: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub derived_to: Option<String>,
    /// Nota interna (no se publica)
    #[serde(default)]
    pub note: Option<String>,
}

/// PUT /api/transparency/requests/{id} (Gestionar): asigna responsable y cambia el estado
pub async fn update(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<UpdateRequest>,
) -> Result<Json<RequestView>, ApiError> {
    need_manage(&session)?;
    let new_status = req.status.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    if let Some(s) = &new_status {
        if !STATUSES.contains(&s.as_str()) {
            return Err(bad("Estado invalido"));
        }
    }
    let response = req.response.as_deref().map(|s| clean(s, 8000, "La respuesta")).transpose()?;
    let reason = req.reason.as_deref().map(|s| clean(s, 1000, "El motivo")).transpose()?;
    let derived = req.derived_to.as_deref().map(|s| clean(s, 200, "El destino")).transpose()?;
    let note = clean(req.note.as_deref().unwrap_or(""), 1000, "La nota")?;
    let assign = req.assigned_to.as_deref().map(str::trim).map(String::from);
    let me = session.username.clone();

    let (before, after, assignee_changed) = db_op_status(&state.db, move |conn| {
        let before = conn.query_row(&format!("{} WHERE id = ?1", SELECT), [&id], row_view).optional().map_err(db_err)?.ok_or_else(not_found)?;
        if !is_open(&before.status) {
            return Err(conflict("La solicitud ya esta cerrada: respondida, derivada o rechazada"));
        }
        // Responsable: alguien que gestiona Transparencia
        let mut assigned = before.assigned_to.clone();
        if let Some(a) = &assign {
            if a.is_empty() {
                assigned = None;
            } else {
                if !managers(conn).map_err(db_err)?.contains(a) {
                    return Err(bad("El responsable debe gestionar Transparencia"));
                }
                assigned = Some(a.clone());
            }
        }
        let status = new_status.clone().unwrap_or_else(|| before.status.clone());
        let (mut resp, mut why, mut der) = (before.response.clone(), before.reason.clone(), before.derived_to.clone());
        if let Some(r) = response {
            resp = r;
        }
        if let Some(r) = reason {
            why = r;
        }
        if let Some(d) = derived {
            der = d;
        }
        match status.as_str() {
            "respondida" if resp.is_empty() => return Err(bad("Escribe la respuesta para marcarla como respondida")),
            "rechazada" if why.is_empty() => return Err(bad("Indica el motivo del rechazo")),
            "derivada" if der.is_empty() => return Err(bad("Indica a quien se derivo")),
            _ => {}
        }
        let closing = !is_open(&status);
        let now = crate::finance::now();
        conn.execute(
            "UPDATE transp_requests SET status = ?2, assigned_to = ?3, response = ?4, reason = ?5, derived_to = ?6,
                 responded_at = CASE WHEN ?7 THEN ?8 ELSE responded_at END, updated_at = ?8 WHERE id = ?1",
            params![id, status, assigned, resp, why, der, closing, now],
        )
        .map_err(db_err)?;
        let changed_status = status != before.status;
        let changed_assignee = assigned != before.assigned_to;
        if changed_status || changed_assignee || !note.is_empty() || closing {
            conn.execute(
                "INSERT INTO transp_request_events (id, request_id, at, username, status, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![crate::finance::new_id(), id, now, me, status, note],
            )
            .map_err(db_err)?;
        }
        let after = conn.query_row(&format!("{} WHERE id = ?1", SELECT), [&id], row_view).map_err(db_err)?;
        Ok((before, after, changed_assignee))
    })
    .await?;

    let status_changed = before.status != after.status;
    if status_changed || assignee_changed {
        let seq = crate::finance::ledger(
            &state,
            &session.username,
            "transparencia",
            &format!("solicitud:{}", after.folio),
            json!({ "de": before.status, "a": after.status, "responsable": after.assigned_to, "motivo": after.reason, "derivada_a": after.derived_to }),
        )
        .await?;
        let rid = after.id.clone();
        db_op_status(&state.db, move |conn| {
            conn.execute(
                "UPDATE transp_request_events SET ledger_seq = ?2 WHERE rowid = (SELECT MAX(rowid) FROM transp_request_events WHERE request_id = ?1)",
                params![rid, seq],
            )
            .map_err(db_err)
        })
        .await?;
        state
            .log_activity("Transparencia", &format!("Solicitud {}: {} -> {}", after.folio, before.status, after.status), &session.username)
            .await;
    }
    if assignee_changed {
        if let Some(a) = after.assigned_to.clone().filter(|a| *a != session.username) {
            tell(&state, vec![a], format!("Te asignaron la solicitud de informacion {} (vence el {}).", after.folio, after.due_date));
        }
    }
    publish(&state);
    Ok(Json(after))
}

// ─── Avisos de plazo ───

/// Avisa de las solicitudes a ≤ 3 dias habiles de vencer y de las vencidas, una vez por
/// umbral. Devuelve cuantos avisos mando.
pub async fn check_deadlines(state: &AppState) -> usize {
    let day = today();
    let found = db_op_status(&state.db, move |conn| {
        let mut stmt = conn
            .prepare("SELECT id, folio, due_date, assigned_to, alerted_soon, alerted_due FROM transp_requests WHERE status IN ('recibida','en_curso')")
            .map_err(db_err)?;
        let rows: Vec<(String, String, String, Option<String>, bool, bool)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)? != 0, r.get::<_, i64>(5)? != 0)))
            .map_err(db_err)?
            .collect::<rusqlite::Result<_>>()
            .map_err(db_err)?;
        let staff = managers(conn).map_err(db_err)?;
        let mut notices: Vec<(Vec<String>, String)> = Vec::new();
        for (id, folio, due, assigned, soon, over) in rows {
            let Some(d) = parse_date(&due) else { continue };
            let left = business_days_left(day, d);
            let mut to: Vec<String> = staff.clone();
            if let Some(a) = assigned.filter(|a| !to.contains(a)) {
                to.push(a);
            }
            if left < 0 && !over {
                conn.execute("UPDATE transp_requests SET alerted_due = 1, alerted_soon = 1 WHERE id = ?1", [&id]).map_err(db_err)?;
                notices.push((to, format!("La solicitud de informacion {} esta VENCIDA (plazo: {}).", folio, due)));
            } else if (0..=SOON_DAYS).contains(&left) && !soon {
                conn.execute("UPDATE transp_requests SET alerted_soon = 1 WHERE id = ?1", [&id]).map_err(db_err)?;
                let txt = if left == 0 { "vence HOY".to_string() } else { format!("vence en {} dias habiles", left) };
                notices.push((to, format!("La solicitud de informacion {} {} (plazo: {}).", folio, txt, due)));
            }
        }
        Ok(notices)
    })
    .await
    .unwrap_or_default();
    let n = found.len();
    for (users, text) in found {
        crate::handlers::notifications::notify_users(state, &users, &text).await;
    }
    n
}

/// Tarea de fondo del modulo: revisa los plazos cada media hora
pub async fn alert_loop(state: AppState) {
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        check_deadlines(&state).await;
        tokio::time::sleep(Duration::from_secs(1800)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn plazo_en_dias_habiles() {
        // 2026-11-26 es jueves: 1 dia habil = viernes; 2 = lunes (se salta el fin de semana)
        assert_eq!(d("2026-11-26").weekday(), Weekday::Thu);
        assert_eq!(add_business_days(d("2026-11-26"), 1), d("2026-11-27"));
        assert_eq!(add_business_days(d("2026-11-26"), 2), d("2026-11-30"));
        assert_eq!(add_business_days(d("2026-11-26"), 20), d("2026-12-24"));
        // Desde un sabado cuenta desde el lunes
        assert_eq!(add_business_days(d("2026-11-28"), 1), d("2026-11-30"));
        assert_eq!(add_business_days(d("2026-11-26"), 0), d("2026-11-26"));
    }

    #[test]
    fn dias_habiles_que_faltan() {
        assert_eq!(business_days_left(d("2026-11-26"), d("2026-11-26")), 0);
        assert_eq!(business_days_left(d("2026-11-26"), d("2026-11-30")), 2);
        assert_eq!(business_days_left(d("2026-11-30"), d("2026-11-26")), -2);
        assert_eq!(business_days_left(d("2026-11-27"), d("2026-12-01")), 2);
        // Ida y vuelta coherente con el plazo
        let due = add_business_days(d("2026-11-26"), 20);
        assert_eq!(business_days_left(d("2026-11-26"), due), 20);
    }

    #[test]
    fn el_limitador_corta_y_libera() {
        let mut w = Window::default();
        assert!(w.allow(2, Duration::from_secs(60)));
        assert!(w.allow(2, Duration::from_secs(60)));
        assert!(!w.allow(2, Duration::from_secs(60)));
        assert!(w.allow(2, Duration::from_millis(0)), "fuera de la ventana vuelve a dejar pasar");
    }

    #[test]
    fn codigo_sin_ambiguos_y_normalizado() {
        let c = new_code();
        assert_eq!(c.len(), 10);
        assert!(c.chars().all(|ch| CODE_ALPHABET.contains(&(ch as u8))));
        assert_eq!(hash_code("ab-cd 12"), hash_code("ABCD12"));
    }
}
