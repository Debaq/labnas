use axum::{
    extract::{Path, Query, State},
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
    #[serde(default = "default_volume")]
    pub volume: u8,
    #[serde(default)]
    pub repeat: RepeatMode,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub video: bool,
    #[serde(default)]
    pub video_screen: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

fn default_volume() -> u8 { 80 }

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

#[derive(Debug, Deserialize)]
pub struct SetVolumeRequest {
    pub volume: u8,
}

#[derive(Debug, Deserialize)]
pub struct QueueMoveRequest {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Deserialize)]
pub struct SetVideoRequest {
    pub video: bool,
    #[serde(default)]
    pub screen: Option<u8>,
}

#[derive(Debug, Serialize)]
pub struct ScreenInfo {
    pub index: u8,
    pub connector: String,
    pub name: String,
    pub connected: bool,
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

/// Detecta el usuario que tiene la sesión X en :0
fn detect_x_user() -> Option<String> {
    // Parsear `who` para encontrar el usuario con :0
    let output = std::process::Command::new("who")
        .output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("(:0)") || line.contains(":0") {
            return line.split_whitespace().next().map(|s| s.to_string());
        }
    }
    None
}

/// Asegura que root tenga acceso al display X del usuario
pub async fn ensure_x_access() {
    if let Some(user) = detect_x_user() {
        // Ejecutar xhost +local:root como el usuario dueño de X
        let _ = Command::new("su")
            .args(["-", &user, "-c", "DISPLAY=:0 xhost +local:root"])
            .output().await;
    }
}

/// Lanza mpv en el NAS para reproducir audio/video
async fn spawn_player(state: &AppState, video_id: &str) {
    kill_player(state).await;

    let ms = state.music.lock().await;
    let video = ms.video;
    let screen = ms.video_screen;
    drop(ms);

    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let mut args: Vec<String> = vec!["--really-quiet".to_string()];

    if video {
        // Asegurar acceso X antes de abrir ventana
        ensure_x_access().await;

        if let Some(scr) = screen {
            args.push(format!("--screen={}", scr));
            args.push(format!("--fs-screen={}", scr));
        }
        args.push("--fs".to_string());
    } else {
        args.push("--no-video".to_string());
    }

    args.push(url);

    let mut cmd = Command::new("mpv");
    cmd.env("DISPLAY", ":0");
    // XDG_RUNTIME_DIR necesario para PulseAudio/PipeWire audio
    if let Some(user) = detect_x_user() {
        // Obtener UID del usuario para XDG_RUNTIME_DIR
        if let Ok(output) = std::process::Command::new("id").args(["-u", &user]).output() {
            let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
            cmd.env("XDG_RUNTIME_DIR", format!("/run/user/{}", uid));
        }
        cmd.env("XAUTHORITY", format!("/home/{}/.Xauthority", user));
    }
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let child = cmd
        .args(&arg_refs)
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

// Wrappers publicos para uso desde notifications
pub fn add_to_history_pub(ms: &mut MusicState, track: &MusicTrack, played_by: &str) {
    add_to_history(ms, track, played_by);
}
pub async fn start_playback_pub(state: &AppState, video_id: &str, mode: &PlaybackMode) -> Option<String> {
    start_playback(state, video_id, mode).await
}
pub async fn kill_player_pub(state: &AppState) {
    kill_player(state).await;
}
pub async fn pause_player_pub(state: &AppState) {
    pause_player(state).await;
}
pub async fn resume_player_pub(state: &AppState) {
    resume_player(state).await;
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

/// POST /api/music/next - Siguiente canción (respeta repeat mode)
pub async fn next(
    State(state): State<AppState>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    kill_player(&state).await;
    advance_queue(&state).await;
    Ok(Json(state.music.lock().await.clone()))
}

/// Lógica compartida de avanzar la cola (usada por next, auto-next, etc.)
async fn advance_queue(state: &AppState) {
    let mut ms = state.music.lock().await;

    // Repeat One: volver a reproducir la misma
    if ms.repeat == RepeatMode::One {
        if let Some(ref track) = ms.current {
            let track_id = track.id.clone();
            let mode = ms.mode.clone();
            ms.paused = false;
            drop(ms);
            let stream = start_playback(state, &track_id, &mode).await;
            let mut ms = state.music.lock().await;
            ms.stream_url = stream;
            return;
        }
    }

    // Repeat All: mover la actual al final de la cola antes de avanzar
    if ms.repeat == RepeatMode::All {
        if let Some(current) = ms.current.clone() {
            ms.queue.push(current);
        }
    }

    if ms.queue.is_empty() {
        ms.current = None;
        ms.started_by = None;
        ms.stream_url = None;
        ms.paused = false;
        return;
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

    let stream = start_playback(state, &next_id, &mode).await;
    let mut ms = state.music.lock().await;
    ms.stream_url = stream;
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
        advance_queue(&state).await;
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

/// POST /api/music/volume - Ajustar volumen (0-100)
pub async fn set_volume(
    State(state): State<AppState>,
    Json(req): Json<SetVolumeRequest>,
) -> Json<MusicState> {
    let vol = req.volume.min(100);
    let mut ms = state.music.lock().await;
    ms.volume = vol;
    let mode = ms.mode.clone();
    drop(ms);

    // En modo NAS, ajustar volumen del sistema
    if mode == PlaybackMode::Nas {
        let vol_str = format!("{}%", vol);
        // amixer funciona sin sesion de usuario (a diferencia de pactl)
        let amixer = Command::new("amixer")
            .args(["sset", "Master", &vol_str])
            .output().await;
        // Si amixer falla, intentar pactl con PULSE_RUNTIME_PATH
        if amixer.is_err() || !amixer.as_ref().unwrap().status.success() {
            let _ = Command::new("pactl")
                .env("XDG_RUNTIME_DIR", "/run/user/1000")
                .args(["set-sink-volume", "@DEFAULT_SINK@", &vol_str])
                .output().await;
        }
    }

    Json(state.music.lock().await.clone())
}

/// POST /api/music/queue/play/{index} - Reproducir un item de la cola directamente
pub async fn queue_play(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    kill_player(&state).await;

    let mut ms = state.music.lock().await;
    if index >= ms.queue.len() {
        return Err((StatusCode::BAD_REQUEST, "Indice fuera de rango".to_string()));
    }

    let track = ms.queue.remove(index);
    let track_id = track.id.clone();
    let played_by = track.added_by.clone().unwrap_or_default();
    let mode = ms.mode.clone();

    // Devolver la canción actual a la cola al frente (si existe)
    if let Some(current) = ms.current.take() {
        ms.queue.insert(0, current);
    }

    add_to_history(&mut ms, &track, &played_by);
    ms.current = Some(track);
    ms.started_by = Some(played_by);
    ms.paused = false;
    drop(ms);

    let stream = start_playback(&state, &track_id, &mode).await;
    let mut ms = state.music.lock().await;
    ms.stream_url = stream;
    Ok(Json(ms.clone()))
}

/// POST /api/music/queue/move - Mover un item de la cola
pub async fn queue_move(
    State(state): State<AppState>,
    Json(req): Json<QueueMoveRequest>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    let mut ms = state.music.lock().await;
    if req.from >= ms.queue.len() || req.to >= ms.queue.len() {
        return Err((StatusCode::BAD_REQUEST, "Indice fuera de rango".to_string()));
    }
    let item = ms.queue.remove(req.from);
    ms.queue.insert(req.to, item);
    Ok(Json(ms.clone()))
}

/// POST /api/music/shuffle - Activar/desactivar aleatorio
pub async fn toggle_shuffle(
    State(state): State<AppState>,
) -> Json<MusicState> {
    let mut ms = state.music.lock().await;
    ms.shuffle = !ms.shuffle;

    if ms.shuffle && ms.queue.len() > 1 {
        // Mezclar la cola
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut indices: Vec<usize> = (0..ms.queue.len()).collect();
        // Fisher-Yates con seed del timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut hasher = DefaultHasher::new();
        now.hash(&mut hasher);
        let mut seed = hasher.finish();
        for i in (1..indices.len()).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let j = (seed as usize) % (i + 1);
            indices.swap(i, j);
        }
        let shuffled: Vec<MusicTrack> = indices.into_iter().map(|i| ms.queue[i].clone()).collect();
        ms.queue = shuffled;
    }

    Json(ms.clone())
}

/// POST /api/music/repeat - Ciclar modo de repetición (off -> all -> one -> off)
pub async fn toggle_repeat(
    State(state): State<AppState>,
) -> Json<MusicState> {
    let mut ms = state.music.lock().await;
    ms.repeat = match ms.repeat {
        RepeatMode::Off => RepeatMode::All,
        RepeatMode::All => RepeatMode::One,
        RepeatMode::One => RepeatMode::Off,
    };
    Json(ms.clone())
}

/// POST /api/music/video - Activar/desactivar video + pantalla
pub async fn set_video(
    State(state): State<AppState>,
    Json(req): Json<SetVideoRequest>,
) -> Json<MusicState> {
    let mut ms = state.music.lock().await;
    ms.video = req.video;
    if let Some(scr) = req.screen {
        ms.video_screen = Some(scr);
    }

    // Si hay algo reproduciéndose, reiniciar mpv con la nueva config
    if let Some(ref track) = ms.current {
        if ms.mode == PlaybackMode::Nas && !ms.paused {
            let track_id = track.id.clone();
            drop(ms);
            kill_player(&state).await;
            spawn_player(&state, &track_id).await;
            return Json(state.music.lock().await.clone());
        }
    }

    Json(ms.clone())
}

/// Extrae el nombre del monitor desde el EDID binario
fn parse_edid_name(edid: &[u8]) -> Option<String> {
    // EDID tiene 128 bytes minimo, descriptores empiezan en byte 54
    if edid.len() < 128 { return None; }
    // 4 descriptores de 18 bytes cada uno (bytes 54-125)
    for i in 0..4 {
        let offset = 54 + i * 18;
        // Monitor Name descriptor: bytes 0-2 = 0x00, byte 3 = 0xFC
        if edid[offset] == 0 && edid[offset + 1] == 0 && edid[offset + 2] == 0 && edid[offset + 3] == 0xFC {
            // El nombre esta en bytes 5-17 (13 chars), terminado por 0x0A
            let name_bytes = &edid[offset + 5..offset + 18];
            let name: String = name_bytes.iter()
                .take_while(|&&b| b != 0x0A && b != 0x00)
                .map(|&b| b as char)
                .collect();
            let name = name.trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// GET /api/music/screens - Listar pantallas conectadas con nombre del monitor
pub async fn list_screens() -> Json<Vec<ScreenInfo>> {
    let mut screens = Vec::new();
    let mut index: u8 = 0;

    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Json(screens);
    };

    let mut connectors: Vec<(String, String, bool)> = Vec::new();
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if !dir_name.contains('-') { continue; }

        let status_path = entry.path().join("status");
        let connected = std::fs::read_to_string(&status_path)
            .map(|s| s.trim() == "connected")
            .unwrap_or(false);

        if !connected { continue; }

        let connector = dir_name.splitn(2, '-').nth(1).unwrap_or(&dir_name).to_string();

        // Leer EDID para obtener nombre del monitor
        let edid_path = entry.path().join("edid");
        let monitor_name = std::fs::read(&edid_path)
            .ok()
            .and_then(|edid| parse_edid_name(&edid))
            .unwrap_or_else(|| connector.clone());

        connectors.push((connector, monitor_name, connected));
    }

    connectors.sort_by(|a, b| a.0.cmp(&b.0));

    for (connector, name, connected) in connectors {
        screens.push(ScreenInfo { index, connector, name, connected });
        index += 1;
    }

    Json(screens)
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
