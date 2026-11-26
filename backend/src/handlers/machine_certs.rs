//! Induccion por maquina (idea 21). Si la maquina la exige, solo reserva quien tiene la
//! induccion vigente; el staff no la necesita. La persona la pide desde la ficha, el
//! responsable y el staff reciben el aviso, y un operador la da (con vencimiento opcional)
//! o la quita. Los vencimientos aparecen en Plazos.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::db_op_status;
use crate::events::{Audience, Level};
use crate::handlers::machines::{bad, conflict, db_err, forbidden, is_staff, load_machine, md, not_found, publish, MODULE};
use crate::state::{AppState, SessionInfo};
use crate::error::ApiError;

fn today() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct Certification {
    pub machine_id: String,
    pub machine_name: String,
    pub username: String,
    /// solicitada | vigente | vencida
    pub status: String,
    pub requested_at: Option<String>,
    pub granted_by: Option<String>,
    pub granted_at: Option<String>,
    /// `YYYY-MM-DD`; sin fecha no vence
    pub expires_on: Option<String>,
    pub notes: String,
}

const SELECT: &str = "SELECT c.machine_id, COALESCE(m.name, ''), c.username, c.status, c.requested_at, c.granted_by, c.granted_at, c.expires_on, c.notes
    FROM machine_certifications c LEFT JOIN machines m ON m.id = c.machine_id";

fn row_to_cert(r: &rusqlite::Row) -> rusqlite::Result<Certification> {
    let mut c = Certification {
        machine_id: r.get(0)?,
        machine_name: r.get(1)?,
        username: r.get(2)?,
        status: r.get(3)?,
        requested_at: r.get(4)?,
        granted_by: r.get(5)?,
        granted_at: r.get(6)?,
        expires_on: r.get(7)?,
        notes: r.get(8)?,
    };
    if c.status == "vigente" && c.expires_on.as_deref().is_some_and(|e| e < today().as_str()) {
        c.status = "vencida".into();
    }
    Ok(c)
}

/// Induccion vigente (y no vencida) de esa persona en esa maquina
pub fn is_certified(conn: &Connection, machine_id: &str, username: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT 1 FROM machine_certifications
         WHERE machine_id = ?1 AND username = ?2 AND status = 'vigente' AND (expires_on IS NULL OR expires_on >= ?3)",
        params![machine_id, username, today()],
        |_| Ok(()),
    )
    .optional()
    .map(|r| r.is_some())
}

fn load_cert(conn: &Connection, machine_id: &str, username: &str) -> rusqlite::Result<Option<Certification>> {
    conn.query_row(&format!("{} WHERE c.machine_id = ?1 AND c.username = ?2", SELECT), params![machine_id, username], row_to_cert)
        .optional()
}

fn query(conn: &Connection, filter: &str, args: &[&dyn rusqlite::ToSql]) -> rusqlite::Result<Vec<Certification>> {
    let mut stmt = conn.prepare(&format!("{} WHERE {} ORDER BY m.name, c.username", SELECT, filter))?;
    let rows = stmt.query_map(args, row_to_cert)?;
    rows.collect()
}

/// Quienes gestionan Maquinas (y los admin)
fn staff_users(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    crate::roles::users_with_level(conn, "machines", crate::roles::Level::Gestionar)
}

fn tell(state: &AppState, users: Vec<String>, title: &str, body: &str) {
    for u in &users {
        crate::events::notify(state, Audience::User(u.clone()), MODULE, Level::Info, title, body);
    }
    let text = format!("🎓 *{}*\n{}", md(title), md(body));
    let st = state.clone();
    tokio::spawn(async move {
        crate::handlers::notifications::notify_users(&st, &users, &text).await;
    });
}

// ─── Handlers ───

/// GET /api/machines/{id}/certifications
pub async fn list_for_machine(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Vec<Certification>>, ApiError> {
    let list = db_op_status(&state.db, move |conn| query(conn, "c.machine_id = ?1", &[&id]).map_err(db_err)).await?;
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub mine: bool,
    #[serde(default)]
    pub status: Option<String>,
}

/// GET /api/machines/certifications?mine=true | ?status=solicitada
pub async fn list_all(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Certification>>, ApiError> {
    let list = db_op_status(&state.db, move |conn| {
        let mut list = if q.mine {
            query(conn, "c.username = ?1", &[&session.username])
        } else {
            query(conn, "1 = 1", &[])
        }
        .map_err(db_err)?;
        if let Some(s) = q.status {
            list.retain(|c| c.status == s);
        }
        Ok(list)
    })
    .await?;
    Ok(Json(list))
}

#[derive(Deserialize, Default)]
pub struct RequestReq {
    #[serde(default)]
    pub notes: String,
}

/// POST /api/machines/{id}/certifications/request: pedir la induccion
pub async fn request(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    body: Option<Json<RequestReq>>,
) -> Result<(StatusCode, Json<Certification>), ApiError> {
    let notes = body.map(|b| b.0.notes).unwrap_or_default().trim().to_string();
    if notes.chars().count() > 500 {
        return Err(bad("Nota demasiado larga"));
    }
    let user = session.username.clone();
    let (cert, machine, mut to) = db_op_status(&state.db, move |conn| {
        let m = load_machine(conn, &id).map_err(db_err)?.ok_or_else(not_found)?;
        match load_cert(conn, &id, &user).map_err(db_err)? {
            Some(c) if c.status == "vigente" => return Err(conflict("Ya tienes la induccion de esta maquina")),
            Some(c) if c.status == "solicitada" => return Err(conflict("Ya la pediste; te avisaremos cuando te la den")),
            _ => {}
        }
        // Vencida o sin registro: queda solicitada
        conn.execute(
            "INSERT INTO machine_certifications (machine_id, username, status, requested_at, notes) VALUES (?1, ?2, 'solicitada', ?3, ?4)
             ON CONFLICT(machine_id, username) DO UPDATE SET status = 'solicitada', requested_at = ?3, notes = ?4",
            params![id, user, chrono::Utc::now().to_rfc3339(), notes],
        )
        .map_err(db_err)?;
        let mut to = staff_users(conn).map_err(db_err)?;
        to.extend(m.responsible.clone());
        let c = load_cert(conn, &id, &user).map_err(db_err)?.ok_or_else(not_found)?;
        Ok((c, m, to))
    })
    .await?;
    to.sort();
    to.dedup();
    to.retain(|u| u != &session.username);
    state.log_activity("Maquinas", &format!("Pide induccion: {}", machine.name), &session.username).await;
    let body = if cert.notes.is_empty() {
        format!("{} pide la induccion de {}", cert.username, machine.name)
    } else {
        format!("{} pide la induccion de {}: {}", cert.username, machine.name, cert.notes)
    };
    tell(&state, to, "Induccion pedida", &body);
    publish(&state);
    Ok((StatusCode::CREATED, Json(cert)))
}

#[derive(Deserialize)]
pub struct GrantReq {
    pub username: String,
    #[serde(default)]
    pub expires_on: Option<String>,
    #[serde(default)]
    pub notes: String,
}

/// POST /api/machines/{id}/certifications (operador): dar o renovar la induccion
pub async fn grant(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<GrantReq>,
) -> Result<Json<Certification>, ApiError> {
    let username = req.username.trim().to_string();
    let expires = match req.expires_on.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| bad("Fecha de vencimiento invalida (AAAA-MM-DD)"))?;
            let d = d.format("%Y-%m-%d").to_string();
            if d < today() {
                return Err(bad("La fecha de vencimiento ya paso"));
            }
            Some(d)
        }
        None => None,
    };
    let notes = req.notes.trim().to_string();
    if notes.chars().count() > 500 {
        return Err(bad("Nota demasiado larga"));
    }
    let by = session.username.clone();
    let (cert, machine) = db_op_status(&state.db, move |conn| {
        let m = load_machine(conn, &id).map_err(db_err)?.ok_or_else(not_found)?;
        let role: Option<String> = conn
            .query_row("SELECT role FROM web_users WHERE username = ?1", [&username], |r| r.get(0))
            .optional()
            .map_err(db_err)?;
        match role.as_deref() {
            None => return Err(bad("Ese usuario no existe")),
            Some("pendiente") => return Err(bad("La cuenta de ese usuario aun no esta aprobada")),
            _ => {}
        }
        conn.execute(
            "INSERT INTO machine_certifications (machine_id, username, status, granted_by, granted_at, expires_on, notes)
             VALUES (?1, ?2, 'vigente', ?3, ?4, ?5, ?6)
             ON CONFLICT(machine_id, username) DO UPDATE SET status = 'vigente', granted_by = ?3, granted_at = ?4, expires_on = ?5,
             notes = CASE WHEN ?6 = '' THEN notes ELSE ?6 END",
            params![id, username, by, chrono::Utc::now().to_rfc3339(), expires, notes],
        )
        .map_err(db_err)?;
        let c = load_cert(conn, &id, &username).map_err(db_err)?.ok_or_else(not_found)?;
        Ok((c, m))
    })
    .await?;
    state
        .log_activity("Maquinas", &format!("Induccion de {} en {}", cert.username, machine.name), &session.username)
        .await;
    if cert.username != session.username {
        let until = cert.expires_on.as_deref().map(|e| format!(" (vigente hasta el {})", e)).unwrap_or_default();
        tell(&state, vec![cert.username.clone()], "Induccion lista", &format!("Ya puedes reservar {}{}", machine.name, until));
    }
    publish(&state);
    Ok(Json(cert))
}

/// DELETE /api/machines/{id}/certifications/{username}: quitar la induccion o rechazar la
/// solicitud (operador); cada persona puede retirar su propia solicitud
pub async fn revoke(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path((id, username)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let s = session.clone();
    let (cert, machine) = db_op_status(&state.db, move |conn| {
        let m = load_machine(conn, &id).map_err(db_err)?.ok_or_else(not_found)?;
        let c = load_cert(conn, &id, &username)
            .map_err(db_err)?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "No hay induccion ni solicitud".to_string()))?;
        let own_request = c.username == s.username && c.status == "solicitada";
        if !is_staff(&s) && !own_request {
            return Err(forbidden("Solo administradores y operadores quitan inducciones"));
        }
        conn.execute("DELETE FROM machine_certifications WHERE machine_id = ?1 AND username = ?2", params![id, username])
            .map_err(db_err)?;
        Ok((c, m))
    })
    .await?;
    let what = if cert.status == "solicitada" { "Solicitud de induccion retirada" } else { "Induccion quitada" };
    state
        .log_activity("Maquinas", &format!("{}: {} en {}", what, cert.username, machine.name), &session.username)
        .await;
    if cert.username != session.username {
        let title = if cert.status == "solicitada" { "Induccion no aprobada" } else { "Induccion quitada" };
        tell(&state, vec![cert.username.clone()], title, &machine.name);
    }
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}
