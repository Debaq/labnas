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
use crate::storage::{self, Op};

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
    Extension(session): Extension<SessionInfo>,
    Query(query): Query<PathQuery>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    let st = storage::Storage::load(&state.db).await?;
    let raw = query.path.unwrap_or_else(|| "/".to_string());

    // "/" muestra solo las raices que el usuario puede leer
    if raw == "/" && !st.is_root(Path::new("/")) {
        return Ok(Json(roots_as_entries(&st.visible_roots(&session))));
    }

    let target = st.resolve_existing_for(&session, &raw, Op::Read)?;
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
    let st = storage::Storage::load(&state.db).await?;
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
                target_dir = Some(st.resolve_for(&session, &raw, Op::Write)?);
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
    Extension(session): Extension<SessionInfo>,
    Query(query): Query<PathQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let raw = query
        .path
        .ok_or((StatusCode::BAD_REQUEST, "Parametro 'path' requerido".to_string()))?;
    let st = storage::Storage::load(&state.db).await?;
    let file_path = st.resolve_existing_for(&session, &raw, Op::Read)?;

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
    let st = storage::Storage::load(&state.db).await?;
    let target = st.resolve_existing_for(&session, &raw, Op::Write)?;

    if st.is_root(&target) {
        return Err((
            StatusCode::FORBIDDEN,
            "No se puede eliminar una carpeta raiz".to_string(),
        ));
    }

    // No se puede borrar algo que ya esta en una papelera desde el explorador
    if target.components().any(|c| c.as_os_str() == crate::handlers::trash::TRASH_DIR) {
        return Err((StatusCode::BAD_REQUEST, "Usa la papelera para gestionar este elemento".to_string()));
    }

    crate::handlers::trash::move_to_trash(&state, &st.paths(), &target, &session.username).await?;
    state
        .log_activity("A la papelera", &target.display().to_string(), &session.username)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_directory(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(body): Json<MkdirRequest>,
) -> Result<StatusCode, ApiError> {
    let st = storage::Storage::load(&state.db).await?;
    let target = st.resolve_for(&session, &body.path, Op::Write)?;

    tokio::fs::create_dir_all(&target).await.map_err(internal)?;

    state
        .log_activity("Carpeta", &target.display().to_string(), &session.username)
        .await;

    Ok(StatusCode::CREATED)
}

// --- Vista previa y descarga por token ---
//
// <img>/<video>/<a download> no pueden mandar el header Authorization. En vez de
// aceptar el token de sesion en la URL, se emite un token de un solo archivo que
// dura PREVIEW_TTL y se sirve en /api/preview/{token}/{nombre} (ruta publica).

const PREVIEW_TTL: std::time::Duration = std::time::Duration::from_secs(30 * 60);

static PREVIEW_TOKENS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, (PathBuf, std::time::Instant)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Tipos que el navegador ejecutaria como documento en el origen de LabNAS: se
/// sirven como texto plano para que un archivo subido no pueda robar sesiones.
const ACTIVE_CONTENT_EXT: &[&str] = &[
    "html", "htm", "xhtml", "xht", "shtml", "xml", "xsl", "xslt", "mht", "mhtml", "js", "mjs",
];

#[derive(serde::Deserialize)]
pub struct PreviewTokenRequest {
    pub path: String,
}

#[derive(serde::Serialize)]
pub struct PreviewTokenResponse {
    /// Para mostrar en linea (img, video, iframe)
    pub url: String,
    /// Misma URL forzando descarga
    pub download_url: String,
}

pub async fn create_preview_token(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<PreviewTokenRequest>,
) -> Result<Json<PreviewTokenResponse>, ApiError> {
    let st = storage::Storage::load(&state.db).await?;
    let path = st.resolve_existing_for(&session, &req.path, Op::Read)?;
    if path.is_dir() {
        return Err((StatusCode::BAD_REQUEST, "No es un archivo".to_string()));
    }

    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "archivo".to_string());
    {
        let mut tokens = PREVIEW_TOKENS.lock().map_err(internal)?;
        tokens.retain(|_, (_, created)| created.elapsed() < PREVIEW_TTL);
        tokens.insert(token.clone(), (path, std::time::Instant::now()));
    }

    let url = format!("/api/preview/{}/{}", token, urlencoding::encode(&name));
    Ok(Json(PreviewTokenResponse {
        download_url: format!("{}?download=1", url),
        url,
    }))
}

#[derive(serde::Deserialize)]
pub struct PreviewQuery {
    pub download: Option<u8>,
}

/// GET /api/preview/{token}/{nombre} — publico, validado por el token
pub async fn serve_preview(
    axum::extract::Path((token, _name)): axum::extract::Path<(String, String)>,
    Query(q): Query<PreviewQuery>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ApiError> {
    use tower::ServiceExt;

    let path = {
        let tokens = PREVIEW_TOKENS.lock().map_err(internal)?;
        match tokens.get(&token) {
            Some((p, created)) if created.elapsed() < PREVIEW_TTL => p.clone(),
            _ => return Err((StatusCode::GONE, "Link de vista previa expirado".to_string())),
        }
    };

    // ServeFile resuelve tipo MIME, Range (video/audio) y cabeceras condicionales
    let mut resp = tower_http::services::ServeFile::new(&path)
        .oneshot(request)
        .await
        .map_err(internal)?
        .map(Body::new);

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let headers = resp.headers_mut();
    let disposition = if q.download == Some(1) { "attachment" } else { "inline" };
    let ascii_name: String = name.chars().map(|c| if c.is_ascii() && c != '"' { c } else { '_' }).collect();
    if let Ok(v) = format!(
        "{}; filename=\"{}\"; filename*=UTF-8''{}",
        disposition,
        ascii_name,
        urlencoding::encode(&name)
    )
    .parse()
    {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff"));
    headers.insert(header::REFERRER_POLICY, header::HeaderValue::from_static("no-referrer"));
    if ACTIVE_CONTENT_EXT.contains(&ext.as_str()) {
        headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("text/plain; charset=utf-8"));
    }
    if ext == "svg" {
        // Se ve como imagen, pero abierto directo no puede ejecutar scripts
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            header::HeaderValue::from_static("script-src 'none'; object-src 'none'"),
        );
    }

    Ok(resp)
}

// --- Raices de almacenamiento ---

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RootConfig {
    pub path: String,
    #[serde(flatten, default)]
    pub access: storage::RootAccess,
}

#[derive(serde::Serialize)]
pub struct RootsResponse {
    pub roots: Vec<RootConfig>,
    pub defaults: Vec<String>,
}

async fn roots_response(state: &AppState) -> Result<RootsResponse, ApiError> {
    let st = storage::Storage::load(&state.db).await?;
    let configured = crate::db::db_op(&state.db, |conn| Ok(storage::configured_roots(conn))).await?;
    let access: std::collections::HashMap<PathBuf, storage::RootAccess> = st.access().into_iter().collect();
    let roots = configured
        .into_iter()
        .map(|path| {
            let access = std::fs::canonicalize(&path)
                .ok()
                .and_then(|c| access.get(&c).cloned())
                .unwrap_or_default();
            RootConfig { path, access }
        })
        .collect();
    Ok(RootsResponse { roots, defaults: storage::default_roots() })
}

/// GET /api/files/roots — para no admins solo las rutas que pueden leer (sin permisos)
pub async fn get_roots(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<RootsResponse>, ApiError> {
    let mut resp = roots_response(&state).await?;
    if session.role != crate::models::notifications::UserRole::Admin {
        let st = storage::Storage::load(&state.db).await?;
        let visible = st.visible_roots(&session);
        resp.roots.retain(|r| std::fs::canonicalize(&r.path).map(|c| visible.contains(&c)).unwrap_or(false));
        for r in &mut resp.roots {
            r.access = storage::RootAccess { readers: vec![], writers: vec![] };
        }
    }
    Ok(Json(resp))
}

#[derive(serde::Deserialize)]
pub struct SetRootsRequest {
    pub roots: Vec<RootConfig>,
}

/// PUT /api/files/roots (admin) — carpetas accesibles y quien lee/escribe en cada una
pub async fn set_roots(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<SetRootsRequest>,
) -> Result<Json<RootsResponse>, ApiError> {
    for r in &req.roots {
        if let Some(bad) = r.access.readers.iter().chain(&r.access.writers).find(|p| !storage::valid_principal(p)) {
            return Err((StatusCode::BAD_REQUEST, format!("Permiso invalido: '{}'", bad)));
        }
    }
    let paths: Vec<String> = req.roots.iter().map(|r| r.path.clone()).collect();
    let canonical = storage::validate_roots(&paths)?;
    // validate_roots canonicaliza y quita duplicados: emparejar cada ruta con su acceso
    let mut entries: Vec<(String, storage::RootAccess)> = Vec::new();
    for r in req.roots {
        if let Ok(c) = std::fs::canonicalize(r.path.trim()) {
            let c = c.to_string_lossy().to_string();
            if canonical.contains(&c) && !entries.iter().any(|(p, _)| *p == c) {
                entries.push((c, r.access));
            }
        }
    }
    let summary: Vec<String> = entries
        .iter()
        .map(|(p, a)| format!("{} (lee: {}; escribe: {})", p, a.readers.join(","), a.writers.join(",")))
        .collect();
    let (to_save, acl) = (canonical.clone(), entries);
    crate::db::db_op(&state.db, move |conn| {
        storage::save_roots(conn, &to_save)?;
        storage::save_access(conn, &acl)
    })
    .await?;
    state
        .log_activity("Carpetas accesibles", &summary.join(" | "), &session.username)
        .await;
    Ok(Json(roots_response(&state).await?))
}

pub async fn quick_access(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Json<Vec<QuickAccess>> {
    let home = resolve_home();
    let Ok(st) = storage::Storage::load(&state.db).await else { return Json(vec![]) };
    let roots = st.visible_roots(&session);

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
    result.retain(|qa| st.resolve_for(&session, &qa.path, Op::Read).is_ok());
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
