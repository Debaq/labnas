use axum::{
    body::Body,
    extract::{Multipart, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use crate::config::resolve_home;
use crate::models::files::{FileEntry, MkdirRequest, PathQuery, QuickAccess};
use crate::state::{AppState, SessionInfo};
use crate::storage;

type ApiError = (StatusCode, String);

fn internal(e: impl std::fmt::Display) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Las raices como carpetas (vista de "/" cuando "/" no es una raiz)
fn roots_as_entries(roots: &[PathBuf]) -> Vec<FileEntry> {
    roots
        .iter()
        .map(|r| {
            let modified = std::fs::metadata(r)
                .and_then(|m| m.modified())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());
            FileEntry {
                name: r.to_string_lossy().to_string(),
                path: r.to_string_lossy().to_string(),
                is_dir: true,
                size: 0,
                modified,
                extension: None,
            }
        })
        .collect()
}

pub async fn list_files(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    let roots = storage::load_roots(&state.db).await?;
    let raw = query.path.unwrap_or_else(|| "/".to_string());

    if raw == "/" && !storage::is_root(&roots, Path::new("/")) {
        return Ok(Json(roots_as_entries(&roots)));
    }

    let target = storage::resolve_existing(&roots, &raw)?;
    if !target.is_dir() {
        return Err((StatusCode::BAD_REQUEST, "La ruta no es un directorio".to_string()));
    }

    let data_dir = std::fs::canonicalize(crate::db::data_dir()).ok();
    let mut entries = Vec::new();
    let mut dir = tokio::fs::read_dir(&target).await.map_err(internal)?;

    while let Some(entry) = dir.next_entry().await.map_err(internal)? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if data_dir.as_deref() == Some(entry.path().as_path()) {
            continue;
        }
        let metadata = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified = metadata
            .modified()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());
        let extension = entry
            .path()
            .extension()
            .map(|e| e.to_string_lossy().to_string());

        entries.push(FileEntry {
            name,
            path: entry.path().to_string_lossy().to_string(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified,
            extension,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(Json(entries))
}

pub async fn upload_file(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    mut multipart: Multipart,
) -> Result<(StatusCode, String), ApiError> {
    let roots = storage::load_roots(&state.db).await?;
    // El frontend envia "path" antes que "file"; el archivo se escribe en streaming
    let mut target_dir: Option<PathBuf> = None;
    let mut saved: Option<(String, PathBuf)> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        match field.name().unwrap_or("") {
            "path" => {
                let raw = field.text().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                target_dir = Some(storage::resolve(&roots, &raw)?);
            }
            "file" => {
                let dir = target_dir.clone().ok_or((
                    StatusCode::BAD_REQUEST,
                    "Falta la carpeta destino".to_string(),
                ))?;
                let file_name = field
                    .file_name()
                    .and_then(storage::sanitize_filename)
                    .ok_or((StatusCode::BAD_REQUEST, "Nombre de archivo invalido".to_string()))?;

                tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
                let file_path = dir.join(&file_name);
                let mut file = tokio::fs::File::create(&file_path).await.map_err(internal)?;

                let write_result: Result<(), ApiError> = async {
                    while let Some(chunk) = field
                        .chunk()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                    {
                        file.write_all(&chunk).await.map_err(internal)?;
                    }
                    file.flush().await.map_err(internal)
                }
                .await;

                if let Err(e) = write_result {
                    let _ = tokio::fs::remove_file(&file_path).await;
                    return Err(e);
                }
                saved = Some((file_name, file_path));
            }
            _ => {}
        }
    }

    let (file_name, file_path) =
        saved.ok_or((StatusCode::BAD_REQUEST, "No se proporciono archivo".to_string()))?;

    state
        .log_activity("Subida", &file_path.display().to_string(), &session.username)
        .await;

    Ok((
        StatusCode::CREATED,
        format!("Archivo '{}' subido correctamente", file_name),
    ))
}

pub async fn download_file(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let raw = query
        .path
        .ok_or((StatusCode::BAD_REQUEST, "Parametro 'path' requerido".to_string()))?;
    let roots = storage::load_roots(&state.db).await?;
    let file_path = storage::resolve_existing(&roots, &raw)?;

    if file_path.is_dir() {
        return Err((StatusCode::NOT_FOUND, "Archivo no encontrado".to_string()));
    }

    let file_name = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .replace('"', "");
    let file = tokio::fs::File::open(&file_path).await.map_err(internal)?;
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);

    let headers = [
        (header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (header::CONTENT_LENGTH, len.to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                file_name,
                urlencoding::encode(&file_name)
            ),
        ),
    ];

    let body = Body::from_stream(tokio_util::io::ReaderStream::new(file));
    Ok((headers, body))
}

pub async fn delete_file(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(query): Query<PathQuery>,
) -> Result<StatusCode, ApiError> {
    let raw = query
        .path
        .ok_or((StatusCode::BAD_REQUEST, "Parametro 'path' requerido".to_string()))?;
    let roots = storage::load_roots(&state.db).await?;
    let target = storage::resolve_existing(&roots, &raw)?;

    if storage::is_root(&roots, &target) {
        return Err((
            StatusCode::FORBIDDEN,
            "No se puede eliminar una carpeta raiz".to_string(),
        ));
    }

    let shown = target.display().to_string();
    if target.is_dir() {
        tokio::fs::remove_dir_all(&target).await.map_err(internal)?;
        state
            .log_activity("Eliminado", &format!("Carpeta: {}", shown), &session.username)
            .await;
    } else {
        tokio::fs::remove_file(&target).await.map_err(internal)?;
        state.log_activity("Eliminado", &shown, &session.username).await;
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_directory(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(body): Json<MkdirRequest>,
) -> Result<StatusCode, ApiError> {
    let roots = storage::load_roots(&state.db).await?;
    let target = storage::resolve(&roots, &body.path)?;

    tokio::fs::create_dir_all(&target).await.map_err(internal)?;

    state
        .log_activity("Carpeta", &target.display().to_string(), &session.username)
        .await;

    Ok(StatusCode::CREATED)
}

// --- Raices de almacenamiento ---

#[derive(serde::Serialize)]
pub struct RootsResponse {
    pub roots: Vec<String>,
    pub defaults: Vec<String>,
}

pub async fn get_roots(State(state): State<AppState>) -> Result<Json<RootsResponse>, ApiError> {
    let roots = crate::db::db_op(&state.db, |conn| Ok(storage::configured_roots(conn))).await?;
    Ok(Json(RootsResponse {
        roots,
        defaults: storage::default_roots(),
    }))
}

#[derive(serde::Deserialize)]
pub struct SetRootsRequest {
    pub roots: Vec<String>,
}

pub async fn set_roots(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<SetRootsRequest>,
) -> Result<Json<RootsResponse>, ApiError> {
    let roots = storage::validate_roots(&req.roots)?;
    let to_save = roots.clone();
    crate::db::db_op(&state.db, move |conn| storage::save_roots(conn, &to_save)).await?;
    state
        .log_activity("Raices de almacenamiento", &roots.join(", "), &session.username)
        .await;
    Ok(Json(RootsResponse {
        roots,
        defaults: storage::default_roots(),
    }))
}

pub async fn quick_access(State(state): State<AppState>) -> Json<Vec<QuickAccess>> {
    let home = resolve_home();
    let roots = storage::load_roots(&state.db).await.unwrap_or_default();

    let candidates: Vec<(&str, &str, &str)> = vec![
        ("Escritorio", "Desktop", "monitor"),
        ("Escritorio", "Escritorio", "monitor"),
        ("Documentos", "Documents", "file-text"),
        ("Documentos", "Documentos", "file-text"),
        ("Descargas", "Downloads", "download"),
        ("Descargas", "Descargas", "download"),
        ("Imagenes", "Pictures", "image"),
        ("Imagenes", "Imagenes", "image"),
        ("Musica", "Music", "music"),
        ("Musica", "Musica", "music"),
        ("Videos", "Videos", "video"),
        ("Plantillas", "Templates", "layout"),
        ("Plantillas", "Plantillas", "layout"),
        ("Publico", "Public", "globe"),
        ("Publico", "Publico", "globe"),
        ("Proyectos", "Projects", "code"),
        ("Proyectos", "Proyectos", "code"),
    ];

    let mut result = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    if PathBuf::from(&home).exists() {
        result.push(QuickAccess {
            name: "Inicio".to_string(),
            path: home.clone(),
            icon: "home".to_string(),
        });
        seen_names.insert("Inicio".to_string());
    }

    for (name, subdir, icon) in &candidates {
        let full_path = PathBuf::from(&home).join(subdir);
        if full_path.exists() && full_path.is_dir() && !seen_names.contains(*name) {
            result.push(QuickAccess {
                name: name.to_string(),
                path: full_path.to_string_lossy().to_string(),
                icon: icon.to_string(),
            });
            seen_names.insert(name.to_string());
        }
    }

    // Solo medios externos montados, no rutas de sistema
    for (name, path, icon) in [("Medios", "/media", "disc"), ("Montajes", "/mnt", "disc")] {
        let p = PathBuf::from(path);
        if p.exists() && p.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false) {
            result.push(QuickAccess {
                name: name.to_string(),
                path: path.to_string(),
                icon: icon.to_string(),
            });
        }
    }

    // Solo accesos dentro de las raices; y las raices propias que no esten listadas
    result.retain(|qa| storage::resolve(&roots, &qa.path).is_ok());
    for r in &roots {
        let path = r.to_string_lossy().to_string();
        if !result.iter().any(|qa| qa.path == path) {
            result.push(QuickAccess {
                name: r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(path.clone()),
                path,
                icon: "disc".to_string(),
            });
        }
    }

    Json(result)
}
