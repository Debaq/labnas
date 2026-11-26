//! Manual de procedimientos con historial de versiones (idea 67). Cada version es inmutable
//! (triggers en la base): una nueva version es un registro nuevo con su resumen de cambios.
//! Publicar (Gestionar) y firmar (Ver, quien tenga acceso a Finanzas) quedan en la bitacora
//! con la huella del texto.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{bad, clean, conflict, db, esc_md, forbidden, not_found};
use crate::db::db_op_status;
use crate::error::ApiError;
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

const MAX_BODY: usize = 100_000;

#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    pub id: String,
    pub version: i64,
    pub title: String,
    pub summary: String,
    pub sha256: String,
    pub published_by: String,
    pub published_at: String,
    pub ledger_seq: Option<i64>,
    pub signatures: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionFull {
    #[serde(flatten)]
    pub info: VersionInfo,
    pub body: String,
}

const COLS: &str = "v.id, v.version, v.title, v.summary, v.sha256, v.published_by, v.published_at, v.ledger_seq,
    (SELECT COUNT(*) FROM fin_manual_signatures s WHERE s.version_id = v.id), v.body";

fn read_full(r: &rusqlite::Row) -> rusqlite::Result<VersionFull> {
    Ok(VersionFull {
        info: VersionInfo {
            id: r.get(0)?,
            version: r.get(1)?,
            title: r.get(2)?,
            summary: r.get(3)?,
            sha256: r.get(4)?,
            published_by: r.get(5)?,
            published_at: r.get(6)?,
            ledger_seq: r.get(7)?,
            signatures: r.get(8)?,
        },
        body: r.get(9)?,
    })
}

fn latest(conn: &Connection) -> Result<Option<VersionFull>, ApiError> {
    conn.query_row(&format!("SELECT {} FROM fin_manual_versions v ORDER BY v.version DESC LIMIT 1", COLS), [], read_full)
        .optional()
        .map_err(db)
}

#[derive(Debug, Serialize)]
pub struct Signature {
    pub username: String,
    pub signed_at: String,
}

#[derive(Debug, Serialize)]
pub struct SignStatus {
    pub version: Option<VersionInfo>,
    /// Quienes deben firmar (acceso a Finanzas)
    pub required: Vec<String>,
    pub signed: Vec<Signature>,
    pub pending: Vec<String>,
}

fn status_of(conn: &Connection, v: Option<&VersionInfo>) -> Result<SignStatus, ApiError> {
    let Some(v) = v else { return Ok(SignStatus { version: None, required: vec![], signed: vec![], pending: vec![] }) };
    let required = crate::roles::users_with_level(conn, "finanzas", Level::Ver).map_err(db)?;
    let mut stmt = conn.prepare("SELECT username, signed_at FROM fin_manual_signatures WHERE version_id = ?1 ORDER BY signed_at").map_err(db)?;
    let signed: Vec<Signature> = stmt
        .query_map([&v.id], |r| Ok(Signature { username: r.get(0)?, signed_at: r.get(1)? }))
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)?;
    let mut pending: Vec<String> = required.iter().filter(|u| !signed.iter().any(|s| &s.username == *u)).cloned().collect();
    pending.sort();
    Ok(SignStatus { version: Some(v.clone()), required, signed, pending })
}

#[derive(Debug, Serialize)]
pub struct ManualView {
    pub current: Option<VersionFull>,
    pub versions: Vec<VersionInfo>,
    /// Cuando firmo la persona que consulta la version vigente
    pub my_signed_at: Option<String>,
    pub pending_count: usize,
}

/// GET /api/finance/manual — version vigente, historial y si ya la firme
pub async fn current(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<ManualView>, ApiError> {
    let me = session.username.clone();
    Ok(Json(
        db_op_status(&state.db, move |conn| {
            let current = latest(conn)?;
            let mut stmt = conn.prepare(&format!("SELECT {} FROM fin_manual_versions v ORDER BY v.version DESC", COLS)).map_err(db)?;
            let versions: Vec<VersionInfo> = stmt.query_map([], read_full).map_err(db)?.map(|v| v.map(|v| v.info)).collect::<Result<_, _>>().map_err(db)?;
            let my_signed_at = match &current {
                Some(c) => conn
                    .query_row("SELECT signed_at FROM fin_manual_signatures WHERE version_id = ?1 AND username = ?2", params![c.info.id, me], |r| r.get(0))
                    .optional()
                    .map_err(db)?,
                None => None,
            };
            let pending_count = status_of(conn, current.as_ref().map(|c| &c.info))?.pending.len();
            Ok(ManualView { current, versions, my_signed_at, pending_count })
        })
        .await?,
    ))
}

/// GET /api/finance/manual/versions/{id} — una version con su texto
pub async fn version(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<VersionFull>, ApiError> {
    let v = db_op_status(&state.db, move |conn| {
        conn.query_row(&format!("SELECT {} FROM fin_manual_versions v WHERE v.id = ?1", COLS), [&id], read_full).optional().map_err(db)
    })
    .await?;
    v.map(Json).ok_or_else(|| not_found("Version no encontrada"))
}

#[derive(Debug, Deserialize)]
pub struct SigQuery {
    version_id: Option<String>,
}

/// GET /api/finance/manual/signatures — quien firmo y quien falta (vigente, o la `version_id`)
pub async fn signatures(State(state): State<AppState>, Query(q): Query<SigQuery>) -> Result<Json<SignStatus>, ApiError> {
    Ok(Json(
        db_op_status(&state.db, move |conn| {
            let v = match &q.version_id {
                Some(id) => conn
                    .query_row(&format!("SELECT {} FROM fin_manual_versions v WHERE v.id = ?1", COLS), [id], read_full)
                    .optional()
                    .map_err(db)?
                    .ok_or_else(|| not_found("Version no encontrada"))?,
                None => match latest(conn)? {
                    Some(v) => v,
                    None => return status_of(conn, None),
                },
            };
            status_of(conn, Some(&v.info))
        })
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    title: Option<String>,
    body: String,
    summary: Option<String>,
}

/// POST /api/finance/manual/versions — publica una version nueva (Gestionar)
pub async fn publish(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<PublishRequest>,
) -> Result<(StatusCode, Json<VersionFull>), ApiError> {
    if !session.can("finanzas", Level::Gestionar) {
        return Err(forbidden("Solo quien gestiona Finanzas publica el manual"));
    }
    let body = req.body.trim().to_string();
    if body.chars().count() < 20 {
        return Err(bad("El manual esta vacio o es demasiado corto"));
    }
    if body.chars().count() > MAX_BODY {
        return Err(bad(format!("El manual es demasiado largo (maximo {} caracteres)", MAX_BODY)));
    }
    let title = clean(req.title.as_deref().unwrap_or(""), 200, "Titulo")?;
    let title = if title.is_empty() { "Manual de procedimientos de Finanzas".to_string() } else { title };
    let summary = clean(req.summary.as_deref().unwrap_or(""), 1000, "Resumen")?;
    let user = session.username.clone();
    let sha = crate::ledger::sha256_hex(body.as_bytes());

    let (u2, t2, s2, b2, sha2) = (user.clone(), title.clone(), summary.clone(), body.clone(), sha.clone());
    let v = db_op_status(&state.db, move |conn| {
        let n: i64 = conn.query_row("SELECT COALESCE(MAX(version), 0) FROM fin_manual_versions", [], |r| r.get(0)).map_err(db)?;
        if n > 0 && s2.is_empty() {
            return Err(bad("Indique el resumen de cambios de esta version"));
        }
        let summary = if s2.is_empty() { "Version inicial".to_string() } else { s2 };
        if let Some(last) = latest(conn)? {
            if last.info.sha256 == sha2 {
                return Err(conflict("El texto es igual al de la version vigente"));
            }
        }
        let id = crate::finance::new_id();
        conn.execute(
            "INSERT INTO fin_manual_versions (id, version, title, body, summary, sha256, published_by, published_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, n + 1, t2, b2, summary, sha2, u2, crate::finance::now()],
        )
        .map_err(|_| conflict("Otra persona publico una version al mismo tiempo; reintente"))?;
        conn.query_row(&format!("SELECT {} FROM fin_manual_versions v WHERE v.id = ?1", COLS), [&id], read_full).map_err(db)
    })
    .await?;

    let seq = crate::finance::ledger(
        &state,
        &user,
        "manual",
        &format!("manual v{}", v.info.version),
        json!({ "version": v.info.version, "titulo": title, "resumen": v.info.summary, "sha256": sha }),
    )
    .await?;
    let id = v.info.id.clone();
    db_op_status(&state.db, move |conn| {
        conn.execute("UPDATE fin_manual_versions SET ledger_seq = ?1 WHERE id = ?2 AND ledger_seq IS NULL", params![seq, id]).map_err(db)?;
        Ok(())
    })
    .await?;
    let mut v = v;
    v.info.ledger_seq = Some(seq);
    state.log_activity("Finanzas", &format!("Manual de procedimientos v{} publicado", v.info.version), &user).await;
    state.events.publish("finance", (), crate::events::Audience::All, Some("finanzas"));

    // Aviso por Telegram a quienes deben firmar
    let (version, summary) = (v.info.version, v.info.summary.clone());
    let st = state.clone();
    let publisher = user.clone();
    let targets = db_op_status(&state.db, move |conn| crate::roles::users_with_level(conn, "finanzas", Level::Ver).map_err(db)).await?;
    let targets: Vec<String> = targets.into_iter().filter(|u| *u != publisher).collect();
    tokio::spawn(async move {
        let text = format!(
            "📘 *Manual de procedimientos de Finanzas, version {}*\n{}\nPublicado por {}. Leelo y firmalo en Finanzas > Rendicion y auditoria > Manual.",
            version,
            esc_md(&summary),
            esc_md(&publisher)
        );
        crate::handlers::notifications::notify_users(&st, &targets, &text).await;
    });
    Ok((StatusCode::CREATED, Json(v)))
}

#[derive(Debug, Deserialize)]
pub struct SignRequest {
    version_id: Option<String>,
}

/// POST /api/finance/manual/sign — la persona confirma que leyo la version vigente (Ver)
pub async fn sign(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<SignRequest>,
) -> Result<Json<SignStatus>, ApiError> {
    if !session.can("finanzas", Level::Ver) {
        return Err(forbidden("Sin acceso a Finanzas"));
    }
    let user = session.username.clone();
    let (u2, vid) = (user.clone(), req.version_id.clone());
    let (v, status) = db_op_status(&state.db, move |conn| {
        let v = latest(conn)?.ok_or_else(|| not_found("Todavia no hay manual publicado"))?;
        if vid.as_deref().is_some_and(|id| id != v.info.id) {
            return Err(conflict("Hay una version mas nueva del manual: leala y firme esa"));
        }
        let done = conn
            .execute(
                "INSERT OR IGNORE INTO fin_manual_signatures (version_id, username, signed_at) VALUES (?1, ?2, ?3)",
                params![v.info.id, u2, crate::finance::now()],
            )
            .map_err(db)?;
        if done == 0 {
            return Err(conflict("Ya firmo esta version del manual"));
        }
        let st = status_of(conn, Some(&v.info))?;
        Ok((v, st))
    })
    .await?;
    let seq = crate::finance::ledger(
        &state,
        &user,
        "manual.firma",
        &format!("manual v{}", v.info.version),
        json!({ "version": v.info.version, "sha256": v.info.sha256, "persona": user }),
    )
    .await?;
    let (vid, u2) = (v.info.id.clone(), user.clone());
    db_op_status(&state.db, move |conn| {
        conn.execute("UPDATE fin_manual_signatures SET ledger_seq = ?1 WHERE version_id = ?2 AND username = ?3", params![seq, vid, u2]).map_err(db)?;
        Ok(())
    })
    .await?;
    state.log_activity("Finanzas", &format!("Firmo el manual v{}", v.info.version), &user).await;
    state.events.publish("finance", (), crate::events::Audience::All, Some("finanzas"));
    Ok(Json(status))
}
