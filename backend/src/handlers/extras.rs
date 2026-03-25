use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use std::time::Instant;

use crate::config::save_config;
use crate::models::notes::Note;
use crate::state::AppState;

fn get_session_user(
    state: &AppState,
    sessions: &std::collections::HashMap<String, crate::state::SessionInfo>,
    headers: &HeaderMap,
) -> Option<String> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))?;
    let _ = state;
    sessions.get(token).map(|s| s.username.clone())
}

// =====================
// Temporary file sharing
// =====================

#[derive(Deserialize)]
pub struct ShareRequest {
    pub path: String,
    #[serde(default = "default_hours")]
    pub expires_hours: u32,
}

fn default_hours() -> u32 {
    24
}

#[derive(serde::Serialize)]
pub struct ShareResponse {
    pub token: String,
    pub url: String,
    pub expires_hours: u32,
}

pub async fn create_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ShareRequest>,
) -> Result<Json<ShareResponse>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let username = get_session_user(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let path = std::path::PathBuf::from(&req.path);
    if !path.exists() || path.is_dir() {
        return Err((StatusCode::NOT_FOUND, "Archivo no encontrado".to_string()));
    }

    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let token = uuid::Uuid::new_v4().to_string();
    let hours = req.expires_hours.max(1).min(168); // 1h to 7 days

    let mut shares = state.share_links.lock().await;
    shares.insert(
        token.clone(),
        crate::state::ShareLink {
            file_path: req.path.clone(),
            file_name: file_name.clone(),
            created_at: Instant::now(),
            expires_secs: (hours as u64) * 3600,
        },
    );

    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "localhost".to_string());

    state
        .log_activity("Compartido", &format!("{} ({}h)", file_name, hours), &username)
        .await;

    Ok(Json(ShareResponse {
        url: format!("http://{}:3001/api/share/{}", local_ip, token),
        token,
        expires_hours: hours,
    }))
}

pub async fn download_share(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut shares = state.share_links.lock().await;

    let share = shares
        .get(&token)
        .ok_or((StatusCode::NOT_FOUND, "Link no encontrado o expirado".to_string()))?;

    // Check expiration
    if share.created_at.elapsed().as_secs() > share.expires_secs {
        shares.remove(&token);
        return Err((StatusCode::GONE, "Link expirado".to_string()));
    }

    let file_path = share.file_path.clone();
    let file_name = share.file_name.clone();
    drop(shares);

    let data = tokio::fs::read(&file_path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Archivo no disponible".to_string()))?;

    let headers = [
        (header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", file_name),
        ),
    ];

    Ok((headers, data))
}

pub async fn list_shares(
    State(state): State<AppState>,
) -> Json<Vec<serde_json::Value>> {
    let shares = state.share_links.lock().await;
    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "localhost".to_string());

    let list: Vec<serde_json::Value> = shares
        .iter()
        .filter(|(_, s)| s.created_at.elapsed().as_secs() <= s.expires_secs)
        .map(|(token, s)| {
            let remaining = s.expires_secs.saturating_sub(s.created_at.elapsed().as_secs());
            serde_json::json!({
                "token": token,
                "file_name": s.file_name,
                "file_path": s.file_path,
                "url": format!("http://{}:3001/api/share/{}", local_ip, token),
                "remaining_minutes": remaining / 60,
            })
        })
        .collect();

    Json(list)
}

pub async fn delete_share(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> StatusCode {
    let mut shares = state.share_links.lock().await;
    shares.remove(&token);
    StatusCode::NO_CONTENT
}

// =====================
// Download from URL
// =====================

#[derive(Deserialize)]
pub struct DownloadUrlRequest {
    pub url: String,
    pub destination: String, // directory path
}

pub async fn download_url(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DownloadUrlRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let username = get_session_user(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let dest = std::path::PathBuf::from(&req.destination);
    if !dest.is_absolute() {
        return Err((StatusCode::BAD_REQUEST, "Ruta destino debe ser absoluta".to_string()));
    }

    // Extract filename from URL
    let url_parsed = req.url.split('?').next().unwrap_or(&req.url);
    let file_name = url_parsed
        .split('/')
        .last()
        .filter(|s| !s.is_empty())
        .unwrap_or("descarga");

    let file_path = dest.join(file_name);

    // Download
    let response = state
        .http_client
        .get(&req.url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Error descargando: {}", e)))?;

    if !response.status().is_success() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("URL respondio con {}", response.status()),
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error leyendo datos: {}", e)))?;

    // Ensure directory exists
    tokio::fs::create_dir_all(&dest)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tokio::fs::write(&file_path, &bytes)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let size = bytes.len();
    let msg = format!("{} ({} bytes)", file_name, size);

    state.log_activity("Descarga", &msg, &username).await;

    Ok((StatusCode::OK, format!("Descargado: {} ({} bytes)", file_name, size)))
}

// =====================
// Notes (Markdown)
// =====================

#[derive(Deserialize)]
pub struct CreateNoteRequest {
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub shared_with: Vec<String>,
    #[serde(default)]
    pub is_public: bool,
}

#[derive(Deserialize)]
pub struct UpdateNoteRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub shared_with: Option<Vec<String>>,
    pub is_public: Option<bool>,
}

pub async fn list_notes(State(state): State<AppState>) -> Json<Vec<Note>> {
    let config = state.config.lock().await;
    Json(config.notes.clone())
}

pub async fn create_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateNoteRequest>,
) -> Result<Json<Note>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let username = get_session_user(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let now = Utc::now();
    let note = Note {
        id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
        title: req.title,
        content: req.content,
        created_by: username.clone(),
        updated_by: username.clone(),
        shared_with: req.shared_with.clone(),
        is_public: req.is_public,
        created_at: now,
        updated_at: now,
    };

    let mut config = state.config.lock().await;
    config.notes.push(note.clone());

    // Notificar a usuarios compartidos por Telegram
    let token = config.notifications.bot_token.clone();
    let chats = config.notifications.telegram_chats.clone();
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    drop(config);

    if !req.shared_with.is_empty() {
        if let Some(token) = &token {
            let msg = format!(
                "📝 *Nota compartida*\n\n*{}*\nPor: {}",
                note.title, username
            );
            for chat in &chats {
                let is_target = req.shared_with.iter().any(|u| {
                    u.to_lowercase() == chat.name.to_lowercase()
                        || chat
                            .linked_web_user
                            .as_ref()
                            .map(|w| w.to_lowercase() == u.to_lowercase())
                            .unwrap_or(false)
                });
                if is_target {
                    let _ = crate::handlers::notifications::send_tg_public(
                        &state.http_client,
                        token,
                        chat.chat_id,
                        &msg,
                    )
                    .await;
                }
            }
        }
    }

    state.log_activity("Nota", &note.title, &username).await;
    Ok(Json(note))
}

pub async fn update_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateNoteRequest>,
) -> Result<Json<Note>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let username = get_session_user(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let mut config = state.config.lock().await;
    let note = config
        .notes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Nota no encontrada".to_string()))?;

    if let Some(title) = req.title {
        note.title = title;
    }
    if let Some(content) = req.content {
        note.content = content;
    }

    // Detectar nuevos usuarios compartidos para notificar
    let old_shared: std::collections::HashSet<String> =
        note.shared_with.iter().map(|s| s.to_lowercase()).collect();
    let mut new_users: Vec<String> = Vec::new();

    if let Some(shared_with) = req.shared_with {
        for u in &shared_with {
            if !old_shared.contains(&u.to_lowercase()) {
                new_users.push(u.clone());
            }
        }
        note.shared_with = shared_with;
    }
    if let Some(is_public) = req.is_public {
        note.is_public = is_public;
    }
    note.updated_by = username.clone();
    note.updated_at = Utc::now();

    let updated = note.clone();
    let token = config.notifications.bot_token.clone();
    let chats = config.notifications.telegram_chats.clone();
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    drop(config);

    // Notificar nuevos compartidos
    if !new_users.is_empty() {
        if let Some(token) = &token {
            let msg = format!(
                "📝 *Nota compartida*\n\n*{}*\nPor: {}",
                updated.title, username
            );
            for chat in &chats {
                let is_target = new_users.iter().any(|u| {
                    u.to_lowercase() == chat.name.to_lowercase()
                        || chat
                            .linked_web_user
                            .as_ref()
                            .map(|w| w.to_lowercase() == u.to_lowercase())
                            .unwrap_or(false)
                });
                if is_target {
                    let _ = crate::handlers::notifications::send_tg_public(
                        &state.http_client,
                        token,
                        chat.chat_id,
                        &msg,
                    )
                    .await;
                }
            }
        }
    }

    Ok(Json(updated))
}

pub async fn delete_note(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.notes.len();
    config.notes.retain(|n| n.id != id);
    if config.notes.len() == before {
        return Err((StatusCode::NOT_FOUND, "Nota no encontrada".to_string()));
    }
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::NO_CONTENT)
}
