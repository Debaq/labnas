//! Alarmas criticas con escalamiento (idea 6). Una alerta de sensor marcada como **critica**
//! (freezer, incubadora, sala de servidores) no avisa a todos a la vez: abre un **incidente**
//! y avisa por Telegram a la primera persona de su lista. Si nadie responde `/visto` (o toca
//! «Lo vi» en la web) en N minutos, avisa a la siguiente; al terminar la lista avisa a todos
//! los que gestionan Sensores. Cuando la lectura vuelve a rango el incidente se cierra y se
//! avisa a quienes ya sabian. Todo queda en el historial y en la auditoria.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::{internal, ApiError};
use crate::models::sensors::SensorAlert;
use crate::state::{AppState, SessionInfo};

#[derive(Debug, Clone, Serialize)]
pub struct Incident {
    pub id: String,
    pub alert_id: String,
    pub device_id: String,
    pub device_name: String,
    pub key: String,
    pub condition: String,
    pub threshold: f64,
    pub first_value: f64,
    pub worst_value: f64,
    pub started_at: String,
    /// Paso del escalamiento: 0 = primera persona; len(lista) = todo el staff
    pub step: i64,
    pub last_notified_at: String,
    /// A quienes se aviso, en orden
    pub notified: Vec<String>,
    pub acked_by: Option<String>,
    pub acked_at: Option<String>,
    pub resolved_at: Option<String>,
}

const SELECT: &str = "SELECT id, alert_id, device_id, device_name, key, condition, threshold, first_value, worst_value,
    started_at, step, last_notified_at, notified, acked_by, acked_at, resolved_at FROM sensor_incidents";

fn row_to_incident(r: &rusqlite::Row) -> rusqlite::Result<Incident> {
    let notified: String = r.get(12)?;
    Ok(Incident {
        id: r.get(0)?,
        alert_id: r.get(1)?,
        device_id: r.get(2)?,
        device_name: r.get(3)?,
        key: r.get(4)?,
        condition: r.get(5)?,
        threshold: r.get(6)?,
        first_value: r.get(7)?,
        worst_value: r.get(8)?,
        started_at: r.get(9)?,
        step: r.get(10)?,
        last_notified_at: r.get(11)?,
        notified: serde_json::from_str(&notified).unwrap_or_default(),
        acked_by: r.get(13)?,
        acked_at: r.get(14)?,
        resolved_at: r.get(15)?,
    })
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn open_for_alert(conn: &Connection, alert_id: &str) -> rusqlite::Result<Option<Incident>> {
    conn.query_row(&format!("{} WHERE alert_id = ?1 AND resolved_at IS NULL", SELECT), [alert_id], row_to_incident)
        .optional()
}

/// Quienes gestionan Sensores (el ultimo paso del escalamiento)
fn staff(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    crate::roles::users_with_level(conn, "sensors", crate::roles::Level::Gestionar)
}

fn describe(i: &Incident, value: f64) -> String {
    let unit = crate::models::sensors::reading_key_unit(&i.key);
    let cond = if i.condition == "below" { "bajo" } else { "sobre" };
    format!("*{}* — {}: {:.1}{} ({} el umbral de {:.1}{})", md(&i.device_name), i.key, value, unit, cond, i.threshold, unit)
}

fn md(s: &str) -> String {
    crate::handlers::machines::md(s)
}

fn minutes_since(rfc: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(rfc).map(|t| (Utc::now() - t.with_timezone(&Utc)).num_minutes()).unwrap_or(0)
}

/// Lectura fuera de rango en una alerta critica: abre el incidente (y avisa a la primera
/// persona) o, si ya estaba abierto, anota el peor valor
pub async fn on_trigger(state: &AppState, alert: &SensorAlert, device_name: &str, value: f64) {
    let (a, name) = (alert.clone(), device_name.to_string());
    let opened = db_op(&state.db, move |conn| {
        if let Some(open) = open_for_alert(conn, &a.id).map_err(|e| e.to_string())? {
            let worse = if a.condition == "below" { value < open.worst_value } else { value > open.worst_value };
            if worse {
                conn.execute("UPDATE sensor_incidents SET worst_value = ?2 WHERE id = ?1", params![open.id, value]).map_err(|e| e.to_string())?;
            }
            return Ok(None);
        }
        let first: Vec<String> = match a.escalation.first() {
            Some(u) => vec![u.clone()],
            None => staff(conn).map_err(|e| e.to_string())?,
        };
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let t = now();
        conn.execute(
            "INSERT INTO sensor_incidents (id, alert_id, device_id, device_name, key, condition, threshold, first_value, worst_value,
                started_at, step, last_notified_at, notified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?9, ?11)",
            params![
                id, a.id, a.device_id, name, a.key, a.condition, a.threshold, value, t,
                if a.escalation.is_empty() { 1 } else { 0 },
                serde_json::to_string(&first).unwrap_or_default()
            ],
        )
        .map_err(|e| e.to_string())?;
        let inc = conn.query_row(&format!("{} WHERE id = ?1", SELECT), [&id], row_to_incident).map_err(|e| e.to_string())?;
        Ok(Some((inc, first)))
    })
    .await
    .ok()
    .flatten();
    let Some((inc, first)) = opened else { return };
    let text = format!("🚨 *Alarma critica*: {}\nSi lo atiendes, responde /visto", describe(&inc, value));
    crate::handlers::notifications::notify_users(state, &first, &text).await;
    crate::events::notify(
        state,
        crate::events::Audience::All,
        Some("sensors"),
        crate::events::Level::Error,
        &format!("Alarma critica: {}", inc.device_name),
        &format!("{} fuera de rango; avisado a {}", inc.key, first.join(", ")),
    );
    state
        .log_activity("Sensores", &format!("Alarma critica abierta: {} {} = {:.1}; avisado a {}", inc.device_name, inc.key, value, first.join(", ")), "sistema")
        .await;
    publish(state);
}

/// Lectura en rango: cierra el incidente abierto de la alerta (si hay) y avisa a quienes sabian
pub async fn on_normal(state: &AppState, alert_id: &str, value: f64) {
    let id = alert_id.to_string();
    let closed = db_op(&state.db, move |conn| {
        let Some(open) = open_for_alert(conn, &id).map_err(|e| e.to_string())? else { return Ok(None) };
        conn.execute("UPDATE sensor_incidents SET resolved_at = ?2 WHERE id = ?1", params![open.id, now()]).map_err(|e| e.to_string())?;
        Ok(Some(open))
    })
    .await
    .ok()
    .flatten();
    let Some(inc) = closed else { return };
    let mins = minutes_since(&inc.started_at);
    let text = format!(
        "✅ Volvio a rango: {} (duro {} min{})",
        describe(&inc, value),
        mins,
        inc.acked_by.as_ref().map(|u| format!(", lo vio {}", u)).unwrap_or_else(|| ", nadie lo confirmo".into())
    );
    crate::handlers::notifications::notify_users(state, &inc.notified, &text).await;
    state.log_activity("Sensores", &format!("Alarma critica cerrada: {} {} (duro {} min)", inc.device_name, inc.key, mins), "sistema").await;
    publish(state);
}

#[derive(Debug, PartialEq)]
enum Next {
    /// La persona en esa posicion de la lista
    Person(usize),
    /// Todos los que gestionan Sensores
    Staff,
}

/// A quien avisar despues del paso `step` con una lista de `len` personas (None = ya se aviso
/// a todos)
fn next_step(step: i64, len: usize) -> Option<Next> {
    let n = (step + 1) as usize;
    if step as usize > len {
        None
    } else if n < len {
        Some(Next::Person(n))
    } else if n == len {
        Some(Next::Staff)
    } else {
        None
    }
}

/// Cada 30 segundos: avisa al siguiente de la lista los incidentes sin confirmar
pub async fn escalation_loop(state: AppState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        tick.tick().await;
        let due = db_op(&state.db, |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "{} WHERE resolved_at IS NULL AND acked_at IS NULL", SELECT
                ))
                .map_err(|e| e.to_string())?;
            let open: Vec<Incident> = stmt.query_map([], row_to_incident).map_err(|e| e.to_string())?.filter_map(|r| r.ok()).collect();
            let mut out = Vec::new();
            for mut inc in open {
                let alert: Option<(String, i64)> = conn
                    .query_row("SELECT escalation, escalate_minutes FROM sensor_alerts WHERE id = ?1", [&inc.alert_id], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()
                    .map_err(|e| e.to_string())?;
                let (list, minutes) = match alert {
                    Some((l, m)) => (serde_json::from_str::<Vec<String>>(&l).unwrap_or_default(), m.max(1)),
                    None => (Vec::new(), 10),
                };
                if minutes_since(&inc.last_notified_at) < minutes {
                    continue;
                }
                let Some(next) = next_step(inc.step, list.len()) else { continue };
                let (who, last): (Vec<String>, bool) = match next {
                    Next::Person(i) => (vec![list[i].clone()], false),
                    Next::Staff => (staff(conn).map_err(|e| e.to_string())?, true),
                };
                let next_step = inc.step + 1;
                for w in &who {
                    if !inc.notified.contains(w) {
                        inc.notified.push(w.clone());
                    }
                }
                // Despues del staff no se sigue escalando
                let step = if last { list.len() as i64 + 1 } else { next_step };
                conn.execute(
                    "UPDATE sensor_incidents SET step = ?2, last_notified_at = ?3, notified = ?4 WHERE id = ?1",
                    params![inc.id, step, now(), serde_json::to_string(&inc.notified).unwrap_or_default()],
                )
                .map_err(|e| e.to_string())?;
                out.push((inc, who, minutes, last));
            }
            Ok(out)
        })
        .await
        .unwrap_or_default();
        for (inc, who, minutes, last) in due {
            let text = format!(
                "🚨 *Alarma critica sin confirmar* ({} min): {}\n{}Responde /visto si lo atiendes",
                minutes_since(&inc.started_at).max(minutes),
                describe(&inc, inc.worst_value),
                if last { "Nadie de la lista respondio. " } else { "" }
            );
            crate::handlers::notifications::notify_users(&state, &who, &text).await;
            state
                .log_activity("Sensores", &format!("Alarma critica escalada: {} {}; avisado a {}", inc.device_name, inc.key, who.join(", ")), "sistema")
                .await;
            publish(&state);
        }
    }
}

/// Confirma ("lo vi") incidentes abiertos: uno por id, o todos los avisados a `user`.
/// Devuelve los confirmados.
pub async fn ack(state: &AppState, user: &str, id: Option<String>) -> Result<Vec<Incident>, ApiError> {
    let u = user.to_string();
    let acked = db_op_status(&state.db, move |conn| {
        let mut stmt = conn
            .prepare(&format!("{} WHERE resolved_at IS NULL AND acked_at IS NULL", SELECT))
            .map_err(internal)?;
        let open: Vec<Incident> = stmt.query_map([], row_to_incident).map_err(internal)?.filter_map(|r| r.ok()).collect();
        let mut targets: Vec<Incident> = match &id {
            Some(id) => {
                let one: Vec<Incident> = open.into_iter().filter(|i| &i.id == id).collect();
                if one.is_empty() {
                    return Err((StatusCode::NOT_FOUND, "No hay una alarma abierta sin confirmar con ese id".into()));
                }
                one
            }
            None => open.into_iter().filter(|i| i.notified.contains(&u)).collect(),
        };
        let t = now();
        for i in targets.iter_mut() {
            conn.execute("UPDATE sensor_incidents SET acked_by = ?2, acked_at = ?3 WHERE id = ?1", params![i.id, u, t]).map_err(internal)?;
            i.acked_by = Some(u.clone());
            i.acked_at = Some(t.clone());
        }
        Ok(targets)
    })
    .await?;
    for i in &acked {
        let others: Vec<String> = i.notified.iter().filter(|n| n.as_str() != user).cloned().collect();
        let text = format!("👀 {} lo esta atendiendo: {}", user, describe(i, i.worst_value));
        crate::handlers::notifications::notify_users(state, &others, &text).await;
        state.log_activity("Sensores", &format!("Confirmo la alarma critica de {} ({})", i.device_name, i.key), user).await;
    }
    if !acked.is_empty() {
        publish(state);
    }
    Ok(acked)
}

/// /visto por Telegram (chat vinculado a una cuenta web)
pub async fn telegram_ack(state: &AppState, user: Option<&str>) -> String {
    let Some(user) = user else {
        return "Vincula este chat a tu cuenta web para confirmar alarmas (/vincular).".into();
    };
    match ack(state, user, None).await {
        Ok(list) if list.is_empty() => "No tienes alarmas abiertas por confirmar.".into(),
        Ok(list) => format!(
            "Listo: quedaste atendiendo {}. Se avisa a los demas.",
            list.iter().map(|i| format!("{} ({})", i.device_name, i.key)).collect::<Vec<_>>().join(", ")
        ),
        Err((_, e)) => e,
    }
}

fn publish(state: &AppState) {
    state.events.publish("sensors.incidents", (), crate::events::Audience::All, Some("sensors"));
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// Solo los abiertos
    #[serde(default)]
    pub open: bool,
}

/// GET /api/sensors/incidents?open=true
pub async fn list(State(state): State<AppState>, Query(q): Query<ListQuery>) -> Result<Json<Vec<Incident>>, ApiError> {
    db_op(&state.db, move |conn| {
        let sql = if q.open {
            format!("{} WHERE resolved_at IS NULL ORDER BY started_at DESC", SELECT)
        } else {
            format!("{} ORDER BY started_at DESC LIMIT 100", SELECT)
        };
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_incident).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())
    })
    .await
    .map(Json)
}

/// POST /api/sensors/incidents/{id}/ack — «Lo vi» desde la web
pub async fn ack_web(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<Incident>, ApiError> {
    let list = ack(&state, &session.username, Some(id)).await?;
    list.into_iter().next().map(Json).ok_or((StatusCode::NOT_FOUND, "No encontrada".into()))
}

/// Usuarios validos para la lista de escalamiento (cuentas aprobadas)
pub fn valid_escalation(conn: &Connection, list: &[String]) -> Result<(), String> {
    if list.len() > 10 {
        return Err("Maximo 10 personas en la lista".into());
    }
    for u in list {
        let ok: bool = conn
            .query_row("SELECT 1 FROM web_users WHERE username = ?1 AND role != 'pendiente'", [u], |_| Ok(true))
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or(false);
        if !ok {
            return Err(format!("La cuenta {} no existe o esta pendiente", u));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalamiento_por_la_lista_y_luego_el_staff() {
        // lista de 2: A (paso 0) -> B -> staff -> nada
        assert_eq!(next_step(0, 2), Some(Next::Person(1)));
        assert_eq!(next_step(1, 2), Some(Next::Staff));
        assert_eq!(next_step(3, 2), None);
        // sin lista se aviso al staff de entrada (paso 1)
        assert_eq!(next_step(1, 0), None);
        // una persona: A -> staff
        assert_eq!(next_step(0, 1), Some(Next::Staff));
        assert_eq!(next_step(2, 1), None);
    }
}
