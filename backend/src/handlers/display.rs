//! Pantalla del laboratorio (ideas 8 y 47): una TV o tablet que muestra, sin sesion, las
//! maquinas en uso y el tiempo que les queda, las impresoras 3D, la agenda del dia, los
//! sensores, los plazos cercanos y los avisos (fallas, mantenciones vencidas, alarmas).
//!
//! El admin crea una pantalla por equipo en Configuracion > Laboratorio y elige que secciones
//! muestra; cada una tiene un enlace secreto `/pantalla/<token>` que se puede revocar. Solo
//! lectura: el enlace no sirve para nada mas. Cada seccion respeta si su modulo esta encendido.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Duration, Local};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::{internal, ApiError};
use crate::state::{AppState, SessionInfo};

/// (seccion, modulo que la alimenta, nombre)
pub const SECTIONS: &[(&str, &str, &str)] = &[
    ("maquinas", "machines", "Maquinas"),
    ("impresoras", "printers3d", "Impresoras 3D"),
    ("agenda", "machines", "Reservas de hoy"),
    ("sensores", "sensors", "Sensores"),
    ("plazos", "deadlines", "Plazos cercanos"),
    ("avisos", "machines", "Avisos"),
    ("presencia", "seguridad", "Quien esta en el laboratorio"),
];

#[derive(Debug, Serialize)]
pub struct Screen {
    pub token: String,
    pub name: String,
    pub sections: Vec<String>,
    pub created_by: String,
    pub created_at: String,
    /// Ultima vez que la pantalla pidio datos
    pub last_seen_at: Option<String>,
}

fn row_to_screen(r: &rusqlite::Row) -> rusqlite::Result<Screen> {
    let sections: String = r.get(2)?;
    Ok(Screen {
        token: r.get(0)?,
        name: r.get(1)?,
        sections: serde_json::from_str(&sections).unwrap_or_default(),
        created_by: r.get(3)?,
        created_at: r.get(4)?,
        last_seen_at: r.get(5)?,
    })
}

const SELECT: &str = "SELECT token, name, sections, created_by, created_at, last_seen_at FROM display_screens";

fn random_token() -> String {
    let mut buf = [0u8; 20];
    getrandom::fill(&mut buf).expect("getrandom");
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

fn now_str() -> String {
    Local::now().naive_local().format("%Y-%m-%dT%H:%M").to_string()
}

#[derive(Deserialize)]
pub struct ScreenReq {
    pub name: String,
    pub sections: Vec<String>,
}

fn validate(req: &mut ScreenReq) -> Result<(), ApiError> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() || req.name.chars().count() > 60 {
        return Err((StatusCode::BAD_REQUEST, "Ponle un nombre (maximo 60 caracteres)".into()));
    }
    req.sections.retain(|s| SECTIONS.iter().any(|(id, _, _)| id == s));
    req.sections.dedup();
    if req.sections.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Elige al menos una seccion".into()));
    }
    Ok(())
}

/// GET /api/display/screens (admin)
pub async fn list_screens(State(state): State<AppState>) -> Result<Json<Vec<Screen>>, ApiError> {
    db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(&format!("{} ORDER BY created_at", SELECT)).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_screen).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())
    })
    .await
    .map(Json)
}

/// POST /api/display/screens (admin)
pub async fn create_screen(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(mut req): Json<ScreenReq>,
) -> Result<(StatusCode, Json<Screen>), ApiError> {
    validate(&mut req)?;
    let user = session.username.clone();
    let screen = db_op_status(&state.db, move |conn| {
        let token = random_token();
        conn.execute(
            "INSERT INTO display_screens (token, name, sections, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![token, req.name, serde_json::to_string(&req.sections).unwrap_or_default(), user, now_str()],
        )
        .map_err(internal)?;
        conn.query_row(&format!("{} WHERE token = ?1", SELECT), [&token], row_to_screen).map_err(internal)
    })
    .await?;
    state.log_activity("Pantalla", &format!("Creo la pantalla «{}»", screen.name), &session.username).await;
    Ok((StatusCode::CREATED, Json(screen)))
}

/// PUT /api/display/screens/{token} (admin)
pub async fn update_screen(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(mut req): Json<ScreenReq>,
) -> Result<Json<Screen>, ApiError> {
    validate(&mut req)?;
    db_op_status(&state.db, move |conn| {
        let n = conn
            .execute(
                "UPDATE display_screens SET name = ?2, sections = ?3 WHERE token = ?1",
                params![token, req.name, serde_json::to_string(&req.sections).unwrap_or_default()],
            )
            .map_err(internal)?;
        if n == 0 {
            return Err((StatusCode::NOT_FOUND, "Pantalla no encontrada".into()));
        }
        conn.query_row(&format!("{} WHERE token = ?1", SELECT), [&token], row_to_screen).map_err(internal)
    })
    .await
    .map(Json)
}

/// DELETE /api/display/screens/{token} (admin): el enlace deja de funcionar
pub async fn delete_screen(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(token): Path<String>,
) -> Result<StatusCode, ApiError> {
    let name = db_op_status(&state.db, move |conn| {
        let name: Option<String> = conn
            .query_row("SELECT name FROM display_screens WHERE token = ?1", [&token], |r| r.get(0))
            .optional()
            .map_err(internal)?;
        let name = name.ok_or((StatusCode::NOT_FOUND, "Pantalla no encontrada".to_string()))?;
        conn.execute("DELETE FROM display_screens WHERE token = ?1", [&token]).map_err(internal)?;
        Ok(name)
    })
    .await?;
    state.log_activity("Pantalla", &format!("Revoco la pantalla «{}»", name), &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Datos de la pantalla ───

#[derive(Debug, Serialize)]
pub struct MachineRow {
    pub name: String,
    pub kind: String,
    /// libre | en_uso | bloqueada | fuera_servicio
    pub state: &'static str,
    pub user: Option<String>,
    pub until: Option<String>,
    pub next_start: Option<String>,
    pub next_user: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PrinterRow {
    pub name: String,
    pub online: bool,
    pub state: String,
    pub file: Option<String>,
    pub progress: Option<f64>,
    pub remaining_secs: Option<u64>,
    pub hotend: Option<f64>,
    pub bed: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct AgendaRow {
    pub machine: String,
    pub user: String,
    pub start: String,
    pub end: String,
    pub kind: String,
    pub pending: bool,
}

#[derive(Debug, Serialize)]
pub struct SensorRow {
    pub name: String,
    pub online: bool,
    pub readings: Vec<(String, f64)>,
    /// Lecturas fuera del umbral de alguna alerta
    pub alarm: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DeadlineRow {
    pub title: String,
    pub due_date: String,
    pub days_left: i64,
    pub level: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Notice {
    /// falla | mantencion | sensor
    pub kind: &'static str,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct PresenceRow {
    pub who: String,
    pub visitor: bool,
    pub since: String,
}

#[derive(Debug, Default, Serialize)]
pub struct DisplayData {
    pub name: String,
    pub lab_name: String,
    pub logo_url: String,
    pub now: String,
    pub sections: Vec<String>,
    pub machines: Vec<MachineRow>,
    pub printers: Vec<PrinterRow>,
    pub agenda: Vec<AgendaRow>,
    pub sensors: Vec<SensorRow>,
    pub deadlines: Vec<DeadlineRow>,
    pub notices: Vec<Notice>,
    pub presence: Vec<PresenceRow>,
}

/// Nombre y logo del laboratorio (Configuracion > Laboratorio)
fn branding(conn: &Connection) -> (String, String) {
    conn.query_row("SELECT lab_name, logo_url FROM branding WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or_default()
}

fn machines_data(conn: &Connection, data: &mut DisplayData, want: &dyn Fn(&str) -> bool) -> rusqlite::Result<()> {
    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM machines WHERE status != 'retirada' ORDER BY name COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for id in &ids {
        let Some(m) = crate::handlers::machines::load_machine(conn, id)? else { continue };
        if want("maquinas") {
            let (state, user, until) = if m.status == "fuera_servicio" {
                ("fuera_servicio", None, None)
            } else if let Some(c) = &m.current {
                (if c.kind == "bloqueo" { "bloqueada" } else { "en_uso" }, (c.kind != "bloqueo").then(|| c.username.clone()), Some(c.end.clone()))
            } else {
                ("libre", None, None)
            };
            data.machines.push(MachineRow {
                name: m.name.clone(),
                kind: m.kind.clone(),
                state,
                user,
                until,
                next_start: m.next.as_ref().map(|n| n.start.clone()),
                next_user: m.next.as_ref().filter(|n| n.kind == "reserva").map(|n| n.username.clone()),
            });
        }
        if want("avisos") {
            for f in &m.open_faults {
                data.notices.push(Notice { kind: "falla", text: format!("{}: {}", m.name, f.description) });
            }
        }
    }
    if want("avisos") {
        let tasks = crate::handlers::machine_maintenance::load_all(conn)?;
        for t in tasks.into_iter().filter(|t| t.level == "vencida") {
            data.notices.push(Notice { kind: "mantencion", text: format!("{}: toca «{}»", t.machine_name, t.title) });
        }
    }
    if want("agenda") {
        let now = Local::now().naive_local();
        let today = now.date();
        let (from, to) = (today.format("%Y-%m-%dT00:00").to_string(), (today + Duration::days(1)).format("%Y-%m-%dT00:00").to_string());
        let now_s = now.format("%Y-%m-%dT%H:%M").to_string();
        let mut stmt = conn.prepare(
            "SELECT m.name, b.username, b.start, b.end, b.kind, b.status FROM machine_bookings b JOIN machines m ON m.id = b.machine_id
             WHERE b.status IN ('confirmada', 'pendiente') AND b.start < ?2 AND b.end > ?1 AND b.end > ?3 ORDER BY b.start LIMIT 30",
        )?;
        let rows = stmt.query_map(params![from, to, now_s], |r| {
            let status: String = r.get(5)?;
            Ok(AgendaRow { machine: r.get(0)?, user: r.get(1)?, start: r.get(2)?, end: r.get(3)?, kind: r.get(4)?, pending: status == "pendiente" })
        })?;
        data.agenda = rows.collect::<rusqlite::Result<_>>()?;
    }
    Ok(())
}

/// GET /api/pantalla/{token} — publica, de solo lectura
pub async fn display_data(State(state): State<AppState>, Path(token): Path<String>) -> Result<Json<DisplayData>, ApiError> {
    let none = || (StatusCode::NOT_FOUND, "Pantalla no encontrada".to_string());
    if token.len() != 40 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(none());
    }
    let on = state.enabled_modules.lock().await.clone();
    let tk = token.clone();
    let mut data = db_op_status(&state.db, move |conn| {
        let screen = conn
            .query_row(&format!("{} WHERE token = ?1", SELECT), [&tk], row_to_screen)
            .optional()
            .map_err(internal)?
            .ok_or_else(none)?;
        conn.execute("UPDATE display_screens SET last_seen_at = ?2 WHERE token = ?1", params![tk, now_str()]).ok();
        // Solo las secciones cuyo modulo esta encendido
        let sections: Vec<String> = screen
            .sections
            .into_iter()
            .filter(|s| SECTIONS.iter().any(|(id, m, _)| id == s && on.contains(*m)))
            .collect();
        let want = |s: &str| sections.iter().any(|x| x == s);
        let (lab_name, logo_url) = branding(conn);
        let mut data = DisplayData {
            name: screen.name,
            lab_name,
            logo_url,
            now: now_str(),
            ..Default::default()
        };
        if want("maquinas") || want("agenda") || want("avisos") {
            machines_data(conn, &mut data, &want).map_err(internal)?;
        }
        if want("plazos") {
            data.deadlines = crate::handlers::deadlines::upcoming(conn, 14)
                .map_err(internal)?
                .into_iter()
                .take(8)
                .map(|d| DeadlineRow { title: d.title, due_date: d.due_date, days_left: d.days_left, level: d.level })
                .collect();
        }
        if want("presencia") {
            data.presence = crate::handlers::presence::inside(conn)
                .map_err(internal)?
                .into_iter()
                .map(|e| PresenceRow { who: e.who, visitor: e.visitor, since: e.entered_at })
                .collect();
        }
        data.sections = sections;
        Ok(data)
    })
    .await?;

    let want = |s: &str| data.sections.iter().any(|x| x == s);
    if want("impresoras") {
        data.printers = crate::handlers::printers3d::cached_statuses()
            .into_iter()
            .map(|(name, st)| match st {
                Some(s) => PrinterRow {
                    name,
                    online: s.online,
                    state: s.current_job.as_ref().map(|j| j.state.clone()).unwrap_or_else(|| if s.online { "Operational".into() } else { "Offline".into() }),
                    file: s.current_job.as_ref().map(|j| j.file_name.clone()).filter(|f| !f.is_empty()),
                    progress: s.current_job.as_ref().map(|j| j.progress),
                    remaining_secs: s.current_job.as_ref().and_then(|j| j.time_remaining),
                    hotend: s.temperatures.as_ref().map(|t| t.hotend_actual),
                    bed: s.temperatures.as_ref().map(|t| t.bed_actual),
                },
                None => PrinterRow { name, online: false, state: "Offline".into(), file: None, progress: None, remaining_secs: None, hotend: None, bed: None },
            })
            .collect();
    }
    if want("sensores") || want("avisos") {
        let latest = crate::handlers::sensors::get_latest(State(state.clone())).await.0;
        let alerts: Vec<(String, String, String, f64)> = db_op(&state.db, |conn| {
            let mut stmt = conn
                .prepare("SELECT device_id, key, condition, threshold FROM sensor_alerts WHERE enabled = 1")
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(|e| e.to_string())?;
            rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())
        })
        .await
        .unwrap_or_default();
        for s in latest {
            let Some(dev) = s.device else { continue };
            if dev.status != "accepted" {
                continue;
            }
            let mut readings: Vec<(String, f64)> = s.readings.into_iter().collect();
            readings.sort_by(|a, b| a.0.cmp(&b.0));
            let alarm: Vec<String> = alerts
                .iter()
                .filter(|(d, k, c, t)| {
                    *d == dev.id && readings.iter().any(|(rk, v)| rk == k && if c == "below" { v < t } else { v > t })
                })
                .map(|(_, k, _, _)| k.clone())
                .collect();
            if want("avisos") && !alarm.is_empty() {
                data.notices.push(Notice { kind: "sensor", text: format!("{}: {} fuera de rango", dev.name, alarm.join(", ")) });
            }
            if want("sensores") {
                data.sensors.push(SensorRow { name: dev.name, online: s.online, readings, alarm });
            }
        }
    }
    state.log_access(&format!("Pantalla «{}»", data.name), "pantalla").await;
    Ok(Json(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secciones_con_modulo_conocido() {
        let ids = crate::modules::ids();
        for (_, m, _) in SECTIONS {
            assert!(ids.contains(m), "{m}");
        }
    }
}
