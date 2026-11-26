//! Registro de retazos (idea 32), en Inventario: cada sobrante de plancha (MDF, acrilico,
//! terciado...) con foto, material, espesor y medidas, para reutilizarlo antes de cortar una
//! plancha nueva. Se busca por material, espesor y tamaño minimo (en cualquier orientacion);
//! quien lo usa lo marca «lo tome». Ver docs/INVENTARIO.md.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    Extension, Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::{internal, ApiError};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

fn bad(msg: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.into())
}

fn now_str() -> String {
    chrono::Local::now().naive_local().format("%Y-%m-%dT%H:%M").to_string()
}

fn publish(state: &AppState) {
    state.events.publish("inventory.offcuts", (), crate::events::Audience::All, Some("inventory"));
}

#[derive(Debug, Clone, Serialize)]
pub struct Offcut {
    pub id: String,
    pub material: String,
    pub thickness_mm: Option<f64>,
    pub width_mm: f64,
    pub height_mm: f64,
    pub location: String,
    pub notes: String,
    pub has_photo: bool,
    pub added_by: String,
    pub added_at: String,
    pub taken_by: Option<String>,
    pub taken_at: Option<String>,
}

const SELECT: &str = "SELECT id, material, thickness_mm, width_mm, height_mm, location, notes, photo IS NOT NULL, added_by, added_at, taken_by, taken_at FROM offcuts";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Offcut> {
    Ok(Offcut {
        id: r.get(0)?,
        material: r.get(1)?,
        thickness_mm: r.get(2)?,
        width_mm: r.get(3)?,
        height_mm: r.get(4)?,
        location: r.get(5)?,
        notes: r.get(6)?,
        has_photo: r.get(7)?,
        added_by: r.get(8)?,
        added_at: r.get(9)?,
        taken_by: r.get(10)?,
        taken_at: r.get(11)?,
    })
}

fn load(conn: &Connection, id: &str) -> rusqlite::Result<Option<Offcut>> {
    conn.query_row(&format!("{} WHERE id = ?1", SELECT), [id], row).optional()
}

/// ¿Sirve un retazo de `w`×`h` para una pieza de `need_w`×`need_h`? (se puede girar)
pub fn fits(w: f64, h: f64, need_w: f64, need_h: f64) -> bool {
    (w >= need_w && h >= need_h) || (w >= need_h && h >= need_w)
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    /// Material (contiene, sin distinguir mayusculas)
    pub material: Option<String>,
    pub thickness: Option<f64>,
    /// Tamaño minimo de la pieza que se quiere cortar
    pub min_w: Option<f64>,
    pub min_h: Option<f64>,
    /// Incluir los ya tomados
    #[serde(default)]
    pub all: bool,
}

/// GET /api/inventory/offcuts?material=&thickness=&min_w=&min_h=&all= — los mas chicos que
/// sirven primero (para no gastar uno grande en una pieza chica)
pub async fn list(State(state): State<AppState>, Query(q): Query<ListQuery>) -> Result<Json<Vec<Offcut>>, ApiError> {
    db_op(&state.db, move |conn| {
        let mut sql = format!("{} WHERE 1 = 1", SELECT);
        let mut args: Vec<String> = Vec::new();
        if !q.all {
            sql.push_str(" AND taken_at IS NULL");
        }
        if let Some(m) = q.material.map(|m| m.trim().to_lowercase()).filter(|m| !m.is_empty()) {
            args.push(format!("%{}%", m));
            sql.push_str(&format!(" AND lower(material) LIKE ?{}", args.len()));
        }
        sql.push_str(" ORDER BY width_mm * height_mm LIMIT 1000");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows: Vec<Offcut> = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), row)
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<_>>()
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .filter(|o| q.thickness.is_none_or(|t| o.thickness_mm.is_some_and(|ot| (ot - t).abs() < 0.05)))
            .filter(|o| fits(o.width_mm, o.height_mm, q.min_w.unwrap_or(0.0), q.min_h.unwrap_or(0.0)))
            .collect())
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct OffcutReq {
    pub material: String,
    pub thickness_mm: Option<f64>,
    pub width_mm: f64,
    pub height_mm: f64,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub notes: String,
    /// Foto (data URL o base64; JPEG/PNG/WebP, maximo 2 MB)
    pub photo: Option<String>,
}

/// POST /api/inventory/offcuts
pub async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<OffcutReq>,
) -> Result<(StatusCode, Json<Offcut>), ApiError> {
    let material = req.material.trim().to_string();
    if material.is_empty() || material.chars().count() > 80 {
        return Err(bad("Indica el material (maximo 80 caracteres)"));
    }
    let dim_ok = |v: f64| v > 0.0 && v <= 10000.0;
    if !dim_ok(req.width_mm) || !dim_ok(req.height_mm) || req.thickness_mm.is_some_and(|t| !(0.0..=500.0).contains(&t)) {
        return Err(bad("Medidas invalidas (mm)"));
    }
    let (location, notes) = (req.location.trim().to_string(), req.notes.trim().to_string());
    if location.chars().count() > 120 || notes.chars().count() > 1000 {
        return Err(bad("Texto demasiado largo"));
    }
    let photo = req.photo.as_deref().filter(|p| !p.is_empty()).map(crate::handlers::machines::decode_photo).transpose()?;
    let user = session.username.clone();
    let o = db_op_status(&state.db, move |conn| {
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let (bytes, mime) = match photo {
            Some((b, m)) => (Some(b), Some(m)),
            None => (None, None),
        };
        conn.execute(
            "INSERT INTO offcuts (id, material, thickness_mm, width_mm, height_mm, location, notes, photo, photo_mime, added_by, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, material, req.thickness_mm, req.width_mm, req.height_mm, location, notes, bytes, mime, user, now_str()],
        )
        .map_err(internal)?;
        load(conn, &id).map_err(internal)?.ok_or((StatusCode::NOT_FOUND, "Retazo no encontrado".to_string()))
    })
    .await?;
    publish(&state);
    Ok((StatusCode::CREATED, Json(o)))
}

/// POST /api/inventory/offcuts/{id}/take — «lo tome» (lo saca de la lista)
pub async fn take(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<Offcut>, ApiError> {
    let user = session.username.clone();
    let o = db_op_status(&state.db, move |conn| {
        let n = conn
            .execute("UPDATE offcuts SET taken_by = ?2, taken_at = ?3 WHERE id = ?1 AND taken_at IS NULL", params![id, user, now_str()])
            .map_err(internal)?;
        if n == 0 {
            return Err((StatusCode::CONFLICT, "Ese retazo ya no esta disponible".into()));
        }
        load(conn, &id).map_err(internal)?.ok_or((StatusCode::NOT_FOUND, "Retazo no encontrado".to_string()))
    })
    .await?;
    publish(&state);
    Ok(Json(o))
}

/// DELETE /api/inventory/offcuts/{id} — quien lo subio o quien gestiona Inventario
pub async fn delete(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let staff = session.can("inventory", Level::Gestionar);
    let user = session.username.clone();
    db_op_status(&state.db, move |conn| {
        let o = load(conn, &id).map_err(internal)?.ok_or((StatusCode::NOT_FOUND, "Retazo no encontrado".to_string()))?;
        if o.added_by != user && !staff {
            return Err((StatusCode::FORBIDDEN, "Lo borra quien lo subio o quien gestiona Inventario".into()));
        }
        conn.execute("DELETE FROM offcuts WHERE id = ?1", [&id]).map_err(internal)?;
        Ok(())
    })
    .await?;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/inventory/offcuts/{id}/photo
pub async fn photo(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let found: Option<(Vec<u8>, String)> = db_op(&state.db, move |conn| {
        conn.query_row("SELECT photo, photo_mime FROM offcuts WHERE id = ?1 AND photo IS NOT NULL", [&id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(|e| e.to_string())
    })
    .await?;
    let (bytes, mime) = found.ok_or((StatusCode::NOT_FOUND, "Sin foto".to_string()))?;
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "private, max-age=86400")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(bytes))
        .map_err(internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sirve_girado() {
        assert!(fits(300.0, 100.0, 90.0, 250.0));
        assert!(!fits(300.0, 100.0, 120.0, 120.0));
        assert!(fits(200.0, 200.0, 0.0, 0.0));
    }
}
