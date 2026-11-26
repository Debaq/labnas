//! Mantenimiento y calibracion por maquina (idea 5). Ver docs/MAQUINAS.md.
//!
//! - **Plan**: tareas que se repiten cada N dias y/o cada N horas de uso ("limpiar el lente
//!   cada 40 h", "calibrar la cama cada 30 dias"). Las de fecha aparecen en Plazos; las de
//!   horas avisan por Telegram al vencer. Una tarea puede **bloquear las reservas** mientras
//!   este vencida (por ejemplo, la revision de seguridad de la laser).
//! - **Bitacora**: cada mantencion, calibracion, reparacion o nota, con quien, cuando, las
//!   horas de uso de ese momento y foto opcional. No se edita ni se borra. Las fallas
//!   reparadas entran solas.
//! - **Horas de uso**: las horas de reservas confirmadas ya terminadas, mas un ajuste que se
//!   fija leyendo el horometro de la maquina. Cuando exista el medidor de corriente
//!   (idea 19), se sumara aqui.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Duration, Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::ApiError;
use crate::handlers::machines::{bad, db_err, decode_photo, forbidden, insert_photo, is_staff, load_machine, md, not_found, publish};
use crate::state::{AppState, SessionInfo};

pub const TASK_KINDS: &[&str] = &["mantencion", "calibracion"];
pub const LOG_KINDS: &[&str] = &["mantencion", "calibracion", "reparacion", "nota"];

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

fn now_str() -> String {
    Local::now().naive_local().format("%Y-%m-%dT%H:%M").to_string()
}

fn date_of(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Horas de uso de una maquina: (total, horas de reservas, ajuste por horometro)
pub fn usage_hours(conn: &Connection, machine_id: &str) -> rusqlite::Result<(f64, f64, f64)> {
    let booked: f64 = conn.query_row(
        "SELECT COALESCE(SUM((julianday(end) - julianday(start)) * 24.0), 0) FROM machine_bookings
         WHERE machine_id = ?1 AND kind = 'reserva' AND status = 'confirmada' AND end <= ?2",
        params![machine_id, now_str()],
        |r| r.get(0),
    )?;
    let adjust: f64 = conn
        .query_row("SELECT usage_adjust_hours FROM machines WHERE id = ?1", [machine_id], |r| r.get(0))
        .optional()?
        .unwrap_or(0.0);
    Ok((round1((booked + adjust).max(0.0)), round1(booked), round1(adjust)))
}

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: String,
    pub machine_id: String,
    pub machine_name: String,
    pub title: String,
    pub kind: String,
    pub every_days: Option<i64>,
    pub every_hours: Option<f64>,
    pub instructions: String,
    pub blocks: bool,
    pub last_done_at: Option<String>,
    pub last_done_hours: f64,
    pub created_at: String,
    /// Calculados: fecha en que toca y dias que faltan
    pub due_date: Option<String>,
    pub days_left: Option<i64>,
    /// Horas de uso que faltan (negativo = pasada)
    pub hours_left: Option<f64>,
    /// ok | pronto | vencida
    pub level: &'static str,
}

const TASK_SELECT: &str = "SELECT t.id, t.machine_id, COALESCE(m.name, ''), t.title, t.kind, t.every_days, t.every_hours,
    t.instructions, t.blocks, t.last_done_at, t.last_done_hours, t.created_at
    FROM machine_maintenance_tasks t LEFT JOIN machines m ON m.id = t.machine_id";

fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        machine_id: r.get(1)?,
        machine_name: r.get(2)?,
        title: r.get(3)?,
        kind: r.get(4)?,
        every_days: r.get(5)?,
        every_hours: r.get(6)?,
        instructions: r.get(7)?,
        blocks: r.get::<_, i64>(8)? != 0,
        last_done_at: r.get(9)?,
        last_done_hours: r.get::<_, Option<f64>>(10)?.unwrap_or(0.0),
        created_at: r.get(11)?,
        due_date: None,
        days_left: None,
        hours_left: None,
        level: "ok",
    })
}

/// Calcula cuando toca una tarea segun la fecha de hoy y las horas de uso de su maquina
pub fn compute(mut t: Task, today: NaiveDate, usage: f64) -> Task {
    let mut level = "ok";
    if let Some(days) = t.every_days {
        let base = t.last_done_at.as_deref().or(Some(t.created_at.as_str())).and_then(date_of).unwrap_or(today);
        let due = base + Duration::days(days);
        let left = (due - today).num_days();
        t.due_date = Some(due.format("%Y-%m-%d").to_string());
        t.days_left = Some(left);
        level = if left < 0 { "vencida" } else if left <= 7.min(days / 4).max(1) { "pronto" } else { "ok" };
    }
    if let Some(every) = t.every_hours {
        let left = round1(t.last_done_hours + every - usage);
        t.hours_left = Some(left);
        let hl = if left <= 0.0 { "vencida" } else if left <= (every * 0.1).max(1.0) { "pronto" } else { "ok" };
        level = worst(level, hl);
    }
    t.level = level;
    t
}

fn worst(a: &'static str, b: &'static str) -> &'static str {
    let rank = |l: &str| match l {
        "vencida" => 2,
        "pronto" => 1,
        _ => 0,
    };
    if rank(b) > rank(a) { b } else { a }
}

fn load_tasks(conn: &Connection, machine_id: Option<&str>) -> rusqlite::Result<Vec<Task>> {
    let today = Local::now().date_naive();
    let (sql, args): (String, Vec<String>) = match machine_id {
        Some(id) => (format!("{} WHERE t.machine_id = ?1 ORDER BY t.title", TASK_SELECT), vec![id.to_string()]),
        None => (format!("{} WHERE m.status != 'retirada' ORDER BY m.name, t.title", TASK_SELECT), vec![]),
    };
    let mut stmt = conn.prepare(&sql)?;
    let tasks: Vec<Task> = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_task)?.collect::<rusqlite::Result<_>>()?;
    let mut usage = std::collections::HashMap::new();
    let mut out = Vec::with_capacity(tasks.len());
    for t in tasks {
        let u = match usage.get(&t.machine_id) {
            Some(u) => *u,
            None => {
                let u = usage_hours(conn, &t.machine_id)?.0;
                usage.insert(t.machine_id.clone(), u);
                u
            }
        };
        out.push(compute(t, today, u));
    }
    Ok(out)
}

/// Tarea vencida que bloquea las reservas de la maquina (para create_booking)
pub fn blocking_task(conn: &Connection, machine_id: &str) -> rusqlite::Result<Option<String>> {
    Ok(load_tasks(conn, Some(machine_id))?.into_iter().find(|t| t.blocks && t.level == "vencida").map(|t| t.title))
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub id: String,
    pub machine_id: String,
    pub task_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub note: String,
    pub usage_hours: f64,
    pub photo_id: Option<String>,
    pub done_by: String,
    pub done_at: String,
}

fn load_log(conn: &Connection, machine_id: &str) -> rusqlite::Result<Vec<LogEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, machine_id, task_id, kind, title, note, usage_hours, photo_id, done_by, done_at
         FROM machine_maintenance_log WHERE machine_id = ?1 ORDER BY done_at DESC LIMIT 200",
    )?;
    let rows = stmt.query_map([machine_id], |r| {
        Ok(LogEntry {
            id: r.get(0)?,
            machine_id: r.get(1)?,
            task_id: r.get(2)?,
            kind: r.get(3)?,
            title: r.get(4)?,
            note: r.get(5)?,
            usage_hours: r.get(6)?,
            photo_id: r.get(7)?,
            done_by: r.get(8)?,
            done_at: r.get(9)?,
        })
    })?;
    rows.collect()
}

/// Anota en la bitacora (tambien la usa machines::resolve_fault)
#[allow(clippy::too_many_arguments)]
pub fn insert_log(
    conn: &Connection,
    machine_id: &str,
    task_id: Option<&str>,
    kind: &str,
    title: &str,
    note: &str,
    photo_id: Option<&str>,
    user: &str,
) -> rusqlite::Result<String> {
    let id = new_id();
    let usage = usage_hours(conn, machine_id)?.0;
    conn.execute(
        "INSERT INTO machine_maintenance_log (id, machine_id, task_id, kind, title, note, usage_hours, photo_id, done_by, done_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![id, machine_id, task_id, kind, title, note, usage, photo_id, user, now_str()],
    )?;
    Ok(id)
}

#[derive(Debug, Serialize)]
pub struct Overview {
    pub usage_hours: f64,
    pub booked_hours: f64,
    pub adjust_hours: f64,
    pub tasks: Vec<Task>,
    pub log: Vec<LogEntry>,
}

/// GET /api/machines/{id}/maintenance — plan, bitacora y horas de uso
pub async fn get_overview(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Overview>, ApiError> {
    db_op_status(&state.db, move |conn| {
        load_machine(conn, &id).map_err(db_err)?.ok_or_else(not_found)?;
        let (usage, booked, adjust) = usage_hours(conn, &id).map_err(db_err)?;
        Ok(Overview {
            usage_hours: usage,
            booked_hours: booked,
            adjust_hours: adjust,
            tasks: load_tasks(conn, Some(&id)).map_err(db_err)?,
            log: load_log(conn, &id).map_err(db_err)?,
        })
    })
    .await
    .map(Json)
}

/// GET /api/machines/maintenance — tareas de todas las maquinas, vencidas primero
pub async fn list_all(State(state): State<AppState>) -> Result<Json<Vec<Task>>, ApiError> {
    let mut list = db_op(&state.db, |conn| load_tasks(conn, None).map_err(|e| e.to_string())).await?;
    let rank = |t: &Task| match t.level {
        "vencida" => 0,
        "pronto" => 1,
        _ => 2,
    };
    list.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.machine_name.cmp(&b.machine_name)));
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct TaskReq {
    pub title: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub every_days: Option<i64>,
    pub every_hours: Option<f64>,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub blocks: bool,
}

fn default_kind() -> String {
    "mantencion".into()
}

fn validate(req: &mut TaskReq) -> Result<(), ApiError> {
    req.title = req.title.trim().to_string();
    req.instructions = req.instructions.trim().to_string();
    if req.title.is_empty() || req.title.chars().count() > 120 {
        return Err(bad("El nombre de la tarea es obligatorio (maximo 120 caracteres)"));
    }
    if req.instructions.chars().count() > 4000 {
        return Err(bad("Instrucciones demasiado largas"));
    }
    if !TASK_KINDS.contains(&req.kind.as_str()) {
        return Err(bad("Tipo invalido: mantencion o calibracion"));
    }
    if req.every_days.is_none() && req.every_hours.is_none() {
        return Err(bad("Indica cada cuantos dias o cada cuantas horas de uso"));
    }
    if req.every_days.is_some_and(|d| !(1..=3650).contains(&d)) {
        return Err(bad("Cada cuantos dias: entre 1 y 3650"));
    }
    if req.every_hours.is_some_and(|h| !(0.5..=100000.0).contains(&h)) {
        return Err(bad("Cada cuantas horas: entre 0,5 y 100000"));
    }
    Ok(())
}

/// POST /api/machines/{id}/maintenance/tasks — las horas cuentan desde ahora
pub async fn create_task(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(machine_id): Path<String>,
    Json(mut req): Json<TaskReq>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    validate(&mut req)?;
    let user = session.username.clone();
    let task = db_op_status(&state.db, move |conn| {
        load_machine(conn, &machine_id).map_err(db_err)?.ok_or_else(not_found)?;
        let id = new_id();
        let usage = usage_hours(conn, &machine_id).map_err(db_err)?.0;
        conn.execute(
            "INSERT INTO machine_maintenance_tasks (id, machine_id, title, kind, every_days, every_hours, instructions, blocks, last_done_hours, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, machine_id, req.title, req.kind, req.every_days, req.every_hours, req.instructions, req.blocks, usage, user, now_str()],
        )
        .map_err(db_err)?;
        load_tasks(conn, Some(&machine_id)).map_err(db_err)?.into_iter().find(|t| t.id == id).ok_or_else(not_found)
    })
    .await?;
    state.log_activity("Maquinas", &format!("Plan de mantencion de {}: nueva tarea «{}»", task.machine_name, task.title), &session.username).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(task)))
}

/// PUT /api/machines/maintenance/tasks/{id}
pub async fn update_task(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(mut req): Json<TaskReq>,
) -> Result<Json<Task>, ApiError> {
    validate(&mut req)?;
    let task = db_op_status(&state.db, move |conn| {
        let n = conn
            .execute(
                "UPDATE machine_maintenance_tasks SET title = ?2, kind = ?3, every_days = ?4, every_hours = ?5, instructions = ?6, blocks = ?7 WHERE id = ?1",
                params![id, req.title, req.kind, req.every_days, req.every_hours, req.instructions, req.blocks],
            )
            .map_err(db_err)?;
        if n == 0 {
            return Err((StatusCode::NOT_FOUND, "Tarea no encontrada".into()));
        }
        let t = conn.query_row(&format!("{} WHERE t.id = ?1", TASK_SELECT), [&id], row_to_task).map_err(db_err)?;
        let usage = usage_hours(conn, &t.machine_id).map_err(db_err)?.0;
        Ok(compute(t, Local::now().date_naive(), usage))
    })
    .await?;
    state.log_activity("Maquinas", &format!("Plan de mantencion de {}: cambio «{}»", task.machine_name, task.title), &session.username).await;
    publish(&state);
    Ok(Json(task))
}

/// DELETE /api/machines/maintenance/tasks/{id} — la bitacora se conserva
pub async fn delete_task(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let t = db_op_status(&state.db, move |conn| {
        let t = conn
            .query_row(&format!("{} WHERE t.id = ?1", TASK_SELECT), [&id], row_to_task)
            .optional()
            .map_err(db_err)?
            .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;
        conn.execute("DELETE FROM machine_maintenance_tasks WHERE id = ?1", [&id]).map_err(db_err)?;
        Ok(t)
    })
    .await?;
    state.log_activity("Maquinas", &format!("Plan de mantencion de {}: quito «{}»", t.machine_name, t.title), &session.username).await;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct LogReq {
    /// Tarea del plan que se hizo (si no, es un registro suelto)
    pub task_id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub note: String,
    /// Foto en base64 (data URL o crudo)
    pub photo: Option<String>,
}

/// POST /api/machines/{id}/maintenance/log — anota una mantencion. Si es una tarea del plan,
/// la tarea vuelve a contar desde ahora. La registra el staff o el responsable de la maquina.
pub async fn add_log(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(machine_id): Path<String>,
    Json(req): Json<LogReq>,
) -> Result<(StatusCode, Json<Overview>), ApiError> {
    let note = req.note.trim().to_string();
    if note.chars().count() > 4000 {
        return Err(bad("Nota demasiado larga"));
    }
    let photo = req.photo.as_deref().filter(|p| !p.is_empty()).map(decode_photo).transpose()?;
    let staff = is_staff(&session);
    let user = session.username.clone();
    let (overview, text, name) = db_op_status(&state.db, move |conn| {
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate).map_err(db_err)?;
        let m = load_machine(&tx, &machine_id).map_err(db_err)?.ok_or_else(not_found)?;
        if !staff && m.responsible.as_deref() != Some(user.as_str()) {
            return Err(forbidden("La mantencion la registra el responsable de la maquina, un operador o un tecnico"));
        }
        let task = match req.task_id.as_deref() {
            Some(tid) => Some(
                tx.query_row(&format!("{} WHERE t.id = ?1 AND t.machine_id = ?2", TASK_SELECT), params![tid, machine_id], row_to_task)
                    .optional()
                    .map_err(db_err)?
                    .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?,
            ),
            None => None,
        };
        let (kind, title) = match &task {
            Some(t) => (t.kind.clone(), t.title.clone()),
            None => {
                let kind = req.kind.clone().unwrap_or_else(|| "nota".into());
                if !LOG_KINDS.contains(&kind.as_str()) {
                    return Err(bad("Tipo invalido"));
                }
                let title = req.title.trim().to_string();
                if title.is_empty() || title.chars().count() > 120 {
                    return Err(bad("Escribe que se hizo (maximo 120 caracteres)"));
                }
                (kind, title)
            }
        };
        let photo_id = match &photo {
            Some((bytes, mime)) => Some(insert_photo(&tx, &machine_id, bytes, mime, &user).map_err(db_err)?),
            None => None,
        };
        insert_log(&tx, &machine_id, task.as_ref().map(|t| t.id.as_str()), &kind, &title, &note, photo_id.as_deref(), &user).map_err(db_err)?;
        if let Some(t) = &task {
            let usage = usage_hours(&tx, &machine_id).map_err(db_err)?.0;
            tx.execute(
                "UPDATE machine_maintenance_tasks SET last_done_at = ?2, last_done_hours = ?3, hours_alerted = NULL WHERE id = ?1",
                params![t.id, now_str(), usage],
            )
            .map_err(db_err)?;
        }
        let (usage, booked, adjust) = usage_hours(&tx, &machine_id).map_err(db_err)?;
        let overview = Overview {
            usage_hours: usage,
            booked_hours: booked,
            adjust_hours: adjust,
            tasks: load_tasks(&tx, Some(&machine_id)).map_err(db_err)?,
            log: load_log(&tx, &machine_id).map_err(db_err)?,
        };
        tx.commit().map_err(db_err)?;
        Ok((overview, title, m.name))
    })
    .await?;
    state.log_activity("Maquinas", &format!("Mantencion de {}: {}", name, text), &session.username).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(overview)))
}

#[derive(Deserialize)]
pub struct UsageReq {
    /// Lectura del horometro: horas de uso reales de la maquina
    pub hours: f64,
}

/// PUT /api/machines/{id}/usage — fija las horas de uso (ajusta la diferencia con las reservas)
pub async fn set_usage(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(machine_id): Path<String>,
    Json(req): Json<UsageReq>,
) -> Result<Json<Overview>, ApiError> {
    if !(0.0..=1_000_000.0).contains(&req.hours) {
        return Err(bad("Horas invalidas"));
    }
    let (overview, name, before) = db_op_status(&state.db, move |conn| {
        let m = load_machine(conn, &machine_id).map_err(db_err)?.ok_or_else(not_found)?;
        let (before, booked, _) = usage_hours(conn, &machine_id).map_err(db_err)?;
        conn.execute("UPDATE machines SET usage_adjust_hours = ?2 WHERE id = ?1", params![machine_id, req.hours - booked]).map_err(db_err)?;
        let (usage, booked, adjust) = usage_hours(conn, &machine_id).map_err(db_err)?;
        Ok((
            Overview {
                usage_hours: usage,
                booked_hours: booked,
                adjust_hours: adjust,
                tasks: load_tasks(conn, Some(&machine_id)).map_err(db_err)?,
                log: load_log(conn, &machine_id).map_err(db_err)?,
            },
            m.name,
            before,
        ))
    })
    .await?;
    state
        .log_activity("Maquinas", &format!("Horas de uso de {}: {} → {}", name, before, overview.usage_hours), &session.username)
        .await;
    publish(&state);
    Ok(Json(overview))
}

/// Tareas por fecha, para Plazos (con sus avisos a 30, 7, 1 y 0 dias)
pub fn dated_tasks(conn: &Connection) -> rusqlite::Result<Vec<(Task, Option<String>)>> {
    let tasks = load_tasks(conn, None)?;
    let mut out = Vec::new();
    for t in tasks.into_iter().filter(|t| t.due_date.is_some()) {
        let responsible: Option<String> = conn
            .query_row("SELECT responsible FROM machines WHERE id = ?1", [&t.machine_id], |r| r.get(0))
            .optional()?
            .flatten();
        out.push((t, responsible));
    }
    Ok(out)
}

/// Cada 30 minutos: aviso por Telegram cuando una tarea por horas de uso se vence (una vez por
/// ciclo), al responsable y a quienes gestionan Maquinas
pub async fn alert_loop(state: AppState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(30 * 60));
    loop {
        tick.tick().await;
        let due = db_op(&state.db, |conn| {
            let tasks = load_tasks(conn, None).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for t in tasks.into_iter().filter(|t| t.hours_left.is_some_and(|h| h <= 0.0)) {
                let alerted: Option<f64> = conn
                    .query_row("SELECT hours_alerted FROM machine_maintenance_tasks WHERE id = ?1", [&t.id], |r| r.get(0))
                    .map_err(|e| e.to_string())?;
                if alerted.is_some_and(|a| (a - t.last_done_hours).abs() < 1e-6) {
                    continue;
                }
                conn.execute("UPDATE machine_maintenance_tasks SET hours_alerted = last_done_hours WHERE id = ?1", [&t.id])
                    .map_err(|e| e.to_string())?;
                let mut users = crate::roles::users_with_level(conn, "machines", crate::roles::Level::Gestionar).map_err(|e| e.to_string())?;
                if let Some(r) = conn
                    .query_row("SELECT responsible FROM machines WHERE id = ?1", [&t.machine_id], |r| r.get::<_, Option<String>>(0))
                    .map_err(|e| e.to_string())?
                {
                    users.push(r);
                }
                users.sort();
                users.dedup();
                out.push((t, users));
            }
            Ok(out)
        })
        .await
        .unwrap_or_default();
        for (t, users) in due {
            let every = t.every_hours.unwrap_or_default();
            let mut text = format!("🔧 *{}*: toca «{}» (cada {} h de uso).", md(&t.machine_name), md(&t.title), every);
            if t.blocks {
                text.push_str(" Mientras no se registre, la maquina no admite reservas.");
            }
            crate::handlers::notifications::notify_users(&state, &users, &text).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(every_days: Option<i64>, every_hours: Option<f64>, last_done_at: Option<&str>, last_done_hours: f64) -> Task {
        Task {
            id: "t".into(),
            machine_id: "m".into(),
            machine_name: "Laser".into(),
            title: "Lente".into(),
            kind: "mantencion".into(),
            every_days,
            every_hours,
            instructions: String::new(),
            blocks: false,
            last_done_at: last_done_at.map(String::from),
            last_done_hours,
            created_at: "2026-11-01T10:00".into(),
            due_date: None,
            days_left: None,
            hours_left: None,
            level: "ok",
        }
    }

    #[test]
    fn por_fecha() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        // Sin hacer nunca: cuenta desde que se creo
        let t = compute(task(Some(30), None, None, 0.0), today, 0.0);
        assert_eq!(t.due_date.as_deref(), Some("2026-12-01"));
        assert_eq!(t.days_left, Some(5));
        assert_eq!(t.level, "pronto");
        let t = compute(task(Some(30), None, Some("2026-10-01T09:00"), 0.0), today, 0.0);
        assert_eq!(t.level, "vencida");
        let t = compute(task(Some(90), None, Some("2026-11-20T09:00"), 0.0), today, 0.0);
        assert_eq!(t.level, "ok");
    }

    #[test]
    fn por_horas_y_lo_peor_gana() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let t = compute(task(None, Some(40.0), None, 10.0), today, 30.0);
        assert_eq!(t.hours_left, Some(20.0));
        assert_eq!(t.level, "ok");
        let t = compute(task(None, Some(40.0), None, 10.0), today, 47.0);
        assert_eq!(t.level, "pronto");
        let t = compute(task(None, Some(40.0), None, 10.0), today, 51.0);
        assert_eq!(t.level, "vencida");
        // Fecha holgada, horas pasadas: vencida
        let t = compute(task(Some(365), Some(40.0), Some("2026-11-20T09:00"), 0.0), today, 45.0);
        assert_eq!(t.level, "vencida");
    }
}
