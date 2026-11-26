//! Modulo Transparencia: portal publico generado desde los datos reales y solicitudes de
//! informacion con plazo (ideas 70 y 71). Viene apagado. Ver docs/TRANSPARENCIA.md.
//!
//! Lo publico (sin sesion) vive solo bajo `/api/transparency/public/`; apagado el modulo,
//! esas rutas responden 404 (lo hace el middleware).

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::json;

use crate::db::db_op_status;
use crate::error::{internal, ApiError};
use crate::handlers::machines::{bad, conflict, db_err, forbidden};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

pub mod portal;
pub mod requests;

use portal::Sections;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Publico (sin sesion)
        .route("/api/transparency/public/portal", get(public_portal))
        .route("/api/transparency/public/informes/{id}", get(public_report_file))
        .route("/api/transparency/public/solicitudes", post(requests::create_public))
        .route("/api/transparency/public/solicitudes/consulta", post(requests::lookup_public))
        // Gestion
        .route("/api/transparency/config", get(get_config).put(put_config))
        .route("/api/transparency/preview", post(preview))
        .route("/api/transparency/reports", get(list_reports).post(create_report))
        .route("/api/transparency/reports/{id}/file", get(report_file))
        .route("/api/transparency/reports/{id}/publish", post(publish_report))
        .route("/api/transparency/reports/{id}/withdraw", post(withdraw_report))
        .route("/api/transparency/requests", get(requests::list))
        .route("/api/transparency/requests/{id}", get(requests::detail).put(requests::update))
}

fn publish_event(state: &AppState) {
    state.events.publish("transparency", (), crate::events::Audience::All, Some("transparencia"));
}

fn need_manage(s: &SessionInfo) -> Result<(), ApiError> {
    if s.can("transparencia", Level::Gestionar) {
        Ok(())
    } else {
        Err(forbidden("Solo quien gestiona Transparencia puede hacerlo"))
    }
}

// ─── Publico ───

/// GET /api/transparency/public/portal: solo las secciones activas
pub async fn public_portal(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let v = db_op_status(&state.db, |conn| {
        let c = portal::load_config(conn);
        portal::build(conn, &c.sections, &c.intro, c.deadline_days).map_err(db_err)
    })
    .await?;
    Ok(Json(v))
}

/// Un informe publicado, por su id. Solo si esta publicado; no se abre /api/finance/files.
async fn serve_report(state: &AppState, id: String, only_published: bool) -> Result<Response, ApiError> {
    let found: Option<(String, String, Vec<u8>)> = db_op_status(&state.db, move |conn| {
        conn.query_row(
            "SELECT f.name, f.mime, f.data FROM transp_reports r JOIN fin_files f ON f.id = r.file_id
              WHERE r.id = ?1 AND (r.published = 1 OR ?2 = 0)",
            params![id, only_published as i64],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(db_err)
    })
    .await?;
    let (name, mime, data) = found.ok_or((StatusCode::NOT_FOUND, "Informe no encontrado".to_string()))?;
    let inline = mime.starts_with("image/") || mime == "application/pdf";
    let safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ')).collect();
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_DISPOSITION, format!("{}; filename=\"{}\"", if inline { "inline" } else { "attachment" }, safe))
        .header(header::CACHE_CONTROL, if only_published { "public, max-age=300" } else { "private, max-age=300" })
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(data))
        .map_err(internal)
}

/// GET /api/transparency/public/informes/{id}
pub async fn public_report_file(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, ApiError> {
    serve_report(&state, id, true).await
}

/// GET /api/transparency/reports/{id}/file (Gestionar): tambien los retirados
pub async fn report_file(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>) -> Result<Response, ApiError> {
    need_manage(&session)?;
    serve_report(&state, id, false).await
}

// ─── Configuracion y vista previa ───

fn managers_of(conn: &rusqlite::Connection) -> Vec<String> {
    crate::roles::users_with_level(conn, "transparencia", Level::Gestionar).unwrap_or_default()
}

/// GET /api/transparency/config
pub async fn get_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let v = db_op_status(&state.db, |conn| {
        let c = portal::load_config(conn);
        Ok(json!({ "sections": c.sections, "deadline_days": c.deadline_days, "intro": c.intro, "managers": managers_of(conn), "available": portal::SECTIONS }))
    })
    .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct ConfigReq {
    pub sections: Sections,
    #[serde(default)]
    pub deadline_days: Option<i64>,
    #[serde(default)]
    pub intro: Option<String>,
}

fn check_sections(s: &Sections) -> Result<Sections, ApiError> {
    if let Some(k) = s.keys().find(|k| !portal::SECTIONS.contains(&k.as_str())) {
        return Err(bad(format!("Seccion desconocida: {}", k)));
    }
    Ok(portal::normalize(s))
}

fn check_intro(s: &str) -> Result<String, ApiError> {
    let s = s.trim().to_string();
    if s.chars().count() > 1500 {
        return Err(bad("La introduccion es demasiado larga (maximo 1500 caracteres)"));
    }
    Ok(s)
}

/// PUT /api/transparency/config (Gestionar): que se publica y el plazo de las solicitudes
pub async fn put_config(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Json(req): Json<ConfigReq>) -> Result<Json<serde_json::Value>, ApiError> {
    let sections = check_sections(&req.sections)?;
    if req.deadline_days.is_some_and(|d| !(1..=90).contains(&d)) {
        return Err(bad("El plazo debe estar entre 1 y 90 dias habiles"));
    }
    let intro = req.intro.as_deref().map(check_intro).transpose()?;
    let (user, new_sections, days) = (session.username.clone(), sections.clone(), req.deadline_days);
    let (before, after) = db_op_status(&state.db, move |conn| {
        let before = portal::load_config(conn);
        let deadline = days.unwrap_or(before.deadline_days);
        let intro = intro.unwrap_or_else(|| before.intro.clone());
        conn.execute(
            "INSERT INTO transp_config (id, sections, deadline_days, intro, updated_by, updated_at) VALUES (1, ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET sections = ?1, deadline_days = ?2, intro = ?3, updated_by = ?4, updated_at = ?5",
            params![serde_json::to_string(&new_sections).unwrap_or_default(), deadline, intro, user, crate::finance::now()],
        )
        .map_err(db_err)?;
        Ok((before, portal::load_config(conn)))
    })
    .await?;
    let changed = before.sections != after.sections || before.deadline_days != after.deadline_days || before.intro != after.intro;
    if changed {
        crate::finance::ledger(
            &state,
            &session.username,
            "transparencia",
            "secciones",
            json!({
                "antes": before.sections, "despues": after.sections,
                "plazo_dias": { "antes": before.deadline_days, "despues": after.deadline_days },
                "introduccion_cambiada": before.intro != after.intro,
            }),
        )
        .await?;
        state.log_activity("Transparencia", "Configuracion del portal cambiada", &session.username).await;
    }
    publish_event(&state);
    Ok(Json(json!({ "sections": after.sections, "deadline_days": after.deadline_days, "intro": after.intro })))
}

/// POST /api/transparency/preview (Gestionar): el portal tal como saldria con esas secciones,
/// sin guardar. Es el mismo constructor que la pagina publica.
pub async fn preview(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Json(req): Json<ConfigReq>) -> Result<Json<serde_json::Value>, ApiError> {
    need_manage(&session)?;
    let sections = check_sections(&req.sections)?;
    let intro = req.intro.as_deref().map(check_intro).transpose()?;
    let v = db_op_status(&state.db, move |conn| {
        let c = portal::load_config(conn);
        let intro = intro.unwrap_or(c.intro);
        portal::build(conn, &sections, &intro, req.deadline_days.unwrap_or(c.deadline_days)).map_err(db_err)
    })
    .await?;
    Ok(Json(v))
}

// ─── Informes publicados ───

#[derive(Deserialize)]
pub struct FileUpload {
    pub name: String,
    pub data: String,
}

#[derive(Deserialize)]
pub struct ReportReq {
    pub title: String,
    /// AAAA-MM-DD (hoy si falta)
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    pub file: FileUpload,
    /// Publicarlo de inmediato (por defecto si)
    #[serde(default = "yes")]
    pub publish: bool,
}

fn yes() -> bool {
    true
}

/// GET /api/transparency/reports (Gestionar): todos, tambien los retirados
pub async fn list_reports(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    need_manage(&session)?;
    let list = db_op_status(&state.db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT r.id, r.title, r.report_date, r.project_id, p.title, r.published, f.name, f.size, f.sha256, r.created_by, r.created_at, r.changed_by, r.changed_at
                   FROM transp_reports r JOIN fin_files f ON f.id = r.file_id LEFT JOIN portfolio_entries p ON p.id = r.project_id
                  ORDER BY r.report_date DESC, r.created_at DESC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "date": r.get::<_, String>(2)?,
                    "project_id": r.get::<_, Option<String>>(3)?, "project": r.get::<_, Option<String>>(4)?,
                    "published": r.get::<_, i64>(5)? != 0, "file_name": r.get::<_, String>(6)?, "size": r.get::<_, i64>(7)?,
                    "sha256": r.get::<_, String>(8)?, "created_by": r.get::<_, String>(9)?, "created_at": r.get::<_, String>(10)?,
                    "changed_by": r.get::<_, String>(11)?, "changed_at": r.get::<_, String>(12)?,
                }))
            })
            .map_err(db_err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
    })
    .await?;
    Ok(Json(list))
}

/// POST /api/transparency/reports (Gestionar): sube un informe y lo publica
pub async fn create_report(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Json(req): Json<ReportReq>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let title = req.title.trim().to_string();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(bad("El titulo es obligatorio (maximo 200 caracteres)"));
    }
    let date = match req.date.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| bad("Fecha invalida (AAAA-MM-DD)"))?.format("%Y-%m-%d").to_string(),
        None => chrono::Local::now().date_naive().format("%Y-%m-%d").to_string(),
    };
    let (bytes, mime) = crate::finance::decode_upload(&req.file.data)?;
    let (user, name, project, publish) = (session.username.clone(), req.file.name.clone(), req.project_id.clone().filter(|p| !p.trim().is_empty()), req.publish);
    let (id, t) = (crate::finance::new_id(), title.clone());
    let (file_ref, id) = db_op_status(&state.db, move |conn| {
        if let Some(p) = &project {
            if crate::finance::project_title(conn, p).map_err(db_err)?.is_none() {
                return Err(bad("El proyecto no existe"));
            }
        }
        let f = crate::finance::store_file(conn, &name, &bytes, mime, &user).map_err(db_err)?;
        let now = crate::finance::now();
        conn.execute(
            "INSERT INTO transp_reports (id, title, report_date, project_id, file_id, published, created_by, created_at, changed_by, changed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?7, ?8)",
            params![id, t, date, project, f.id, publish as i64, user, now],
        )
        .map_err(db_err)?;
        Ok((f, id))
    })
    .await?;
    crate::finance::ledger(
        &state,
        &session.username,
        "transparencia",
        &format!("informe:{}", id),
        json!({ "accion": if req.publish { "publicado" } else { "subido sin publicar" }, "titulo": title, "archivo": file_ref.name, "sha256": file_ref.sha256 }),
    )
    .await?;
    state.log_activity("Transparencia", &format!("Informe subido: {}", title), &session.username).await;
    publish_event(&state);
    Ok((StatusCode::CREATED, Json(json!({ "id": id, "published": req.publish, "sha256": file_ref.sha256 }))))
}

async fn set_published(state: &AppState, session: &SessionInfo, id: String, published: bool) -> Result<Json<serde_json::Value>, ApiError> {
    need_manage(session)?;
    let (user, rid) = (session.username.clone(), id.clone());
    let (title, sha) = db_op_status(&state.db, move |conn| {
        let (title, was, sha): (String, i64, String) = conn
            .query_row(
                "SELECT r.title, r.published, f.sha256 FROM transp_reports r JOIN fin_files f ON f.id = r.file_id WHERE r.id = ?1",
                [&rid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(db_err)?
            .ok_or((StatusCode::NOT_FOUND, "Informe no encontrado".to_string()))?;
        if (was != 0) == published {
            return Err(conflict(if published { "El informe ya esta publicado" } else { "El informe ya esta retirado" }));
        }
        conn.execute(
            "UPDATE transp_reports SET published = ?2, changed_by = ?3, changed_at = ?4 WHERE id = ?1",
            params![rid, published as i64, user, crate::finance::now()],
        )
        .map_err(db_err)?;
        Ok((title, sha))
    })
    .await?;
    crate::finance::ledger(
        state,
        &session.username,
        "transparencia",
        &format!("informe:{}", id),
        json!({ "accion": if published { "publicado" } else { "retirado" }, "titulo": title, "sha256": sha }),
    )
    .await?;
    state
        .log_activity("Transparencia", &format!("Informe {}: {}", if published { "publicado" } else { "retirado" }, title), &session.username)
        .await;
    publish_event(state);
    Ok(Json(json!({ "id": id, "published": published })))
}

/// POST /api/transparency/reports/{id}/publish (Gestionar)
pub async fn publish_report(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    set_published(&state, &session, id, true).await
}

/// POST /api/transparency/reports/{id}/withdraw (Gestionar): se retira, no se borra
pub async fn withdraw_report(State(state): State<AppState>, Extension(session): Extension<SessionInfo>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    set_published(&state, &session, id, false).await
}
