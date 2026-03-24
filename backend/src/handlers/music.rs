use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::state::AppState;

// --- Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub thumbnail: String,
    pub duration: u32,
    pub stream_url: Option<String>,
    #[serde(default)]
    pub added_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub thumbnail: String,
    pub played_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MusicState {
    pub current: Option<MusicTrack>,
    pub queue: Vec<MusicTrack>,
    pub started_by: Option<String>,
    pub history: Vec<HistoryEntry>,
}

const MAX_HISTORY: usize = 50;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Debug, Deserialize)]
pub struct PlayRequest {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct QueueRemoveRequest {
    pub index: usize,
}

// yt-dlp JSON output fields
#[derive(Debug, Deserialize)]
struct YtDlpEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    uploader: String,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    thumbnail: String,
    #[serde(default)]
    thumbnails: Vec<YtDlpThumb>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YtDlpThumb {
    #[serde(default)]
    url: String,
}

fn extract_track_info(entry: &YtDlpEntry) -> (String, String) {
    let thumb = if !entry.thumbnail.is_empty() {
        entry.thumbnail.clone()
    } else {
        entry.thumbnails.last().map(|t| t.url.clone()).unwrap_or_default()
    };
    let artist = if !entry.uploader.is_empty() {
        entry.uploader.clone()
    } else {
        entry.channel.clone()
    };
    (thumb, artist)
}

fn extract_username(sessions: &std::collections::HashMap<String, crate::state::SessionInfo>, headers: &axum::http::HeaderMap) -> String {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| sessions.get(t).map(|s| s.username.clone()))
        .unwrap_or_else(|| "alguien".to_string())
}

async fn resolve_stream(id: &str) -> Result<(String, YtDlpEntry), (StatusCode, String)> {
    let video_url = format!("https://www.youtube.com/watch?v={}", id);

    let output = Command::new("yt-dlp")
        .args(["-f", "bestaudio", "-g", "--dump-json", "--no-warnings", "--no-playlist", &video_url])
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error yt-dlp: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err((StatusCode::BAD_GATEWAY, format!("yt-dlp: {}", stderr)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    if lines.len() < 2 {
        return Err((StatusCode::NOT_FOUND, "No se pudo obtener audio".to_string()));
    }

    let audio_url = lines[0].to_string();
    let info: YtDlpEntry = serde_json::from_str(lines[1])
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error parseando info".to_string()))?;

    Ok((audio_url, info))
}

// --- Handlers ---

/// GET /api/music/search?q=...
pub async fn search(
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<MusicTrack>>, (StatusCode, String)> {
    let search_term = format!("ytsearch15:{}", query.q);

    let output = Command::new("yt-dlp")
        .args(["--flat-playlist", "--dump-json", "--no-warnings", "--ignore-errors", &search_term])
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error yt-dlp: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err((StatusCode::BAD_GATEWAY, format!("yt-dlp: {}", stderr)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let tracks: Vec<MusicTrack> = stdout
        .lines()
        .filter_map(|line| {
            let entry: YtDlpEntry = serde_json::from_str(line).ok()?;
            if entry.id.is_empty() { return None; }
            let (thumb, artist) = extract_track_info(&entry);
            Some(MusicTrack {
                id: entry.id,
                title: entry.title,
                artist,
                thumbnail: thumb,
                duration: entry.duration.unwrap_or(0.0) as u32,
                stream_url: None,
                added_by: None,
            })
        })
        .collect();

    Ok(Json(tracks))
}

/// POST /api/music/play - Reproduce inmediatamente (si no hay nada) o agrega a la cola
pub async fn play(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<PlayRequest>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    let username = {
        let sessions = state.sessions.lock().await;
        extract_username(&sessions, &headers)
    };

    let mut ms = state.music.lock().await;

    if ms.current.is_some() {
        // Ya hay algo reproduciéndose → agregar a la cola (sin resolver stream aún)
        // Necesitamos info básica, la obtenemos con flat-playlist
        let output = Command::new("yt-dlp")
            .args(["--flat-playlist", "--dump-json", "--no-warnings", "--no-playlist",
                   &format!("https://www.youtube.com/watch?v={}", req.id)])
            .output()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error yt-dlp: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = stdout.lines().next() {
            if let Ok(entry) = serde_json::from_str::<YtDlpEntry>(line) {
                let (thumb, artist) = extract_track_info(&entry);
                ms.queue.push(MusicTrack {
                    id: req.id,
                    title: entry.title,
                    artist,
                    thumbnail: thumb,
                    duration: entry.duration.unwrap_or(0.0) as u32,
                    stream_url: None,
                    added_by: Some(username),
                });
            }
        }
        return Ok(Json(ms.clone()));
    }

    // Nada reproduciéndose → resolver stream y reproducir
    drop(ms);
    let (audio_url, info) = resolve_stream(&req.id).await?;
    let (thumb, artist) = extract_track_info(&info);

    let track = MusicTrack {
        id: req.id,
        title: info.title.clone(),
        artist,
        thumbnail: thumb,
        duration: info.duration.unwrap_or(0.0) as u32,
        stream_url: Some(audio_url),
        added_by: Some(username.clone()),
    };

    let mut ms = state.music.lock().await;
    // Guardar en historial
    ms.history.push(HistoryEntry {
        id: track.id.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        thumbnail: track.thumbnail.clone(),
        played_by: username.clone(),
    });
    if ms.history.len() > MAX_HISTORY {
        ms.history.remove(0);
    }
    ms.current = Some(track);
    ms.started_by = Some(username.clone());

    state.log_activity("musica", &format!("Reproduciendo: {}", info.title), &username).await;

    Ok(Json(ms.clone()))
}

/// POST /api/music/next - Pasar a la siguiente canción de la cola
pub async fn next(
    State(state): State<AppState>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    let mut ms = state.music.lock().await;

    if ms.queue.is_empty() {
        ms.current = None;
        ms.started_by = None;
        return Ok(Json(ms.clone()));
    }

    let next_track = ms.queue.remove(0);
    let next_id = next_track.id.clone();
    let next_by = next_track.added_by.clone();
    drop(ms);

    // Resolver stream de la siguiente
    let (audio_url, info) = resolve_stream(&next_id).await?;
    let (thumb, artist) = extract_track_info(&info);

    let track = MusicTrack {
        id: next_id,
        title: info.title,
        artist,
        thumbnail: thumb,
        duration: info.duration.unwrap_or(0.0) as u32,
        stream_url: Some(audio_url),
        added_by: next_by.clone(),
    };

    let mut ms = state.music.lock().await;
    ms.history.push(HistoryEntry {
        id: track.id.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        thumbnail: track.thumbnail.clone(),
        played_by: next_by.clone().unwrap_or_default(),
    });
    if ms.history.len() > MAX_HISTORY {
        ms.history.remove(0);
    }
    ms.current = Some(track);
    ms.started_by = next_by;

    Ok(Json(ms.clone()))
}

/// DELETE /api/music/queue - Quitar un track de la cola por índice
pub async fn queue_remove(
    State(state): State<AppState>,
    Json(req): Json<QueueRemoveRequest>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    let mut ms = state.music.lock().await;
    if req.index < ms.queue.len() {
        ms.queue.remove(req.index);
    }
    Ok(Json(ms.clone()))
}

/// GET /api/music/current
pub async fn current(
    State(state): State<AppState>,
) -> Json<MusicState> {
    Json(state.music.lock().await.clone())
}

/// POST /api/music/stop
pub async fn stop(
    State(state): State<AppState>,
) -> Json<MusicState> {
    let mut ms = state.music.lock().await;
    let history = ms.history.clone();
    *ms = MusicState::default();
    ms.history = history; // preservar historial
    Json(ms.clone())
}

/// GET /api/music/history
pub async fn history(
    State(state): State<AppState>,
) -> Json<Vec<HistoryEntry>> {
    let ms = state.music.lock().await;
    Json(ms.history.clone())
}

/// POST /api/music/recommend - Llenar cola con mix basado en la canción actual o última del historial
pub async fn recommend(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    let ms = state.music.lock().await;

    // Elegir semilla: canción actual, o la última del historial
    let seed_id = ms.current.as_ref().map(|t| t.id.clone())
        .or_else(|| ms.history.last().map(|h| h.id.clone()))
        .ok_or((StatusCode::BAD_REQUEST, "No hay canciones para recomendar. Reproduce algo primero.".to_string()))?;

    // IDs ya en cola o reproduciéndose para no repetir
    let mut existing: std::collections::HashSet<String> = ms.queue.iter().map(|t| t.id.clone()).collect();
    if let Some(ref c) = ms.current {
        existing.insert(c.id.clone());
    }
    // También excluir las últimas 10 del historial
    for h in ms.history.iter().rev().take(10) {
        existing.insert(h.id.clone());
    }
    drop(ms);

    let username = {
        let sessions = state.sessions.lock().await;
        extract_username(&sessions, &headers)
    };

    // YouTube Mix: playlist auto-generada basada en un video
    let mix_url = format!("https://www.youtube.com/watch?v={}&list=RD{}", seed_id, seed_id);

    let output = Command::new("yt-dlp")
        .args(["--flat-playlist", "--dump-json", "--no-warnings", "--ignore-errors", &mix_url])
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error yt-dlp: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let recommended: Vec<MusicTrack> = stdout
        .lines()
        .filter_map(|line| {
            let entry: YtDlpEntry = serde_json::from_str(line).ok()?;
            if entry.id.is_empty() || existing.contains(&entry.id) {
                return None;
            }
            let (thumb, artist) = extract_track_info(&entry);
            Some(MusicTrack {
                id: entry.id,
                title: entry.title,
                artist,
                thumbnail: thumb,
                duration: entry.duration.unwrap_or(0.0) as u32,
                stream_url: None,
                added_by: Some(format!("Mix ({})", username)),
            })
        })
        .take(20)
        .collect();

    if recommended.is_empty() {
        return Err((StatusCode::NOT_FOUND, "No se encontraron recomendaciones".to_string()));
    }

    let mut ms = state.music.lock().await;
    let added = recommended.len();
    ms.queue.extend(recommended);

    state.log_activity("musica", &format!("Mix: {} canciones recomendadas agregadas", added), &username).await;

    Ok(Json(ms.clone()))
}
