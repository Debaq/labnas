//! Plazos con cuenta regresiva: cierre de proyectos, informes, rendiciones, garantias,
//! convocatorias, tesis, licencias... Hay contadores sueltos (tabla `deadlines`) y otros que
//! salen solos del Portafolio (fecha de termino y hitos sin cumplir de los proyectos propios
//! en curso). Avisa por Telegram a los 30, 7 y 1 dia, y el mismo dia. Ver docs/PLAZOS.md.
//!
//! Las fechas son `YYYY-MM-DD` en hora local.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Local, NaiveDate, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::events::{Audience, Level};
use crate::models::portfolio::Milestone;
use crate::state::{AppState, SessionInfo};
use crate::error::ApiError;

const MODULE: Option<&str> = Some("deadlines");
pub const CATEGORIES: &[&str] = &[
    "proyecto", "informe", "rendicion", "garantia", "convocatoria", "tesis", "licencia", "otro",
];
const NOTIFY: &[&str] = &["creador", "staff", "todos"];
/// Dias antes del plazo en que se avisa por Telegram (0 = el mismo dia)
pub const THRESHOLDS: &[i64] = &[30, 7, 1, 0];
/// Los avisos salen solo en este horario (no despertar a nadie)
const ALERT_HOURS: std::ops::Range<u32> = 9..21;

fn bad(msg: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.into())
}
fn not_found() -> ApiError {
    (StatusCode::NOT_FOUND, "Plazo no encontrado".into())
}
fn db_err(e: rusqlite::Error) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn is_staff(s: &SessionInfo) -> bool {
    s.can("deadlines", crate::roles::Level::Gestionar)
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    // El Portafolio puede traer fechas con hora; basta la parte de la fecha
    NaiveDate::parse_from_str(s.trim().get(..10)?, "%Y-%m-%d").ok()
}

/// Verde > 30 dias, amarillo <= 30, rojo <= 7, vencido < 0
pub fn level(days_left: i64) -> &'static str {
    match days_left {
        d if d < 0 => "vencido",
        d if d <= 7 => "urgente",
        d if d <= 30 => "pronto",
        _ => "ok",
    }
}

/// Texto de usuario dentro de un mensaje Markdown de Telegram
fn md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '_' | '*' | '`' | '[') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn publish(state: &AppState) {
    state.events.publish("deadlines", (), Audience::All, MODULE);
}

// ─── Modelo ───

#[derive(Debug, Clone, Serialize)]
pub struct Deadline {
    /// Unico entre todas las fuentes: `d:<id>`, `p:<proyecto>` o `m:<proyecto>:<hito>`
    pub key: String,
    /// manual | proyecto | hito | induccion | mantencion | vencimiento | capacitacion | arqueo
    pub source: &'static str,
    /// Id del contador suelto (solo `manual`)
    pub id: Option<String>,
    pub title: String,
    /// Proyecto al que pertenece (hitos y contadores vinculados)
    pub project: Option<String>,
    pub portfolio_id: Option<String>,
    pub category: String,
    pub due_date: String,
    pub days_left: i64,
    pub level: &'static str,
    pub notes: String,
    pub notify: String,
    pub created_by: Option<String>,
    pub done_at: Option<String>,
    pub done_by: Option<String>,
    /// Participantes del proyecto (para los avisos); no se envia al navegador
    #[serde(skip)]
    pub participants: Vec<String>,
}

fn finish(mut d: Deadline, today: NaiveDate) -> Option<Deadline> {
    let due = parse_date(&d.due_date)?;
    d.due_date = due.format("%Y-%m-%d").to_string();
    d.days_left = (due - today).num_days();
    d.level = if d.done_at.is_some() { "cumplido" } else { level(d.days_left) };
    Some(d)
}

const SELECT: &str = "SELECT d.id, d.title, d.category, d.due_date, d.notes, d.notify, d.portfolio_id, d.done_at, d.done_by, d.created_by, p.title
    FROM deadlines d LEFT JOIN portfolio_entries p ON p.id = d.portfolio_id";

fn row_to_manual(row: &rusqlite::Row) -> rusqlite::Result<Deadline> {
    let id: String = row.get(0)?;
    Ok(Deadline {
        key: format!("d:{}", id),
        source: "manual",
        id: Some(id),
        title: row.get(1)?,
        category: row.get(2)?,
        due_date: row.get(3)?,
        notes: row.get(4)?,
        notify: row.get(5)?,
        portfolio_id: row.get(6)?,
        done_at: row.get(7)?,
        done_by: row.get(8)?,
        created_by: Some(row.get(9)?),
        project: row.get(10)?,
        days_left: 0,
        level: "ok",
        participants: Vec::new(),
    })
}

fn module_on(conn: &Connection, id: &str) -> bool {
    conn.query_row("SELECT enabled FROM modules WHERE id = ?1", [id], |r| r.get::<_, i64>(0))
        .map(|v| v != 0)
        .unwrap_or(false)
}

/// Fecha de termino e hitos pendientes de los proyectos propios que siguen en curso
fn portfolio_deadlines(conn: &Connection, today: NaiveDate) -> rusqlite::Result<Vec<Deadline>> {
    if !module_on(conn, "portfolio") {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, title, end_date, milestones, participants FROM portfolio_entries
         WHERE scope = 'own' AND status IN ('planned', 'active', 'submitted')",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, title, end_date, milestones, participants) = row?;
        let participants: Vec<String> = serde_json::from_str(&participants).unwrap_or_default();
        let base = |key: String, source, t: String, project: Option<String>, due: String| Deadline {
            key,
            source,
            id: None,
            title: t,
            project,
            portfolio_id: Some(id.clone()),
            category: if source == "proyecto" { "proyecto".into() } else { "hito".into() },
            due_date: due,
            days_left: 0,
            level: "ok",
            notes: String::new(),
            notify: "staff".into(),
            created_by: None,
            done_at: None,
            done_by: None,
            participants: participants.clone(),
        };
        if let Some(end) = end_date.filter(|e| !e.trim().is_empty()) {
            out.extend(finish(base(format!("p:{}", id), "proyecto", format!("Cierre de {}", title), None, end), today));
        }
        let milestones: Vec<Milestone> = serde_json::from_str(&milestones).unwrap_or_default();
        for m in milestones.into_iter().filter(|m| !m.completed && !m.date.trim().is_empty()) {
            out.extend(finish(base(format!("m:{}:{}", id, m.id), "hito", m.title, Some(title.clone()), m.date), today));
        }
    }
    Ok(out)
}

/// Vencimientos de inducciones de maquinas (hasta 30 dias despues de vencer)
fn certification_deadlines(conn: &Connection, today: NaiveDate) -> rusqlite::Result<Vec<Deadline>> {
    if !module_on(conn, "machines") {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT c.machine_id, c.username, c.expires_on, m.name FROM machine_certifications c
         JOIN machines m ON m.id = c.machine_id
         WHERE c.status = 'vigente' AND c.expires_on IS NOT NULL AND m.status != 'retirada'",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (machine, user, expires, name) = row?;
        let d = Deadline {
            key: format!("c:{}:{}", machine, user),
            source: "induccion",
            id: None,
            title: format!("Inducción de {} en {}", user, name),
            project: None,
            portfolio_id: None,
            category: "licencia".into(),
            due_date: expires,
            days_left: 0,
            level: "ok",
            notes: String::new(),
            notify: "staff".into(),
            created_by: None,
            done_at: None,
            done_by: None,
            participants: vec![user],
        };
        out.extend(finish(d, today).filter(|d| d.days_left >= -30));
    }
    Ok(out)
}

/// Mantenciones y calibraciones por fecha de las maquinas (hasta 30 dias pasadas: despues
/// siguen vencidas en la ficha de la maquina)
fn maintenance_deadlines(conn: &Connection, today: NaiveDate) -> rusqlite::Result<Vec<Deadline>> {
    if !module_on(conn, "machines") {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for (t, responsible) in crate::handlers::machine_maintenance::dated_tasks(conn)? {
        let Some(due) = t.due_date.clone() else { continue };
        let what = if t.kind == "calibracion" { "Calibracion" } else { "Mantencion" };
        let d = Deadline {
            key: format!("mt:{}:{}", t.id, due),
            source: "mantencion",
            id: None,
            title: format!("{}: {} ({})", what, t.title, t.machine_name),
            project: None,
            portfolio_id: None,
            category: "otro".into(),
            due_date: due,
            days_left: 0,
            level: "ok",
            notes: String::new(),
            notify: "staff".into(),
            created_by: None,
            done_at: None,
            done_by: None,
            participants: responsible.into_iter().collect(),
        };
        out.extend(finish(d, today).filter(|d| d.days_left >= -30));
    }
    Ok(out)
}

/// Plazos sin cumplir que vencen dentro de `days` dias (y los vencidos), para la pantalla
pub(crate) fn upcoming(conn: &Connection, days: i64) -> rusqlite::Result<Vec<Deadline>> {
    Ok(active_deadlines(conn, today())?.into_iter().filter(|d| d.days_left <= days).collect())
}

/// Vencimientos de items del Inventario (resinas, reactivos, baterias), hasta 30 dias pasados
fn inventory_deadlines(conn: &Connection, today: NaiveDate) -> rusqlite::Result<Vec<Deadline>> {
    if !module_on(conn, "inventory") {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for (id, name, expires) in crate::handlers::inventory_loans::expiring(conn)? {
        let d = Deadline {
            key: format!("inv:{}:{}", id, expires),
            source: "vencimiento",
            id: None,
            title: format!("Vence: {}", name),
            project: None,
            portfolio_id: None,
            category: "otro".into(),
            due_date: expires,
            days_left: 0,
            level: "ok",
            notes: String::new(),
            notify: "staff".into(),
            created_by: None,
            done_at: None,
            done_by: None,
            participants: Vec::new(),
        };
        out.extend(finish(d, today).filter(|d| d.days_left >= -30));
    }
    Ok(out)
}

/// Vencimientos de capacitaciones de seguridad (hasta 30 dias pasados)
fn safety_deadlines(conn: &Connection, today: NaiveDate) -> rusqlite::Result<Vec<Deadline>> {
    if !module_on(conn, "seguridad") {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for (tid, name, user, expires) in crate::handlers::safety::expiring(conn)? {
        let d = Deadline {
            key: format!("sg:{}:{}", tid, user),
            source: "capacitacion",
            id: None,
            title: format!("Capacitación {} de {}", name, user),
            project: None,
            portfolio_id: None,
            category: "licencia".into(),
            due_date: expires,
            days_left: 0,
            level: "ok",
            notes: String::new(),
            notify: "staff".into(),
            created_by: None,
            done_at: None,
            done_by: None,
            participants: vec![user],
        };
        out.extend(finish(d, today).filter(|d| d.days_left >= -30));
    }
    Ok(out)
}

/// Arqueos de caja chica pendientes (Finanzas): fecha del ultimo arqueo mas su frecuencia. Los
/// vencidos siguen apareciendo hasta que se haga el arqueo o se cierre el proyecto. El cierre
/// del proyecto ya sale por el Portafolio (fecha de termino).
fn petty_cash_deadlines(conn: &Connection, today: NaiveDate) -> rusqlite::Result<Vec<Deadline>> {
    if !module_on(conn, "finanzas") {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for a in crate::handlers::finance::budget::pending_arqueos(conn)? {
        let d = Deadline {
            key: format!("ar:{}", a.fund_id),
            source: "arqueo",
            id: None,
            title: format!("Arqueo de caja chica: {}", a.title),
            project: Some(a.title),
            portfolio_id: None,
            category: "rendicion".into(),
            due_date: a.due,
            days_left: 0,
            level: "ok",
            notes: String::new(),
            notify: "staff".into(),
            created_by: None,
            done_at: None,
            done_by: None,
            participants: vec![a.responsible],
        };
        out.extend(finish(d, today));
    }
    Ok(out)
}

/// Todos los plazos activos (sin los cumplidos), ordenados por fecha
fn active_deadlines(conn: &Connection, today: NaiveDate) -> rusqlite::Result<Vec<Deadline>> {
    let mut stmt = conn.prepare(&format!("{} WHERE d.done_at IS NULL", SELECT))?;
    let mut list: Vec<Deadline> = stmt
        .query_map([], row_to_manual)?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|d| finish(d, today))
        .collect();
    list.extend(portfolio_deadlines(conn, today)?);
    list.extend(certification_deadlines(conn, today)?);
    list.extend(maintenance_deadlines(conn, today)?);
    list.extend(inventory_deadlines(conn, today)?);
    list.extend(safety_deadlines(conn, today)?);
    list.extend(petty_cash_deadlines(conn, today)?);
    list.sort_by(|a, b| a.due_date.cmp(&b.due_date).then_with(|| a.title.cmp(&b.title)));
    Ok(list)
}

fn load_manual(conn: &Connection, id: &str) -> rusqlite::Result<Option<Deadline>> {
    conn.query_row(&format!("{} WHERE d.id = ?1", SELECT), [id], row_to_manual)
        .optional()
        .map(|d| d.and_then(|d| finish(d, today())))
}

// ─── Handlers ───

#[derive(Deserialize)]
pub struct ListQuery {
    /// Ver los contadores cumplidos (historial) en vez de los activos
    #[serde(default)]
    pub done: bool,
}

/// GET /api/deadlines
pub async fn list_deadlines(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Deadline>>, ApiError> {
    let list = db_op_status(&state.db, move |conn| {
        let t = today();
        if q.done {
            let mut stmt = conn
                .prepare(&format!("{} WHERE d.done_at IS NOT NULL ORDER BY d.done_at DESC LIMIT 100", SELECT))
                .map_err(db_err)?;
            let list = stmt
                .query_map([], row_to_manual)
                .map_err(db_err)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(db_err)?;
            Ok(list.into_iter().filter_map(|d| finish(d, t)).collect())
        } else {
            active_deadlines(conn, t).map_err(db_err)
        }
    })
    .await?;
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct DeadlineReq {
    pub title: String,
    #[serde(default)]
    pub category: String,
    pub due_date: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub notify: String,
    #[serde(default)]
    pub portfolio_id: Option<String>,
}

fn validate(req: &mut DeadlineReq) -> Result<(), ApiError> {
    req.title = req.title.trim().to_string();
    if req.title.is_empty() || req.title.chars().count() > 120 {
        return Err(bad("El nombre es obligatorio (maximo 120 caracteres)"));
    }
    if req.category.is_empty() {
        req.category = "otro".into();
    }
    if !CATEGORIES.contains(&req.category.as_str()) {
        return Err(bad("Categoria desconocida"));
    }
    if req.notify.is_empty() {
        req.notify = "creador".into();
    }
    if !NOTIFY.contains(&req.notify.as_str()) {
        return Err(bad("Destinatarios de los avisos desconocidos"));
    }
    let due = NaiveDate::parse_from_str(req.due_date.trim(), "%Y-%m-%d").map_err(|_| bad("Fecha invalida (formato AAAA-MM-DD)"))?;
    req.due_date = due.format("%Y-%m-%d").to_string();
    req.notes = req.notes.trim().to_string();
    if req.notes.chars().count() > 2000 {
        return Err(bad("Las notas son demasiado largas"));
    }
    req.portfolio_id = req.portfolio_id.take().map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
    Ok(())
}

fn check_portfolio(conn: &Connection, id: &Option<String>) -> Result<(), ApiError> {
    if let Some(id) = id {
        let exists: bool = conn
            .query_row("SELECT 1 FROM portfolio_entries WHERE id = ?1", [id], |_| Ok(true))
            .optional()
            .map_err(db_err)?
            .unwrap_or(false);
        if !exists {
            return Err(bad("El proyecto vinculado no existe"));
        }
    }
    Ok(())
}

/// El creador o el staff pueden cambiar un contador
fn can_edit(conn: &Connection, id: &str, s: &SessionInfo) -> Result<Deadline, ApiError> {
    let d = load_manual(conn, id).map_err(db_err)?.ok_or_else(not_found)?;
    if !is_staff(s) && d.created_by.as_deref() != Some(s.username.as_str()) {
        return Err((StatusCode::FORBIDDEN, "Solo quien lo creo o un operador puede cambiarlo".into()));
    }
    Ok(d)
}

/// POST /api/deadlines
pub async fn create_deadline(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(mut req): Json<DeadlineReq>,
) -> Result<(StatusCode, Json<Deadline>), ApiError> {
    validate(&mut req)?;
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let user = session.username.clone();
    let d = db_op_status(&state.db, move |conn| {
        check_portfolio(conn, &req.portfolio_id)?;
        conn.execute(
            "INSERT INTO deadlines (id, title, category, due_date, notes, notify, portfolio_id, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, req.title, req.category, req.due_date, req.notes, req.notify, req.portfolio_id, user, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
        load_manual(conn, &id).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    state.log_activity("Plazos", &format!("Nuevo plazo: {} ({})", d.title, d.due_date), &session.username).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(d)))
}

/// PUT /api/deadlines/{id}
pub async fn update_deadline(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(mut req): Json<DeadlineReq>,
) -> Result<Json<Deadline>, ApiError> {
    validate(&mut req)?;
    let s = session.clone();
    let d = db_op_status(&state.db, move |conn| {
        can_edit(conn, &id, &s)?;
        check_portfolio(conn, &req.portfolio_id)?;
        conn.execute(
            "UPDATE deadlines SET title = ?2, category = ?3, due_date = ?4, notes = ?5, notify = ?6, portfolio_id = ?7 WHERE id = ?1",
            params![id, req.title, req.category, req.due_date, req.notes, req.notify, req.portfolio_id],
        )
        .map_err(db_err)?;
        load_manual(conn, &id).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    state.log_activity("Plazos", &format!("Plazo editado: {} ({})", d.title, d.due_date), &session.username).await;
    publish(&state);
    Ok(Json(d))
}

#[derive(Deserialize)]
pub struct DoneReq {
    pub done: bool,
}

/// POST /api/deadlines/{id}/done: marcar cumplido (o reabrir)
pub async fn set_done(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<DoneReq>,
) -> Result<Json<Deadline>, ApiError> {
    let s = session.clone();
    let d = db_op_status(&state.db, move |conn| {
        can_edit(conn, &id, &s)?;
        if req.done {
            conn.execute(
                "UPDATE deadlines SET done_at = ?2, done_by = ?3 WHERE id = ?1",
                params![id, chrono::Utc::now().to_rfc3339(), s.username],
            )
        } else {
            conn.execute("UPDATE deadlines SET done_at = NULL, done_by = NULL WHERE id = ?1", [&id])
        }
        .map_err(db_err)?;
        load_manual(conn, &id).map_err(db_err)?.ok_or_else(not_found)
    })
    .await?;
    let what = if req.done { "Plazo cumplido" } else { "Plazo reabierto" };
    state.log_activity("Plazos", &format!("{}: {}", what, d.title), &session.username).await;
    publish(&state);
    Ok(Json(d))
}

/// DELETE /api/deadlines/{id}
pub async fn delete_deadline(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let s = session.clone();
    let d = db_op_status(&state.db, move |conn| {
        let d = can_edit(conn, &id, &s)?;
        conn.execute("DELETE FROM deadlines WHERE id = ?1", [&id]).map_err(db_err)?;
        conn.execute("DELETE FROM deadline_alerts WHERE key = ?1", [format!("d:{}", id)]).map_err(db_err)?;
        crate::handlers::access::forget(conn, "plazo", &id);
        Ok(d)
    })
    .await?;
    state.log_activity("Plazos", &format!("Plazo eliminado: {}", d.title), &session.username).await;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

// ─── Avisos ───

/// Umbral que toca avisar hoy: el menor de los cruzados que aun no se aviso. Al llegar
/// tarde (un plazo creado a 20 dias) se avisa una vez y se dan por cubiertos los mayores.
pub fn due_threshold(days_left: i64, sent: &[i64]) -> Option<i64> {
    if days_left < 0 {
        return None;
    }
    let t = THRESHOLDS.iter().copied().filter(|t| days_left <= *t).min()?;
    (!sent.contains(&t)).then_some(t)
}

fn countdown_text(days: i64) -> String {
    match days {
        0 => "vence hoy".into(),
        1 => "vence mañana".into(),
        d => format!("faltan {} días", d),
    }
}

/// Quienes reciben el aviso: `None` = todos los chats aprobados
fn recipients(conn: &Connection, d: &Deadline) -> rusqlite::Result<Option<Vec<String>>> {
    // Las mantenciones de maquinas van a quienes gestionan Maquinas
    let module = match d.source {
        "mantencion" => "machines",
        "vencimiento" => "inventory",
        "capacitacion" => "seguridad",
        "arqueo" => "finanzas",
        _ => "deadlines",
    };
    let staff = || -> rusqlite::Result<Vec<String>> {
        crate::roles::users_with_level(conn, module, crate::roles::Level::Gestionar)
    };
    let mut users: Vec<String> = match (d.source, d.notify.as_str()) {
        ("manual", "todos") => return Ok(None),
        ("manual", "staff") => staff()?,
        ("manual", _) => Vec::new(),
        _ => {
            let mut v = staff()?;
            v.extend(d.participants.iter().cloned());
            v
        }
    };
    users.extend(d.created_by.iter().cloned());
    users.sort();
    users.dedup();
    Ok(Some(users))
}

pub(crate) struct Alert {
    pub deadline: Deadline,
    pub to: Option<Vec<String>>,
}

/// Revisa los plazos y marca como enviados los avisos que tocan hoy
pub(crate) fn pending_alerts(conn: &Connection, today: NaiveDate) -> rusqlite::Result<Vec<Alert>> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut out = Vec::new();
    for d in active_deadlines(conn, today)? {
        let mut stmt = conn.prepare_cached("SELECT threshold FROM deadline_alerts WHERE key = ?1 AND due_date = ?2")?;
        let sent = stmt
            .query_map(params![d.key, d.due_date], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let Some(t) = due_threshold(d.days_left, &sent) else { continue };
        for th in THRESHOLDS.iter().filter(|th| **th >= t) {
            conn.execute(
                "INSERT OR IGNORE INTO deadline_alerts (key, due_date, threshold, sent_at) VALUES (?1, ?2, ?3, ?4)",
                params![d.key, d.due_date, th, now],
            )?;
        }
        let to = recipients(conn, &d)?;
        out.push(Alert { deadline: d, to });
    }
    Ok(out)
}

pub async fn alert_loop(state: AppState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
    loop {
        tick.tick().await;
        if !ALERT_HOURS.contains(&Local::now().hour()) {
            continue;
        }
        let alerts = db_op(&state.db, |conn| pending_alerts(conn, today()).map_err(|e| e.to_string()))
            .await
            .unwrap_or_else(|e| {
                eprintln!("[plazos] {}", e.1);
                Vec::new()
            });
        for a in alerts {
            let d = &a.deadline;
            let when = countdown_text(d.days_left);
            let project = d.project.as_deref().map(|p| format!(" ({})", p)).unwrap_or_default();
            let text = format!("⏳ *{}*{}: *{}* ({}).", md(&d.title), md(&project), when, d.due_date);
            let body = format!("{}{}: {}", d.title, project, when);
            let level = if d.days_left <= 7 { Level::Warning } else { Level::Info };
            match &a.to {
                None => {
                    crate::events::notify(&state, Audience::All, MODULE, level, "Plazo", &body);
                    crate::handlers::notifications::notify_active_chats(&state, &text).await;
                }
                Some(users) => {
                    for u in users {
                        crate::events::notify(&state, Audience::User(u.clone()), MODULE, level, "Plazo", &body);
                    }
                    crate::handlers::notifications::notify_users(&state, users, &text).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colores_por_cercania() {
        assert_eq!(level(45), "ok");
        assert_eq!(level(30), "pronto");
        assert_eq!(level(8), "pronto");
        assert_eq!(level(7), "urgente");
        assert_eq!(level(0), "urgente");
        assert_eq!(level(-1), "vencido");
    }

    #[test]
    fn umbral_de_aviso() {
        assert_eq!(due_threshold(40, &[]), None);
        assert_eq!(due_threshold(30, &[]), Some(30));
        // Creado a 20 dias: un solo aviso
        assert_eq!(due_threshold(20, &[]), Some(30));
        assert_eq!(due_threshold(20, &[30]), None);
        assert_eq!(due_threshold(7, &[30]), Some(7));
        assert_eq!(due_threshold(3, &[30, 7]), None);
        assert_eq!(due_threshold(1, &[30, 7]), Some(1));
        assert_eq!(due_threshold(0, &[30, 7, 1]), Some(0));
        assert_eq!(due_threshold(0, &[30, 7, 1, 0]), None);
        assert_eq!(due_threshold(-2, &[]), None);
    }

    #[test]
    fn fechas_del_portafolio() {
        assert_eq!(parse_date("2026-12-01"), NaiveDate::from_ymd_opt(2026, 12, 1));
        assert_eq!(parse_date("2026-12-01T10:00:00Z"), NaiveDate::from_ymd_opt(2026, 12, 1));
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("pronto"), None);
    }
}
