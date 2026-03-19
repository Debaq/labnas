use axum::{
    extract::{Multipart, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use std::path::PathBuf;

use crate::models::files::{FileEntry, MkdirRequest, PathQuery, QuickAccess};
use crate::state::AppState;

const PROTECTED_PATHS: &[&str] = &[
    "/", "/bin", "/sbin", "/usr", "/etc", "/boot", "/dev", "/proc", "/sys", "/lib", "/lib64",
    "/var",
];

fn is_path_or_direct_child_of_protected(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy().to_string();
    let path_str = path_str.trim_end_matches('/');

    for protected in PROTECTED_PATHS {
        if path_str == *protected {
            return true;
        }
    }
    false
}

pub async fn list_files(
    Query(query): Query<PathQuery>,
) -> Result<Json<Vec<FileEntry>>, (StatusCode, String)> {
    let target = PathBuf::from(query.path.unwrap_or_else(|| "/".to_string()));

    if !target.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "La ruta debe ser absoluta".to_string(),
        ));
    }

    if !target.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            "Directorio no encontrado".to_string(),
        ));
    }

    if !target.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            "La ruta no es un directorio".to_string(),
        ));
    }

    let mut entries = Vec::new();
    let mut dir = tokio::fs::read_dir(&target)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        let metadata = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        let abs_path = entry.path().to_string_lossy().to_string();
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
            path: abs_path,
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
    mut multipart: Multipart,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let mut dest_path: Option<String> = None;
    let mut file_name: Option<String> = None;
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "path" => {
                dest_path = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
                );
            }
            "file" => {
                file_name = field.file_name().map(|s| s.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let file_name = file_name.ok_or((
        StatusCode::BAD_REQUEST,
        "No se proporcionó archivo".to_string(),
    ))?;
    let file_data = file_data.ok_or((StatusCode::BAD_REQUEST, "Archivo vacío".to_string()))?;

    let target_dir = PathBuf::from(dest_path.unwrap_or_else(|| "/tmp".to_string()));

    if !target_dir.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "La ruta debe ser absoluta".to_string(),
        ));
    }

    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let file_path = target_dir.join(&file_name);
    tokio::fs::write(&file_path, &file_data)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.log_activity("Subida", &format!("{}", file_path.display()), "web").await;

    Ok((
        StatusCode::CREATED,
        format!("Archivo '{}' subido correctamente", file_name),
    ))
}

pub async fn download_file(
    Query(query): Query<PathQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let path = query.path.ok_or((
        StatusCode::BAD_REQUEST,
        "Parámetro 'path' requerido".to_string(),
    ))?;
    let file_path = PathBuf::from(&path);

    if !file_path.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "La ruta debe ser absoluta".to_string(),
        ));
    }

    if !file_path.exists() || file_path.is_dir() {
        return Err((
            StatusCode::NOT_FOUND,
            "Archivo no encontrado".to_string(),
        ));
    }

    let file_name = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let data = tokio::fs::read(&file_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let headers = [
        (header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", file_name),
        ),
    ];

    Ok((headers, data))
}

pub async fn delete_file(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
) -> Result<StatusCode, (StatusCode, String)> {
    let path = query.path.ok_or((
        StatusCode::BAD_REQUEST,
        "Parámetro 'path' requerido".to_string(),
    ))?;
    let target = PathBuf::from(&path);

    if !target.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "La ruta debe ser absoluta".to_string(),
        ));
    }

    if is_path_or_direct_child_of_protected(&target) {
        return Err((
            StatusCode::FORBIDDEN,
            format!(
                "No se puede eliminar '{}': es una ruta protegida del sistema",
                path
            ),
        ));
    }

    if !target.exists() {
        return Err((StatusCode::NOT_FOUND, "No encontrado".to_string()));
    }

    if target.is_dir() {
        tokio::fs::remove_dir_all(&target)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        state.log_activity("Eliminado", &format!("Carpeta: {}", path), "web").await;
    } else {
        tokio::fs::remove_file(&target)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        state.log_activity("Eliminado", &path, "web").await;
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_directory(
    State(state): State<AppState>,
    Json(body): Json<MkdirRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let target = PathBuf::from(&body.path);

    if !target.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "La ruta debe ser absoluta".to_string(),
        ));
    }

    tokio::fs::create_dir_all(&target)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.log_activity("Carpeta", &body.path, "web").await;

    Ok(StatusCode::CREATED)
}

pub async fn quick_access() -> Json<Vec<QuickAccess>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());

    let candidates: Vec<(&str, &str, &str)> = vec![
        ("Inicio", &home, "home"),
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
        if *name == "Inicio" {
            continue;
        }
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

    let system_dirs = vec![
        ("Raiz", "/", "hard-drive"),
        ("Tmp", "/tmp", "trash"),
        ("Medios", "/media", "disc"),
        ("Montajes", "/mnt", "disc"),
    ];

    for (name, path, icon) in system_dirs {
        if PathBuf::from(path).exists() {
            result.push(QuickAccess {
                name: name.to_string(),
                path: path.to_string(),
                icon: icon.to_string(),
            });
        }
    }

    Json(result)
}
