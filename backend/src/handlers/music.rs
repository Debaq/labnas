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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlaybackMode {
    #[serde(rename = "nas")]
    Nas,
    #[serde(rename = "browser")]
    Browser,
}

impl Default for PlaybackMode {
    fn default() -> Self { Self::Nas }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MusicState {
    pub current: Option<MusicTrack>,
    pub queue: Vec<MusicTrack>,
    pub started_by: Option<String>,
    pub history: Vec<HistoryEntry>,
    pub mode: PlaybackMode,
    /// stream_url solo se llena en modo browser
    pub stream_url: Option<String>,
    #[serde(default)]
    pub paused: bool,
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

#[derive(Debug, Deserialize)]
pub struct SetModeRequest {
    pub mode: PlaybackMode,
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

/// Pausa el proceso mpv (SIGSTOP)
async fn pause_player(state: &AppState) {
    let proc = state.music_process.lock().await;
    if let Some(ref child) = *proc {
        if let Some(pid) = child.id() {
            unsafe { libc::kill(pid as i32, libc::SIGSTOP); }
        }
    }
}

/// Resume el proceso mpv (SIGCONT)
async fn resume_player(state: &AppState) {
    let proc = state.music_process.lock().await;
    if let Some(ref child) = *proc {
        if let Some(pid) = child.id() {
            unsafe { libc::kill(pid as i32, libc::SIGCONT); }
        }
    }
}

/// Lanza mpv en el NAS para reproducir audio (solo modo NAS)
async fn spawn_player(state: &AppState, video_id: &str) {
    kill_player(state).await;

    let url = format!("https://www.youtube.com/watch?v={}", video_id);
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

/// Obtiene la URL de audio directa para modo browser
async fn get_stream_url(video_id: &str) -> Option<String> {
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let output = Command::new("yt-dlp")
        .args(["-f", "bestaudio", "-g", "--no-warnings", "--no-playlist", &url])
        .output()
        .await
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().map(|s| s.to_string())
}

/// Inicia reproducción según el modo
async fn start_playback(state: &AppState, video_id: &str, mode: &PlaybackMode) -> Option<String> {
    match mode {
        PlaybackMode::Nas => {
            spawn_player(state, video_id).await;
            None
        }
        PlaybackMode::Browser => {
            kill_player(state).await;
            get_stream_url(video_id).await
        }
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
    let mode = ms.mode.clone();

    add_to_history(&mut ms, &track, &username);
    ms.current = Some(track.clone());
    ms.started_by = Some(username.clone());
    ms.paused = false;
    drop(ms);

    let stream = start_playback(&state, &req.id, &mode).await;

    let mut ms = state.music.lock().await;
    ms.stream_url = stream;

    state.log_activity("musica", &format!("Reproduciendo: {}", track.title), &username).await;

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
        ms.stream_url = None;
        return Ok(Json(ms.clone()));
    }

    let next_track = ms.queue.remove(0);
    let next_id = next_track.id.clone();
    let next_by = next_track.added_by.clone().unwrap_or_default();
    let mode = ms.mode.clone();

    add_to_history(&mut ms, &next_track, &next_by);
    ms.current = Some(next_track);
    ms.started_by = Some(next_by);
    ms.paused = false;
    drop(ms);

    let stream = start_playback(&state, &next_id, &mode).await;

    let mut ms = state.music.lock().await;
    ms.stream_url = stream;
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
    let mode = state.music.lock().await.mode.clone();

    // Verificar si mpv terminó (modo NAS)
    let mut player_finished = false;
    if mode == PlaybackMode::Nas {
        let mut proc = state.music_process.lock().await;
        let finished = if let Some(ref mut child) = *proc {
            matches!(child.try_wait(), Ok(Some(_)))
        } else {
            // No hay proceso pero hay current → se murió
            state.music.lock().await.current.is_some()
        };
        if finished {
            *proc = None;
            player_finished = true;
        }
        drop(proc);
    }

    // Auto-next si terminó la canción actual
    if player_finished {
        let mut ms = state.music.lock().await;
        if !ms.queue.is_empty() {
            let next_track = ms.queue.remove(0);
            let next_id = next_track.id.clone();
            let next_by = next_track.added_by.clone().unwrap_or_default();
            add_to_history(&mut ms, &next_track, &next_by);
            ms.current = Some(next_track);
            ms.started_by = Some(next_by);
            ms.paused = false;
            drop(ms);
            let stream = start_playback(&state, &next_id, &mode).await;
            let mut ms = state.music.lock().await;
            ms.stream_url = stream;
        } else {
            ms.current = None;
            ms.started_by = None;
            ms.stream_url = None;
            ms.paused = false;
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
    let mode = ms.mode.clone();
    *ms = MusicState::default();
    ms.history = history;
    ms.mode = mode;
    Json(ms.clone())
}

/// POST /api/music/pause - Pausar/reanudar
pub async fn pause(
    State(state): State<AppState>,
) -> Json<MusicState> {
    let mut ms = state.music.lock().await;
    if ms.current.is_none() {
        return Json(ms.clone());
    }

    ms.paused = !ms.paused;
    let paused = ms.paused;
    let mode = ms.mode.clone();
    drop(ms);

    if mode == PlaybackMode::Nas {
        if paused {
            pause_player(&state).await;
        } else {
            resume_player(&state).await;
        }
    }

    Json(state.music.lock().await.clone())
}

/// POST /api/music/previous - Volver a la canción anterior
pub async fn previous(
    State(state): State<AppState>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    kill_player(&state).await;

    let mut ms = state.music.lock().await;

    // Necesitamos al menos 1 entrada en historial (la actual) + 1 anterior
    // El historial guarda lo que ya sonó. La última entrada es la canción actual.
    // Queremos volver a la penúltima.
    if ms.history.len() < 2 {
        return Err((StatusCode::BAD_REQUEST, "No hay cancion anterior".to_string()));
    }

    // Devolver la actual a la cola (al frente)
    if let Some(current) = ms.current.take() {
        ms.queue.insert(0, current);
    }

    // Sacar la última del historial (es la actual que ya está en cola)
    ms.history.pop();
    // Sacar la penúltima (es la que queremos reproducir)
    let prev = ms.history.pop()
        .ok_or((StatusCode::BAD_REQUEST, "No hay cancion anterior".to_string()))?;

    let track = MusicTrack {
        id: prev.id.clone(),
        title: prev.title,
        artist: prev.artist,
        thumbnail: prev.thumbnail,
        duration: 0,
        added_by: Some(prev.played_by.clone()),
    };

    let mode = ms.mode.clone();
    add_to_history(&mut ms, &track, &prev.played_by);
    ms.current = Some(track);
    ms.started_by = Some(prev.played_by);
    ms.paused = false;
    drop(ms);

    let stream = start_playback(&state, &prev.id, &mode).await;

    let mut ms = state.music.lock().await;
    ms.stream_url = stream;
    Ok(Json(ms.clone()))
}

/// POST /api/music/mode - Cambiar modo de reproducción
pub async fn set_mode(
    State(state): State<AppState>,
    Json(req): Json<SetModeRequest>,
) -> Json<MusicState> {
    // Si hay algo reproduciéndose, reiniciar con el nuevo modo
    let mut ms = state.music.lock().await;
    let old_mode = ms.mode.clone();
    ms.mode = req.mode.clone();

    if let Some(ref track) = ms.current {
        if old_mode != req.mode {
            let track_id = track.id.clone();
            drop(ms);
            let stream = start_playback(&state, &track_id, &req.mode).await;
            let mut ms = state.music.lock().await;
            ms.stream_url = stream;
            return Json(ms.clone());
        }
    }

    Json(ms.clone())
}

/// POST /api/music/recommend - Mix basado en canción actual/historial
/// Usa múltiples seeds y limita tracks por artista para diversificar
pub async fn recommend(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    let ms = state.music.lock().await;

    // Recolectar múltiples seeds: actual + últimas del historial (distintos artistas)
    let mut seeds: Vec<String> = Vec::new();
    let mut seed_artists: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Some(ref c) = ms.current {
        seeds.push(c.id.clone());
        seed_artists.insert(c.artist.to_lowercase());
    }
    for h in ms.history.iter().rev() {
        let artist_lower = h.artist.to_lowercase();
        if !seed_artists.contains(&artist_lower) && seeds.len() < 4 {
            seeds.push(h.id.clone());
            seed_artists.insert(artist_lower);
        }
    }

    if seeds.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Reproduce algo primero".to_string()));
    }

    let mut existing: std::collections::HashSet<String> = ms.queue.iter().map(|t| t.id.clone()).collect();
    if let Some(ref c) = ms.current { existing.insert(c.id.clone()); }
    for h in &ms.history { existing.insert(h.id.clone()); }
    drop(ms);

    let username = {
        let sessions = state.sessions.lock().await;
        extract_username(&sessions, &headers)
    };

    // Buscar recomendaciones de cada seed en paralelo
    let mut handles = Vec::new();
    for seed_id in &seeds {
        let sid = seed_id.clone();
        handles.push(tokio::spawn(async move {
            let mix_url = format!("https://www.youtube.com/watch?v={}&list=RD{}", sid, sid);
            let output = Command::new("yt-dlp")
                .args(["--flat-playlist", "--dump-json", "--no-warnings", "--ignore-errors", &mix_url])
                .output()
                .await
                .ok()?;
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        }));
    }

    let mut all_candidates: Vec<MusicTrack> = Vec::new();
    for handle in handles {
        if let Ok(Some(stdout)) = handle.await {
            for line in stdout.lines() {
                if let Ok(entry) = serde_json::from_str::<YtDlpEntry>(line) {
                    if !entry.id.is_empty() && !existing.contains(&entry.id) {
                        let (thumb, artist) = extract_track_info(&entry);
                        all_candidates.push(MusicTrack {
                            id: entry.id,
                            title: entry.title,
                            artist,
                            thumbnail: thumb,
                            duration: entry.duration.unwrap_or(0.0) as u32,
                            added_by: Some(format!("Mix ({})", username)),
                        });
                    }
                }
            }
        }
    }

    if all_candidates.is_empty() {
        return Err((StatusCode::NOT_FOUND, "No se encontraron recomendaciones".to_string()));
    }

    // Deduplicar por ID
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    all_candidates.retain(|t| seen_ids.insert(t.id.clone()));

    // Limitar max 3 tracks por artista para diversificar
    let mut artist_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let recommended: Vec<MusicTrack> = all_candidates.into_iter()
        .filter(|t| {
            let key = t.artist.to_lowercase();
            let count = artist_count.entry(key).or_insert(0);
            *count += 1;
            *count <= 3
        })
        .take(20)
        .collect();

    if recommended.is_empty() {
        return Err((StatusCode::NOT_FOUND, "No se encontraron recomendaciones".to_string()));
    }

    let mut ms = state.music.lock().await;
    let added = recommended.len();
    ms.queue.extend(recommended);

    state.log_activity("musica", &format!("Mix: {} recomendaciones de {} seeds", added, seeds.len()), &username).await;
    Ok(Json(ms.clone()))
}
