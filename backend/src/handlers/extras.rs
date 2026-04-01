use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use rusqlite::params;
use serde::Deserialize;
use std::time::Instant;

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

pub async fn list_notes(
    State(state): State<AppState>,
) -> Result<Json<Vec<Note>>, (StatusCode, String)> {
    let notes = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, created_by, updated_by, shared_with, is_public, created_at, updated_at
             FROM notes"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([], |row| {
            let shared_str: String = row.get(5)?;
            let shared_with: Vec<String> = serde_json::from_str(&shared_str).unwrap_or_default();
            let created_str: String = row.get(7)?;
            let updated_str: String = row.get(8)?;
            Ok(Note {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                created_by: row.get(3)?,
                updated_by: row.get(4)?,
                shared_with,
                is_public: row.get(6)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        }).map_err(|e| e.to_string())?;

        let mut notes = Vec::new();
        for row in rows {
            notes.push(row.map_err(|e| e.to_string())?);
        }
        Ok(notes)
    }).await?;

    Ok(Json(notes))
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

    let note_clone = note.clone();
    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO notes (id, title, content, created_by, updated_by, shared_with, is_public, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &note_clone.id, &note_clone.title, &note_clone.content,
                &note_clone.created_by, &note_clone.updated_by,
                serde_json::to_string(&note_clone.shared_with).unwrap_or_default(),
                note_clone.is_public,
                note_clone.created_at.to_rfc3339(), note_clone.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await?;

    // Notificar a usuarios compartidos por Telegram
    if !req.shared_with.is_empty() {
        // Read bot_token and chats from DB
        let tg_data = crate::db::db_op(&state.db, |conn| {
            let token: Option<String> = conn.query_row(
                "SELECT bot_token FROM notification_config WHERE id = 1",
                [],
                |row| row.get(0),
            ).ok();

            let mut stmt = conn.prepare("SELECT chat_id, name, linked_web_user FROM telegram_chats")
                .map_err(|e| e.to_string())?;
            let chats: Vec<(i64, String, Option<String>)> = stmt.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            }).map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

            Ok((token, chats))
        }).await;

        if let Ok((Some(token), chats)) = tg_data {
            let msg = format!(
                "\u{1f4dd} *Nota compartida*\n\n*{}*\nPor: {}",
                note.title, username
            );
            for (chat_id, name, linked_web_user) in &chats {
                let is_target = req.shared_with.iter().any(|u| {
                    u.to_lowercase() == name.to_lowercase()
                        || linked_web_user
                            .as_ref()
                            .map(|w| w.to_lowercase() == u.to_lowercase())
                            .unwrap_or(false)
                });
                if is_target {
                    let _ = crate::handlers::notifications::send_tg_public(
                        &state.http_client,
                        &token,
                        *chat_id,
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

    // Read current note from DB
    let id_clone = id.clone();
    let current_note = crate::db::db_op_status(&state.db, move |conn| {
        conn.query_row(
            "SELECT id, title, content, created_by, updated_by, shared_with, is_public, created_at, updated_at
             FROM notes WHERE id = ?1",
            params![&id_clone],
            |row| {
                let shared_str: String = row.get(5)?;
                let shared_with: Vec<String> = serde_json::from_str(&shared_str).unwrap_or_default();
                let created_str: String = row.get(7)?;
                let updated_str: String = row.get(8)?;
                Ok(Note {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    created_by: row.get(3)?,
                    updated_by: row.get(4)?,
                    shared_with,
                    is_public: row.get(6)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            },
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => (StatusCode::NOT_FOUND, "Nota no encontrada".to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        })
    }).await?;

    // Build updated note
    let mut updated = current_note;
    if let Some(title) = req.title {
        updated.title = title;
    }
    if let Some(content) = req.content {
        updated.content = content;
    }

    // Detectar nuevos usuarios compartidos para notificar
    let old_shared: std::collections::HashSet<String> =
        updated.shared_with.iter().map(|s| s.to_lowercase()).collect();
    let mut new_users: Vec<String> = Vec::new();

    if let Some(shared_with) = req.shared_with {
        for u in &shared_with {
            if !old_shared.contains(&u.to_lowercase()) {
                new_users.push(u.clone());
            }
        }
        updated.shared_with = shared_with;
    }
    if let Some(is_public) = req.is_public {
        updated.is_public = is_public;
    }
    updated.updated_by = username.clone();
    updated.updated_at = Utc::now();

    // Save to DB
    let note_for_db = updated.clone();
    let id_clone2 = id.clone();
    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE notes SET title = ?1, content = ?2, updated_by = ?3, shared_with = ?4, is_public = ?5, updated_at = ?6
             WHERE id = ?7",
            params![
                &note_for_db.title, &note_for_db.content, &note_for_db.updated_by,
                serde_json::to_string(&note_for_db.shared_with).unwrap_or_default(),
                note_for_db.is_public, note_for_db.updated_at.to_rfc3339(),
                &id_clone2,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await?;

    // Notificar nuevos compartidos
    if !new_users.is_empty() {
        let tg_data = crate::db::db_op(&state.db, |conn| {
            let token: Option<String> = conn.query_row(
                "SELECT bot_token FROM notification_config WHERE id = 1",
                [],
                |row| row.get(0),
            ).ok();

            let mut stmt = conn.prepare("SELECT chat_id, name, linked_web_user FROM telegram_chats")
                .map_err(|e| e.to_string())?;
            let chats: Vec<(i64, String, Option<String>)> = stmt.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            }).map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

            Ok((token, chats))
        }).await;

        if let Ok((Some(token), chats)) = tg_data {
            let msg = format!(
                "\u{1f4dd} *Nota compartida*\n\n*{}*\nPor: {}",
                updated.title, username
            );
            for (chat_id, name, linked_web_user) in &chats {
                let is_target = new_users.iter().any(|u| {
                    u.to_lowercase() == name.to_lowercase()
                        || linked_web_user
                            .as_ref()
                            .map(|w| w.to_lowercase() == u.to_lowercase())
                            .unwrap_or(false)
                });
                if is_target {
                    let _ = crate::handlers::notifications::send_tg_public(
                        &state.http_client,
                        &token,
                        *chat_id,
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
    crate::db::db_op_status(&state.db, move |conn| {
        let rows = conn.execute(
            "DELETE FROM notes WHERE id = ?1",
            params![&id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if rows == 0 {
            return Err((StatusCode::NOT_FOUND, "Nota no encontrada".to_string()));
        }
        Ok(())
    }).await?;

    Ok(StatusCode::NO_CONTENT)
}
