//! Registro de entrada (idea 48), dentro del modulo Seguridad: quien esta en el laboratorio
//! ahora (util en una emergencia), cuantas personas vienen y de que carrera. Cada persona marca
//! «Entre» / «Sali» desde el QR de la puerta; las visitas sin cuenta las registra alguien del
//! laboratorio. Lo que queda abierto de un dia anterior se cierra solo. Ver docs/SEGURIDAD.md.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Duration, Local};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::{internal, ApiError};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

fn now_str() -> String {
    Local::now().naive_local().format("%Y-%m-%dT%H:%M").to_string()
}

fn bad(msg: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.into())
}

fn publish(state: &AppState) {
    state.events.publish("safety.presence", (), crate::events::Audience::All, Some("seguridad"));
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub id: String,
    /// Cuenta (si tiene) o nombre de la visita
    pub who: String,
    pub visitor: bool,
    pub affiliation: String,
    pub registered_by: String,
    pub entered_at: String,
    pub left_at: Option<String>,
    pub auto_closed: bool,
}

const SELECT: &str = "SELECT id, COALESCE(username, visitor_name, ''), username IS NULL, affiliation, registered_by, entered_at, left_at, auto_closed FROM safety_presence";

fn row_to_entry(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        who: r.get(1)?,
        visitor: r.get(2)?,
        affiliation: r.get(3)?,
        registered_by: r.get(4)?,
        entered_at: r.get(5)?,
        left_at: r.get(6)?,
        auto_closed: r.get::<_, i64>(7)? != 0,
    })
}

/// Quienes estan adentro ahora
pub fn inside(conn: &Connection) -> rusqlite::Result<Vec<Entry>> {
    let mut stmt = conn.prepare(&format!("{} WHERE left_at IS NULL ORDER BY entered_at", SELECT))?;
    let rows = stmt.query_map([], row_to_entry)?;
    rows.collect()
}

fn open_for_user(conn: &Connection, user: &str) -> rusqlite::Result<Option<Entry>> {
    conn.query_row(&format!("{} WHERE username = ?1 AND left_at IS NULL", SELECT), [user], row_to_entry).optional()
}

#[derive(Debug, Serialize)]
pub struct Overview {
    pub inside: Vec<Entry>,
    pub me: Option<Entry>,
    /// Personas distintas que vinieron hoy
    pub today: i64,
    /// Ultimos 30 dias: (afiliacion, visitas)
    pub by_affiliation: Vec<(String, i64)>,
    /// Ultimos 14 dias: (fecha, personas distintas)
    pub by_day: Vec<(String, i64)>,
    /// Ultima afiliacion que uso la persona (para no escribirla cada vez)
    pub my_affiliation: String,
}

/// GET /api/safety/presence
pub async fn overview(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<Overview>, ApiError> {
    let user = session.username.clone();
    db_op(&state.db, move |conn| {
        let today = Local::now().date_naive();
        let t = today.format("%Y-%m-%d").to_string();
        let count = |sql: &str, p: &str| -> rusqlite::Result<i64> { conn.query_row(sql, [p], |r| r.get(0)) };
        let today_n = count(
            "SELECT COUNT(DISTINCT COALESCE(username, 'v:' || visitor_name)) FROM safety_presence WHERE substr(entered_at, 1, 10) = ?1",
            &t,
        )
        .map_err(|e| e.to_string())?;
        let since30 = (today - Duration::days(29)).format("%Y-%m-%d").to_string();
        let mut stmt = conn
            .prepare(
                "SELECT CASE WHEN affiliation = '' THEN 'Sin indicar' ELSE affiliation END, COUNT(*) FROM safety_presence
                 WHERE substr(entered_at, 1, 10) >= ?1 GROUP BY 1 ORDER BY 2 DESC LIMIT 20",
            )
            .map_err(|e| e.to_string())?;
        let by_aff: Vec<(String, i64)> = stmt
            .query_map([&since30], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        let since14 = (today - Duration::days(13)).format("%Y-%m-%d").to_string();
        let mut stmt = conn
            .prepare(
                "SELECT substr(entered_at, 1, 10), COUNT(DISTINCT COALESCE(username, 'v:' || visitor_name)) FROM safety_presence
                 WHERE substr(entered_at, 1, 10) >= ?1 GROUP BY 1 ORDER BY 1",
            )
            .map_err(|e| e.to_string())?;
        let by_day: Vec<(String, i64)> = stmt
            .query_map([&since14], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        let my_aff: String = conn
            .query_row(
                "SELECT affiliation FROM safety_presence WHERE username = ?1 AND affiliation != '' ORDER BY entered_at DESC LIMIT 1",
                [&user],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        Ok(Overview {
            inside: inside(conn).map_err(|e| e.to_string())?,
            me: open_for_user(conn, &user).map_err(|e| e.to_string())?,
            today: today_n,
            by_affiliation: by_aff,
            by_day,
            my_affiliation: my_aff,
        })
    })
    .await
    .map(Json)
}

#[derive(Deserialize, Default)]
pub struct InReq {
    #[serde(default)]
    pub affiliation: String,
}

fn clean_aff(a: &str) -> Result<String, ApiError> {
    let a = a.trim().to_string();
    if a.chars().count() > 80 {
        return Err(bad("Carrera o afiliacion demasiado larga"));
    }
    Ok(a)
}

/// POST /api/safety/presence/in — «Entre» (si ya estaba adentro, no hace nada)
pub async fn check_in(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<InReq>,
) -> Result<Json<Entry>, ApiError> {
    let aff = clean_aff(&req.affiliation)?;
    let user = session.username.clone();
    let (entry, new) = db_op_status(&state.db, move |conn| {
        if let Some(e) = open_for_user(conn, &user).map_err(internal)? {
            return Ok((e, false));
        }
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        conn.execute(
            "INSERT INTO safety_presence (id, username, affiliation, registered_by, entered_at) VALUES (?1, ?2, ?3, ?2, ?4)",
            params![id, user, aff, now_str()],
        )
        .map_err(internal)?;
        Ok((conn.query_row(&format!("{} WHERE id = ?1", SELECT), [&id], row_to_entry).map_err(internal)?, true))
    })
    .await?;
    if new {
        publish(&state);
    }
    Ok(Json(entry))
}

/// Cierra una entrada abierta
fn close(conn: &Connection, id: &str) -> Result<Entry, ApiError> {
    let n = conn
        .execute("UPDATE safety_presence SET left_at = ?2 WHERE id = ?1 AND left_at IS NULL", params![id, now_str()])
        .map_err(internal)?;
    if n == 0 {
        return Err((StatusCode::CONFLICT, "No estaba adentro".into()));
    }
    conn.query_row(&format!("{} WHERE id = ?1", SELECT), [id], row_to_entry).map_err(internal)
}

/// POST /api/safety/presence/out — «Sali»
pub async fn check_out(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<Entry>, ApiError> {
    let user = session.username.clone();
    let entry = db_op_status(&state.db, move |conn| {
        let open = open_for_user(conn, &user).map_err(internal)?.ok_or((StatusCode::CONFLICT, "No estabas marcado adentro".to_string()))?;
        close(conn, &open.id)
    })
    .await?;
    publish(&state);
    Ok(Json(entry))
}

#[derive(Deserialize)]
pub struct VisitorReq {
    pub name: String,
    #[serde(default)]
    pub affiliation: String,
}

/// POST /api/safety/presence/visitor — registra una visita sin cuenta
pub async fn visitor_in(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<VisitorReq>,
) -> Result<Json<Entry>, ApiError> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(bad("Escribe el nombre de la visita (maximo 80 caracteres)"));
    }
    let aff = clean_aff(&req.affiliation)?;
    let user = session.username.clone();
    let entry = db_op_status(&state.db, move |conn| {
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        conn.execute(
            "INSERT INTO safety_presence (id, visitor_name, affiliation, registered_by, entered_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, aff, user, now_str()],
        )
        .map_err(internal)?;
        conn.query_row(&format!("{} WHERE id = ?1", SELECT), [&id], row_to_entry).map_err(internal)
    })
    .await?;
    state.log_activity("Seguridad", &format!("Registro la visita de {}", entry.who), &session.username).await;
    publish(&state);
    Ok(Json(entry))
}

/// POST /api/safety/presence/{id}/out — marca la salida de otra persona o visita: quien
/// gestiona Seguridad, o quien registro la visita
pub async fn mark_out(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<Entry>, ApiError> {
    let staff = session.can("seguridad", Level::Gestionar);
    let user = session.username.clone();
    let entry = db_op_status(&state.db, move |conn| {
        let e = conn
            .query_row(&format!("{} WHERE id = ?1", SELECT), [&id], row_to_entry)
            .optional()
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Registro no encontrado".to_string()))?;
        let own = (!e.visitor && e.who == user) || (e.visitor && e.registered_by == user);
        if !staff && !own {
            return Err((StatusCode::FORBIDDEN, "Solo quien gestiona Seguridad marca la salida de otra persona".into()));
        }
        close(conn, &id)
    })
    .await?;
    if entry.who != session.username {
        state.log_activity("Seguridad", &format!("Marco la salida de {}", entry.who), &session.username).await;
    }
    publish(&state);
    Ok(Json(entry))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// GET /api/safety/presence/history?from=&to= — registro completo (gestionar), maximo 2000
pub async fn history(State(state): State<AppState>, Query(q): Query<HistoryQuery>) -> Result<Json<Vec<Entry>>, ApiError> {
    let today = Local::now().date_naive();
    let from = q.from.unwrap_or_else(|| (today - Duration::days(30)).format("%Y-%m-%d").to_string());
    let to = q.to.unwrap_or_else(|| today.format("%Y-%m-%d").to_string());
    db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare(&format!(
                "{} WHERE substr(entered_at, 1, 10) BETWEEN ?1 AND ?2 ORDER BY entered_at DESC LIMIT 2000",
                SELECT
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([&from, &to], row_to_entry).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct QrQuery {
    pub base: String,
}

#[derive(Serialize)]
pub struct QrResp {
    pub url: String,
    pub svg: String,
}

/// GET /api/safety/presence/qr?base=<origen> — QR para pegar en la puerta
pub async fn door_qr(Query(q): Query<QrQuery>) -> Result<Json<QrResp>, ApiError> {
    let origin = crate::handlers::machines::clean_origin(&q.base)?;
    let url = format!("{}/safety?entrada=1", origin);
    let svg = crate::totp::qr_svg(&url).map_err(internal)?;
    Ok(Json(QrResp { url, svg }))
}

/// Cierra lo que quedo abierto de dias anteriores (a las 23:59 de ese dia)
pub fn close_stale(conn: &Connection) -> rusqlite::Result<usize> {
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    conn.execute(
        "UPDATE safety_presence SET left_at = substr(entered_at, 1, 10) || 'T23:59', auto_closed = 1
         WHERE left_at IS NULL AND substr(entered_at, 1, 10) < ?1",
        [today],
    )
}

/// Cada hora: cierra entradas de dias anteriores que nadie marco como salida
pub async fn close_loop(state: AppState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        tick.tick().await;
        if let Ok(n) = db_op(&state.db, |conn| close_stale(conn).map_err(|e| e.to_string())).await {
            if n > 0 {
                publish(&state);
            }
        }
    }
}
