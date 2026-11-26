use axum::{
    extract::{Path, State},
    Extension,
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
use crate::error::internal;

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
    Extension(session): Extension<crate::state::SessionInfo>,
    Json(req): Json<ShareRequest>,
) -> Result<Json<ShareResponse>, (StatusCode, String)> {
    let st = crate::storage::Storage::load(&state.db).await?;
    let path = st.resolve_existing_for(&session, &req.path, crate::storage::Op::Read)?;
    if path.is_dir() {
        return Err((StatusCode::NOT_FOUND, "Archivo no encontrado".to_string()));
    }

    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let token = uuid::Uuid::new_v4().to_string();
    let hours = req.expires_hours.clamp(1, 168); // 1h to 7 days

    {
        let mut shares = state.share_links.lock().await;
        // Purga los vencidos (antes solo se borraban al intentar descargarlos)
        shares.retain(|_, s| !s.is_expired());
        shares.insert(
            token.clone(),
            crate::state::ShareLink {
                owner: session.username.clone(),
                file_path: path.to_string_lossy().to_string(),
                file_name: file_name.clone(),
                created_at: Instant::now(),
                expires_secs: (hours as u64) * 3600,
            },
        );
    }

    state
        .log_activity("Compartido", &format!("{} ({}h)", file_name, hours), &session.username)
        .await;

    Ok(Json(ShareResponse {
        // Relativa: el cliente le antepone el origen con que entro (http/https, IP, .local, puerto)
        url: share_path(&token),
        token,
        expires_hours: hours,
    }))
}

fn share_path(token: &str) -> String {
    format!("/api/share/{}", token)
}

pub async fn download_share(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let share = {
        let mut shares = state.share_links.lock().await;
        let share = shares
            .get(&token)
            .cloned()
            .ok_or((StatusCode::NOT_FOUND, "Link no encontrado o expirado".to_string()))?;
        if share.is_expired() {
            shares.remove(&token);
            return Err((StatusCode::GONE, "Link expirado".to_string()));
        }
        share
    };

    let file = tokio::fs::File::open(&share.file_path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Archivo no disponible".to_string()))?;
    let len = file.metadata().await.map(|m| m.len()).ok();
    // En streaming: antes se leia el archivo completo a memoria
    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file));

    // Nombre ASCII de respaldo; el real va en filename* (RFC 5987)
    let ascii_name: String = share
        .file_name
        .chars()
        .map(|c| if (c.is_ascii_graphic() || c == ' ') && c != '"' && c != '\\' { c } else { '_' })
        .collect();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/octet-stream"));
    if let Ok(v) = header::HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        ascii_name,
        urlencoding::encode(&share.file_name)
    )) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    if let Some(len) = len {
        headers.insert(header::CONTENT_LENGTH, header::HeaderValue::from(len));
    }

    Ok((headers, body))
}

/// Links vigentes: los propios, o todos si es admin
pub async fn list_shares(
    State(state): State<AppState>,
    Extension(session): Extension<crate::state::SessionInfo>,
) -> Json<Vec<serde_json::Value>> {
    let is_admin = session.role.is_admin();
    let shares = state.share_links.lock().await;

    let list: Vec<serde_json::Value> = shares
        .iter()
        .filter(|(_, s)| !s.is_expired() && (is_admin || s.owner == session.username))
        .map(|(token, s)| {
            let remaining = s.expires_secs.saturating_sub(s.created_at.elapsed().as_secs());
            serde_json::json!({
                "token": token,
                "file_name": s.file_name,
                "file_path": s.file_path,
                "owner": s.owner,
                "url": share_path(token),
                "remaining_minutes": remaining / 60,
            })
        })
        .collect();

    Json(list)
}

/// Solo el dueño o un admin pueden revocar un link
pub async fn delete_share(
    State(state): State<AppState>,
    Extension(session): Extension<crate::state::SessionInfo>,
    Path(token): Path<String>,
) -> StatusCode {
    let mut shares = state.share_links.lock().await;
    match shares.get(&token) {
        Some(s) if s.owner == session.username || session.role.is_admin() => {
            shares.remove(&token);
            StatusCode::NO_CONTENT
        }
        // Igual que inexistente: no revela tokens ajenos
        _ => StatusCode::NOT_FOUND,
    }
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
    Extension(session): Extension<crate::state::SessionInfo>,
    Json(req): Json<DownloadUrlRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let st = crate::storage::Storage::load(&state.db).await?;
    let dest = st.resolve_for(&session, &req.destination, crate::storage::Op::Write)?;

    let url = reqwest::Url::parse(req.url.trim())
        .map_err(|_| (StatusCode::BAD_REQUEST, "URL invalida".to_string()))?;
    crate::netguard::check_url(&url).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let file_name = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .and_then(|seg| urlencoding::decode(seg).ok().map(|c| c.into_owned()))
        .and_then(|n| crate::storage::sanitize_filename(&n))
        .unwrap_or_else(|| "descarga".to_string());

    // Mismo tope que una subida (setting upload_limit_mb)
    let limit_mb = crate::db::db_op(&state.db, |conn| {
        Ok(crate::db::get_setting_u32(conn, "upload_limit_mb", 50))
    })
    .await?;
    let max_bytes = (limit_mb as u64) * 1024 * 1024;

    // Solo internet publica (ver netguard): nada de localhost, LAN ni Tailscale
    let mut response = crate::netguard::public_client()
        .get(url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Error descargando: {}", e)))?;

    if !response.status().is_success() {
        return Err((StatusCode::BAD_REQUEST, format!("URL respondio con {}", response.status())));
    }
    if response.content_length().is_some_and(|n| n > max_bytes) {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("El archivo supera el limite de {} MB", limit_mb)));
    }

    tokio::fs::create_dir_all(&dest)
        .await
        .map_err(internal)?;

    // Se escribe a un temporal nuevo (create_new no sigue symlinks) y se renombra al final
    let file_path = dest.join(&file_name);
    let part_path = dest.join(format!(".{}.{}.part", file_name, uuid::Uuid::new_v4().simple()));
    let mut part = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part_path)
        .await
        .map_err(internal)?;

    let result: Result<u64, (StatusCode, String)> = async {
        use tokio::io::AsyncWriteExt;
        let mut size: u64 = 0;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error leyendo datos: {}", e)))?
        {
            size += chunk.len() as u64;
            if size > max_bytes {
                return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("El archivo supera el limite de {} MB", limit_mb)));
            }
            part.write_all(&chunk)
                .await
                .map_err(internal)?;
        }
        part.flush().await.map_err(internal)?;
        Ok(size)
    }
    .await;
    drop(part);

    let size = match result {
        Ok(size) => size,
        Err(e) => {
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(e);
        }
    };
    if let Err(e) = tokio::fs::rename(&part_path, &file_path).await {
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    let msg = format!("{} ({} bytes)", file_name, size);
    state.log_activity("Descarga", &msg, &session.username).await;

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
                "SELECT labnas_decrypt(bot_token) FROM notification_config WHERE id = 1",
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
                "SELECT labnas_decrypt(bot_token) FROM notification_config WHERE id = 1",
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
        ).map_err(internal)?;

        if rows == 0 {
            return Err((StatusCode::NOT_FOUND, "Nota no encontrada".to_string()));
        }
        Ok(())
    }).await?;

    Ok(StatusCode::NO_CONTENT)
}
