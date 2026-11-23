//! Subidas por partes: archivos de cualquier tamaño en pedidos chicos.
//!
//! Las usa el frontend para archivos grandes; por el puente PHP son imprescindibles,
//! porque cada pedido debe caber en el `post_max_size` del hosting.
//!
//! 1. `POST /api/files/upload/chunked` {path, name, size} -> {id}
//! 2. `PUT /api/files/upload/chunked/{id}?offset=N` con los bytes (en orden)
//! 3. `POST /api/files/upload/chunked/{id}/complete` -> se renombra al nombre final
//!
//! Se escribe a un temporal `.nombre.<id>.part` creado con `create_new` (no sigue
//! symlinks) dentro de la carpeta destino, asi el rename final es atomico.

use axum::{
    body::Bytes,
    extract::{Path as AxPath, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

use crate::state::{AppState, SessionInfo};
use crate::storage::{self, Op};
use crate::error::{ApiError, internal};

/// Una subida sin actividad por este tiempo se descarta
const IDLE_TTL: Duration = Duration::from_secs(3600);
/// Tope de una parte (el frontend usa partes mas chicas)
pub const MAX_CHUNK: usize = 16 * 1024 * 1024;

struct Upload {
    owner: String,
    part: PathBuf,
    target: PathBuf,
    size: u64,
    written: u64,
    touched: Instant,
}

static UPLOADS: LazyLock<StdMutex<HashMap<String, Upload>>> = LazyLock::new(Default::default);

/// Borra subidas abandonadas (y sus temporales)
fn purge_idle() {
    let stale: Vec<PathBuf> = match UPLOADS.lock() {
        Ok(mut m) => {
            let ids: Vec<String> = m.iter().filter(|(_, u)| u.touched.elapsed() > IDLE_TTL).map(|(k, _)| k.clone()).collect();
            ids.iter().filter_map(|id| m.remove(id)).map(|u| u.part).collect()
        }
        Err(_) => Vec::new(),
    };
    for p in stale {
        let _ = std::fs::remove_file(p);
    }
}

#[derive(Deserialize)]
pub struct InitRequest {
    pub path: String,
    pub name: String,
    pub size: u64,
}

/// POST /api/files/upload/chunked
pub async fn init(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<InitRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    purge_idle();
    let st = storage::Storage::load(&state.db).await?;
    let dir = st.resolve_for(&session, &req.path, Op::Write)?;
    let name = storage::sanitize_filename(&req.name).ok_or((StatusCode::BAD_REQUEST, "Nombre de archivo invalido".to_string()))?;

    let limit_mb = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_setting_u32(conn, "upload_limit_mb", 50))).await?;
    if req.size > u64::from(limit_mb) * 1024 * 1024 {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("El archivo supera el limite de subida ({} MB)", limit_mb)));
    }

    tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let part = dir.join(format!(".{}.{}.part", name, &id[..12]));
    tokio::fs::OpenOptions::new().write(true).create_new(true).open(&part).await.map_err(internal)?;

    if let Ok(mut m) = UPLOADS.lock() {
        m.insert(
            id.clone(),
            Upload { owner: session.username.clone(), part, target: dir.join(&name), size: req.size, written: 0, touched: Instant::now() },
        );
    }
    Ok(Json(serde_json::json!({ "id": id, "max_chunk": MAX_CHUNK })))
}

#[derive(Deserialize)]
pub struct ChunkQuery {
    pub offset: u64,
}

/// Datos de una subida del usuario (sin mantener el lock durante la escritura)
fn lookup(id: &str, user: &str) -> Result<(PathBuf, u64, u64), ApiError> {
    let m = UPLOADS.lock().map_err(internal)?;
    let u = m.get(id).filter(|u| u.owner == user).ok_or((StatusCode::NOT_FOUND, "Subida no encontrada o vencida".to_string()))?;
    Ok((u.part.clone(), u.size, u.written))
}

/// PUT /api/files/upload/chunked/{id}?offset=N
pub async fn chunk(
    Extension(session): Extension<SessionInfo>,
    AxPath(id): AxPath<String>,
    Query(q): Query<ChunkQuery>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.len() > MAX_CHUNK {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "Parte demasiado grande".to_string()));
    }
    let (part, size, written) = lookup(&id, &session.username)?;
    if q.offset != written {
        // El cliente puede reanudar desde `written`
        return Err((StatusCode::CONFLICT, format!("{}", written)));
    }
    if written + body.len() as u64 > size {
        return Err((StatusCode::BAD_REQUEST, "La subida supera el tamaño anunciado".to_string()));
    }
    let mut f = tokio::fs::OpenOptions::new().append(true).open(&part).await.map_err(internal)?;
    f.write_all(&body).await.map_err(internal)?;
    f.flush().await.map_err(internal)?;

    let written = {
        let mut m = UPLOADS.lock().map_err(internal)?;
        let u = m.get_mut(&id).ok_or((StatusCode::NOT_FOUND, "Subida cancelada".to_string()))?;
        u.written += body.len() as u64;
        u.touched = Instant::now();
        u.written
    };
    Ok(Json(serde_json::json!({ "written": written })))
}

/// POST /api/files/upload/chunked/{id}/complete
pub async fn complete(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    AxPath(id): AxPath<String>,
) -> Result<(StatusCode, String), ApiError> {
    let (part, size, written) = lookup(&id, &session.username)?;
    if written != size {
        return Err((StatusCode::BAD_REQUEST, format!("Faltan datos: {} de {} bytes", written, size)));
    }
    let target = {
        let mut m = UPLOADS.lock().map_err(internal)?;
        m.remove(&id).map(|u| u.target).ok_or((StatusCode::NOT_FOUND, "Subida no encontrada".to_string()))?
    };
    // rename reemplaza un symlink existente con ese nombre, no escribe a donde apunta
    tokio::fs::rename(&part, &target).await.map_err(internal)?;
    state.log_activity("Subida", &target.display().to_string(), &session.username).await;
    let name = target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok((StatusCode::CREATED, format!("Archivo '{}' subido correctamente", name)))
}

/// DELETE /api/files/upload/chunked/{id}
pub async fn abort(Extension(session): Extension<SessionInfo>, AxPath(id): AxPath<String>) -> StatusCode {
    let part = UPLOADS.lock().ok().and_then(|mut m| {
        if m.get(&id).is_some_and(|u| u.owner == session.username) {
            m.remove(&id).map(|u| u.part)
        } else {
            None
        }
    });
    match part {
        Some(p) => {
            let _ = tokio::fs::remove_file(p).await;
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}
