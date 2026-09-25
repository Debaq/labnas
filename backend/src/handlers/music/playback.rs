//! Reproduccion, cola, estado y ajustes del reproductor

use super::*;

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

    let has_current = state.music.lock().await.current.is_some();

    // Obtener info del track SIN mantener el lock (yt-dlp puede tardar segundos)
    let mut track = fetch_track_info(&req.id).await?;
    track.added_by = Some(username.clone());

    if has_current && !req.force {
        // Ya hay algo → agregar a la cola
        let mut ms = state.music.lock().await;
        ms.queue.push(track);
        return Ok(Json(ms.clone()));
    }

    if has_current && req.force {
        // Force: detener actual, poner la actual al inicio de la cola, reproducir nueva
        kill_player(&state).await;
        let mut ms = state.music.lock().await;
        if let Some(current) = ms.current.take() {
            ms.queue.insert(0, current);
        }
        add_to_history(&mut ms, &track, &username);
        ms.current = Some(track.clone());
        ms.playback_started_at = Some(now_epoch_secs());
        ms.elapsed = 0;
        ms.started_by = Some(username.clone());
        ms.paused = false;
        drop(ms);
        spawn_player(&state, &req.id).await;
        state.log_activity("musica", &format!("Reproduciendo (forzado): {}", track.title), &username).await;
        return Ok(Json(state.music.lock().await.clone()));
    }

    // Nada reproduciéndose → reproducir ahora
    let mut ms = state.music.lock().await;

    add_to_history(&mut ms, &track, &username);
    ms.current = Some(track.clone());
    ms.playback_started_at = Some(now_epoch_secs());
    ms.elapsed = 0;
    ms.started_by = Some(username.clone());
    ms.paused = false;
    drop(ms);

    spawn_player(&state, &req.id).await;

    state.log_activity("musica", &format!("Reproduciendo: {}", track.title), &username).await;

    Ok(Json(state.music.lock().await.clone()))
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
pub(super) async fn advance_queue(state: &AppState) {
    let mut ms = state.music.lock().await;

    // Repeat One: volver a reproducir la misma
    if ms.repeat == RepeatMode::One {
        if let Some(ref track) = ms.current {
            let track_id = track.id.clone();
            ms.paused = false;
            ms.playback_started_at = Some(now_epoch_secs());
            ms.elapsed = 0;
            drop(ms);
            spawn_player(state, &track_id).await;
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
        ms.paused = false;
        return;
    }

    let next_track = ms.queue.remove(0);
    let next_id = next_track.id.clone();
    let next_by = next_track.added_by.clone().unwrap_or_default();

    add_to_history(&mut ms, &next_track, &next_by);
    ms.current = Some(next_track);
    ms.playback_started_at = Some(now_epoch_secs());
    ms.elapsed = 0;
    ms.started_by = Some(next_by);
    ms.paused = false;
    drop(ms);

    spawn_player(state, &next_id).await;
}

/// POST /api/music/queue/clear - Vaciar la cola completa
pub async fn queue_clear(
    State(state): State<AppState>,
) -> Json<MusicState> {
    let mut ms = state.music.lock().await;
    ms.queue.clear();
    Json(ms.clone())
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
    Json(snapshot(&state).await)
}

/// Estado actual con `elapsed` calculado (avanza la cola si mpv termino)
pub(super) async fn snapshot(state: &AppState) -> MusicState {
    // Verificar si mpv terminó (solo si hay proceso activo, no si falta el proceso)
    // El monitor loop se encarga del caso sin proceso para evitar race conditions con spawn_player
    let mut player_finished = false;
    {
        let mut proc = state.music_process.lock().await;
        if let Some(ref mut child) = *proc {
            if matches!(child.try_wait(), Ok(Some(_))) {
                *proc = None;
                player_finished = true;
            }
        }
    }

    if player_finished {
        advance_queue(state).await;
    }

    let mut ms = state.music.lock().await;
    // Calcular elapsed dinamicamente
    if ms.current.is_some() && !ms.paused {
        if let Some(started) = ms.playback_started_at {
            ms.elapsed = (now_epoch_secs() - started) as u32;
        }
    }
    ms.clone()
}

/// Publica "music.state" cuando cambia algo (no por el avance de `elapsed`, que el
/// frontend cuenta solo). Solo trabaja si hay alguien conectado al bus.
pub async fn music_events_loop(state: AppState) {
    let mut last = String::new();
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if !state.events.has_interest("music.state") {
            last.clear();
            continue;
        }
        let ms = snapshot(&state).await;
        let mut key = ms.clone();
        key.elapsed = 0;
        let key = serde_json::to_string(&key).unwrap_or_default();
        if key != last {
            last = key;
            state.events.publish("music.state", &ms, crate::events::Audience::All, Some("music"));
        }
    }
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
    let volume = ms.volume;
    let repeat = ms.repeat.clone();
    let shuffle = ms.shuffle;
    let video = ms.video;
    let video_screen = ms.video_screen;
    *ms = MusicState::default();
    ms.history = history;
    ms.volume = volume;
    ms.repeat = repeat;
    ms.shuffle = shuffle;
    ms.video = video;
    ms.video_screen = video_screen;
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

    if paused {
        // Guardar elapsed actual antes de pausar
        if let Some(started) = ms.playback_started_at {
            ms.elapsed = (now_epoch_secs() - started) as u32;
        }
    } else {
        // Al despausar, ajustar playback_started_at para que elapsed siga correcto
        ms.playback_started_at = Some(now_epoch_secs() - ms.elapsed as u64);
    }
    drop(ms);

    if paused {
        pause_player(&state).await;
    } else {
        resume_player(&state).await;
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
        duration: prev.duration,
        added_by: Some(prev.played_by.clone()),
    };

    add_to_history(&mut ms, &track, &prev.played_by);
    ms.current = Some(track);
    ms.playback_started_at = Some(now_epoch_secs());
    ms.elapsed = 0;
    ms.started_by = Some(prev.played_by);
    ms.paused = false;
    drop(ms);

    spawn_player(&state, &prev.id).await;

    Ok(Json(state.music.lock().await.clone()))
}

/// POST /api/music/volume - Ajustar volumen (0-100)
pub async fn set_volume(
    State(state): State<AppState>,
    Json(req): Json<SetVolumeRequest>,
) -> Json<MusicState> {
    let vol = req.volume.min(100);
    let mut ms = state.music.lock().await;
    ms.volume = vol;
    drop(ms);

    // Ajustar volumen de mpv via IPC socket
    if let Ok(mut sock) = tokio::net::UnixStream::connect(MPV_SOCKET).await {
        let cmd = format!("{{ \"command\": [\"set_property\", \"volume\", {}] }}\n", vol);
        let _ = sock.write_all(cmd.as_bytes()).await;
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

    // Devolver la canción actual a la cola al frente (si existe)
    if let Some(current) = ms.current.take() {
        ms.queue.insert(0, current);
    }

    add_to_history(&mut ms, &track, &played_by);
    ms.current = Some(track);
    ms.playback_started_at = Some(now_epoch_secs());
    ms.elapsed = 0;
    ms.started_by = Some(played_by);
    ms.paused = false;
    drop(ms);

    spawn_player(&state, &track_id).await;
    Ok(Json(state.music.lock().await.clone()))
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

/// GET /api/music/mpv-args - Obtener args extra de mpv
pub async fn get_mpv_args(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let args = crate::db::db_op(&state.db, |conn| {
        let json = crate::db::get_setting(conn, "mpv_extra_args").unwrap_or_else(|| "[]".to_string());
        let parsed: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
        Ok(parsed)
    }).await?;
    Ok(Json(args))
}

/// POST /api/music/mpv-args - Guardar args extra de mpv
pub async fn set_mpv_args(
    State(state): State<AppState>,
    Json(args): Json<Vec<String>>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let args_clone = args.clone();
    crate::db::db_op(&state.db, move |conn| {
        let json = serde_json::to_string(&args_clone).unwrap_or_else(|_| "[]".to_string());
        crate::db::set_setting(conn, "mpv_extra_args", &json)
    }).await?;
    Ok(Json(args))
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
        if !ms.paused {
            let track_id = track.id.clone();
            drop(ms);
            // spawn_player ya hace kill_player internamente
            spawn_player(&state, &track_id).await;
            return Json(state.music.lock().await.clone());
        }
    }

    Json(ms.clone())
}
