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

// yt-dlp JSON output
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

fn extract_username(
    sessions: &std::collections::HashMap<String, crate::state::SessionInfo>,
    headers: &axum::http::HeaderMap,
) -> String {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| sessions.get(t).map(|s| s.username.clone()))
        .unwrap_or_else(|| "alguien".to_string())
}

/// Mata el proceso mpv actual si existe
async fn kill_player(state: &AppState) {
    let mut proc = state.music_process.lock().await;
    if let Some(ref mut child) = *proc {
        let _ = child.kill().await;
    }
    *proc = None;
}

/// Lanza mpv en el NAS para reproducir audio
async fn spawn_player(state: &AppState, video_id: &str) {
    kill_player(state).await;

    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    // mpv --no-video usa yt-dlp internamente para resolver el stream
    let child = Command::new("mpv")
        .args(["--no-video", "--really-quiet", &url])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    if let Ok(child) = child {
        *state.music_process.lock().await = Some(child);
    }
}

fn add_to_history(ms: &mut MusicState, track: &MusicTrack, played_by: &str) {
    ms.history.push(HistoryEntry {
        id: track.id.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        thumbnail: track.thumbnail.clone(),
        played_by: played_by.to_string(),
    });
    if ms.history.len() > MAX_HISTORY {
        ms.history.remove(0);
    }
}

async fn fetch_track_info(id: &str) -> Result<MusicTrack, (StatusCode, String)> {
    let output = Command::new("yt-dlp")
        .args([
            "--flat-playlist", "--dump-json", "--no-warnings", "--no-playlist",
            &format!("https://www.youtube.com/watch?v={}", id),
        ])
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error yt-dlp: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entry: YtDlpEntry = stdout
        .lines()
        .next()
        .and_then(|line| serde_json::from_str(line).ok())
        .ok_or((StatusCode::NOT_FOUND, "No se encontro info del video".to_string()))?;

    let (thumb, artist) = extract_track_info(&entry);
    Ok(MusicTrack {
        id: entry.id,
        title: entry.title,
        artist,
        thumbnail: thumb,
        duration: entry.duration.unwrap_or(0.0) as u32,
        added_by: None,
    })
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
                added_by: None,
            })
        })
        .collect();

    Ok(Json(tracks))
}

/// POST /api/music/play - Reproduce en el NAS o agrega a la cola
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
        // Ya hay algo → agregar a la cola
        let mut track = fetch_track_info(&req.id).await?;
        track.added_by = Some(username);
        ms.queue.push(track);
        return Ok(Json(ms.clone()));
    }

    // Nada reproduciéndose → reproducir ahora
    let mut track = fetch_track_info(&req.id).await?;
    track.added_by = Some(username.clone());

    add_to_history(&mut ms, &track, &username);
    ms.current = Some(track.clone());
    ms.started_by = Some(username.clone());
    drop(ms);

    // Lanzar mpv en el NAS
    spawn_player(&state, &req.id).await;

    state.log_activity("musica", &format!("Reproduciendo: {}", track.title), &username).await;

    let ms = state.music.lock().await;
    Ok(Json(ms.clone()))
}

/// POST /api/music/next - Siguiente canción
pub async fn next(
    State(state): State<AppState>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    kill_player(&state).await;

    let mut ms = state.music.lock().await;

    if ms.queue.is_empty() {
        ms.current = None;
        ms.started_by = None;
        return Ok(Json(ms.clone()));
    }

    let next_track = ms.queue.remove(0);
    let next_id = next_track.id.clone();
    let next_by = next_track.added_by.clone().unwrap_or_default();

    add_to_history(&mut ms, &next_track, &next_by);
    ms.current = Some(next_track);
    ms.started_by = Some(next_by);
    drop(ms);

    spawn_player(&state, &next_id).await;

    let ms = state.music.lock().await;
    Ok(Json(ms.clone()))
}

/// DELETE /api/music/queue - Quitar de la cola
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
    // Verificar si mpv sigue corriendo; si terminó, pasar a la siguiente
    let mut proc = state.music_process.lock().await;
    let finished = if let Some(ref mut child) = *proc {
        match child.try_wait() {
            Ok(Some(_)) => true,  // terminó
            _ => false,
        }
    } else {
        false
    };
    if finished {
        *proc = None;
    }
    drop(proc);

    if finished {
        // Auto-next
        let mut ms = state.music.lock().await;
        if !ms.queue.is_empty() {
            let next_track = ms.queue.remove(0);
            let next_id = next_track.id.clone();
            let next_by = next_track.added_by.clone().unwrap_or_default();
            add_to_history(&mut ms, &next_track, &next_by);
            ms.current = Some(next_track);
            ms.started_by = Some(next_by);
            drop(ms);
            spawn_player(&state, &next_id).await;
        } else {
            ms.current = None;
            ms.started_by = None;
        }
    }

    Json(state.music.lock().await.clone())
}

/// GET /api/music/history
pub async fn history(
    State(state): State<AppState>,
) -> Json<Vec<HistoryEntry>> {
    Json(state.music.lock().await.history.clone())
}

/// POST /api/music/stop
pub async fn stop(
    State(state): State<AppState>,
) -> Json<MusicState> {
    kill_player(&state).await;
    let mut ms = state.music.lock().await;
    let history = ms.history.clone();
    *ms = MusicState::default();
    ms.history = history;
    Json(ms.clone())
}

/// POST /api/music/recommend - Mix basado en canción actual/historial
pub async fn recommend(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    let ms = state.music.lock().await;

    let seed_id = ms.current.as_ref().map(|t| t.id.clone())
        .or_else(|| ms.history.last().map(|h| h.id.clone()))
        .ok_or((StatusCode::BAD_REQUEST, "Reproduce algo primero".to_string()))?;

    let mut existing: std::collections::HashSet<String> = ms.queue.iter().map(|t| t.id.clone()).collect();
    if let Some(ref c) = ms.current { existing.insert(c.id.clone()); }
    for h in ms.history.iter().rev().take(10) { existing.insert(h.id.clone()); }
    drop(ms);

    let username = {
        let sessions = state.sessions.lock().await;
        extract_username(&sessions, &headers)
    };

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
            if entry.id.is_empty() || existing.contains(&entry.id) { return None; }
            let (thumb, artist) = extract_track_info(&entry);
            Some(MusicTrack {
                id: entry.id,
                title: entry.title,
                artist,
                thumbnail: thumb,
                duration: entry.duration.unwrap_or(0.0) as u32,
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

    state.log_activity("musica", &format!("Mix: {} recomendaciones", added), &username).await;
    Ok(Json(ms.clone()))
}
