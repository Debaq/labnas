//! Playlists guardadas en la base

use super::*;

// ══════════════════════════════════════════
// Playlists (persisted in DB)
// ══════════════════════════════════════════

use rusqlite::params;
use axum::http::HeaderMap;

pub(super) async fn get_session_username(state: &AppState, headers: &HeaderMap) -> String {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());
    if let Some(token) = token {
        let sessions = state.sessions.lock().await;
        if let Some(session) = sessions.get(&token) {
            return session.username.clone();
        }
    }
    "unknown".to_string()
}

pub(super) fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Helper para leer una fila de playlist
pub(super) fn row_to_playlist(row: &rusqlite::Row) -> rusqlite::Result<Playlist> {
    let tracks_json: String = row.get(4)?;
    let tracks: Vec<PlaylistTrack> = serde_json::from_str(&tracks_json).unwrap_or_default();
    Ok(Playlist {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        created_by: row.get(3)?,
        tracks,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

/// GET /api/music/playlists
pub async fn list_playlists(State(state): State<AppState>) -> Result<Json<Vec<Playlist>>, (StatusCode, String)> {
    let playlists = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, created_by, tracks, created_at, updated_at FROM playlists ORDER BY created_at"
        ).map_err(|e| format!("DB: {}", e))?;
        let rows = stmt.query_map([], row_to_playlist)
            .map_err(|e| format!("DB: {}", e))?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row.map_err(|e| format!("DB row: {}", e))?);
        }
        Ok(list)
    }).await?;
    Ok(Json(playlists))
}

#[derive(Debug, Deserialize)]
pub struct CreatePlaylistReq {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// POST /api/music/playlists
pub async fn create_playlist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreatePlaylistReq>,
) -> Result<Json<Playlist>, (StatusCode, String)> {
    if req.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Nombre requerido".to_string()));
    }
    let username = get_session_username(&state, &headers).await;
    let now = now_iso();
    let playlist = Playlist {
        id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
        name: req.name.trim().to_string(),
        description: req.description,
        created_by: username,
        tracks: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    let pl = playlist.clone();
    crate::db::db_op(&state.db, move |conn| {
        let tracks_json = serde_json::to_string(&pl.tracks).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO playlists (id, name, description, created_by, tracks, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![pl.id, pl.name, pl.description, pl.created_by, tracks_json, pl.created_at, pl.updated_at],
        ).map_err(|e| format!("DB: {}", e))?;
        Ok(())
    }).await?;
    Ok(Json(playlist))
}

#[derive(Debug, Deserialize)]
pub struct UpdatePlaylistReq {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// PUT /api/music/playlists/{id}
pub async fn update_playlist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePlaylistReq>,
) -> Result<Json<Playlist>, (StatusCode, String)> {
    let now = now_iso();
    let result = crate::db::db_op_status(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        let mut pl: Playlist = conn.query_row(
            "SELECT id, name, description, created_by, tracks, created_at, updated_at FROM playlists WHERE id = ?1",
            params![id],
            row_to_playlist,
        ).optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
            .ok_or((StatusCode::NOT_FOUND, "Playlist no encontrada".to_string()))?;

        if let Some(name) = req.name { pl.name = name; }
        if let Some(desc) = req.description { pl.description = desc; }
        pl.updated_at = now;

        conn.execute(
            "UPDATE playlists SET name=?1, description=?2, updated_at=?3 WHERE id=?4",
            params![pl.name, pl.description, pl.updated_at, pl.id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        Ok(pl)
    }).await?;
    Ok(Json(result))
}

/// DELETE /api/music/playlists/{id}
pub async fn delete_playlist(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op_status(&state.db, move |conn| {
        let changes = conn.execute("DELETE FROM playlists WHERE id = ?1", params![&id])
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;
        if changes == 0 {
            return Err((StatusCode::NOT_FOUND, "Playlist no encontrada".to_string()));
        }
        Ok(())
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct AddTrackReq {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub duration: u32,
}

/// POST /api/music/playlists/{id}/tracks
pub async fn add_track_to_playlist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<AddTrackReq>,
) -> Result<Json<Playlist>, (StatusCode, String)> {
    let username = get_session_username(&state, &headers).await;
    let now = now_iso();
    let result = crate::db::db_op_status(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        let mut pl: Playlist = conn.query_row(
            "SELECT id, name, description, created_by, tracks, created_at, updated_at FROM playlists WHERE id = ?1",
            params![id],
            row_to_playlist,
        ).optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
            .ok_or((StatusCode::NOT_FOUND, "Playlist no encontrada".to_string()))?;

        pl.tracks.push(PlaylistTrack {
            id: req.id,
            title: req.title,
            artist: req.artist,
            thumbnail: req.thumbnail,
            duration: req.duration,
            added_by: username,
        });
        pl.updated_at = now;

        let tracks_json = serde_json::to_string(&pl.tracks).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE playlists SET tracks=?1, updated_at=?2 WHERE id=?3",
            params![tracks_json, pl.updated_at, pl.id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        Ok(pl)
    }).await?;
    Ok(Json(result))
}

/// DELETE /api/music/playlists/{id}/tracks/{index}
pub async fn remove_track_from_playlist(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, usize)>,
) -> Result<Json<Playlist>, (StatusCode, String)> {
    let now = now_iso();
    let result = crate::db::db_op_status(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        let mut pl: Playlist = conn.query_row(
            "SELECT id, name, description, created_by, tracks, created_at, updated_at FROM playlists WHERE id = ?1",
            params![id],
            row_to_playlist,
        ).optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
            .ok_or((StatusCode::NOT_FOUND, "Playlist no encontrada".to_string()))?;

        if index >= pl.tracks.len() {
            return Err((StatusCode::BAD_REQUEST, "Indice fuera de rango".to_string()));
        }
        pl.tracks.remove(index);
        pl.updated_at = now;

        let tracks_json = serde_json::to_string(&pl.tracks).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE playlists SET tracks=?1, updated_at=?2 WHERE id=?3",
            params![tracks_json, pl.updated_at, pl.id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        Ok(pl)
    }).await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct MoveTrackReq {
    pub from: usize,
    pub to: usize,
}

/// POST /api/music/playlists/{id}/tracks/move
pub async fn move_track_in_playlist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<MoveTrackReq>,
) -> Result<Json<Playlist>, (StatusCode, String)> {
    let now = now_iso();
    let result = crate::db::db_op_status(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        let mut pl: Playlist = conn.query_row(
            "SELECT id, name, description, created_by, tracks, created_at, updated_at FROM playlists WHERE id = ?1",
            params![id],
            row_to_playlist,
        ).optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
            .ok_or((StatusCode::NOT_FOUND, "Playlist no encontrada".to_string()))?;

        if req.from >= pl.tracks.len() || req.to >= pl.tracks.len() {
            return Err((StatusCode::BAD_REQUEST, "Indice fuera de rango".to_string()));
        }
        let track = pl.tracks.remove(req.from);
        pl.tracks.insert(req.to, track);
        pl.updated_at = now;

        let tracks_json = serde_json::to_string(&pl.tracks).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE playlists SET tracks=?1, updated_at=?2 WHERE id=?3",
            params![tracks_json, pl.updated_at, pl.id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        Ok(pl)
    }).await?;
    Ok(Json(result))
}

/// POST /api/music/playlists/{id}/load - Load playlist into queue
pub async fn load_playlist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let username = get_session_username(&state, &headers).await;
    let pl = crate::db::db_op_status(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT id, name, description, created_by, tracks, created_at, updated_at FROM playlists WHERE id = ?1",
            params![id],
            row_to_playlist,
        ).optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
            .ok_or((StatusCode::NOT_FOUND, "Playlist no encontrada".to_string()))
    }).await?;

    let tracks: Vec<MusicTrack> = pl.tracks.iter().map(|t| MusicTrack {
        id: t.id.clone(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        thumbnail: t.thumbnail.clone(),
        duration: t.duration,
        added_by: Some(username.clone()),
    }).collect();
    let count = tracks.len();

    let mut music = state.music.lock().await;
    music.queue.extend(tracks);
    drop(music);

    Ok((StatusCode::OK, format!("{} canciones agregadas a la cola", count)))
}

#[derive(Debug, Deserialize)]
pub struct SaveQueueReq {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// POST /api/music/playlists/save-queue - Save current queue + current as playlist
pub async fn save_queue_as_playlist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SaveQueueReq>,
) -> Result<Json<Playlist>, (StatusCode, String)> {
    if req.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Nombre requerido".to_string()));
    }
    let username = get_session_username(&state, &headers).await;
    let music = state.music.lock().await;

    let mut tracks: Vec<PlaylistTrack> = Vec::new();
    // Add current track first
    if let Some(ref current) = music.current {
        tracks.push(PlaylistTrack {
            id: current.id.clone(),
            title: current.title.clone(),
            artist: current.artist.clone(),
            thumbnail: current.thumbnail.clone(),
            duration: current.duration,
            added_by: username.clone(),
        });
    }
    // Add queue
    for t in &music.queue {
        tracks.push(PlaylistTrack {
            id: t.id.clone(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            thumbnail: t.thumbnail.clone(),
            duration: t.duration,
            added_by: t.added_by.clone().unwrap_or_else(|| username.clone()),
        });
    }
    drop(music);

    if tracks.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No hay canciones para guardar".to_string()));
    }

    let now = now_iso();
    let playlist = Playlist {
        id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
        name: req.name.trim().to_string(),
        description: req.description,
        created_by: username,
        tracks,
        created_at: now.clone(),
        updated_at: now,
    };

    let pl = playlist.clone();
    crate::db::db_op(&state.db, move |conn| {
        let tracks_json = serde_json::to_string(&pl.tracks).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO playlists (id, name, description, created_by, tracks, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![pl.id, pl.name, pl.description, pl.created_by, tracks_json, pl.created_at, pl.updated_at],
        ).map_err(|e| format!("DB: {}", e))?;
        Ok(())
    }).await?;
    Ok(Json(playlist))
}
