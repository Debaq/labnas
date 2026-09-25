//! Comandos de musica

use super::*;

// =====================
// Music commands (Telegram)
// =====================

pub(super) async fn handle_music_status(state: &AppState) -> String {
    let ms = state.music.lock().await;
    if let Some(ref track) = ms.current {
        let status = if ms.paused { "pausado" } else { "reproduciendo" };
        let queue_info = if ms.queue.is_empty() {
            String::new()
        } else {
            format!("\n\n*Cola:* {} canciones", ms.queue.len())
        };
        format!(
            "*Musica* ({})\n\n*{}*\n{}\nVolumen: {}%{}",
            status, track.title, track.artist, ms.volume, queue_info
        )
    } else if !ms.queue.is_empty() {
        format!("No hay musica sonando.\n{} canciones en cola.\n\nUsa `/next` para iniciar.", ms.queue.len())
    } else {
        "No hay musica sonando.\n\nUsa `/play nombre` para buscar y reproducir.".to_string()
    }
}

pub(super) async fn handle_music_play(state: &AppState, username: &str, query: &str) -> String {
    if query.is_empty() {
        return "Uso: `/play nombre de cancion`".to_string();
    }

    let search_term = format!("ytsearch1:{}", query);
    let output = match tokio::process::Command::new("yt-dlp")
        .args(["--flat-playlist", "--dump-json", "--no-warnings", "--ignore-errors", &search_term])
        .output().await {
        Ok(o) => o,
        Err(_) => return "Error: yt-dlp no disponible.".to_string(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entry: Option<crate::handlers::music::MusicTrack> = stdout.lines().find_map(|line| {
        let e: serde_json::Value = serde_json::from_str(line).ok()?;
        let id = e["id"].as_str()?.to_string();
        if id.is_empty() { return None; }
        Some(crate::handlers::music::MusicTrack {
            id,
            title: e["title"].as_str().unwrap_or("?").to_string(),
            artist: e["uploader"].as_str().or(e["channel"].as_str()).unwrap_or("?").to_string(),
            thumbnail: e["thumbnail"].as_str().unwrap_or("").to_string(),
            duration: e["duration"].as_f64().unwrap_or(0.0) as u32,
            added_by: Some(username.to_string()),
        })
    });

    let Some(track) = entry else {
        return format!("No encontre nada para \"{}\"", query);
    };

    let mut ms = state.music.lock().await;
    if ms.current.is_some() {
        let msg = format!("Agregado a la cola: *{}* - {}", track.title, track.artist);
        ms.queue.push(track);
        return msg;
    }

    let title = track.title.clone();
    let artist = track.artist.clone();
    let track_id = track.id.clone();

    crate::handlers::music::add_to_history_pub(&mut ms, &track, username);
    ms.current = Some(track);
    ms.started_by = Some(username.to_string());
    ms.paused = false;
    drop(ms);

    crate::handlers::music::spawn_player_pub(state, &track_id).await;

    format!("Reproduciendo: *{}* - {}", title, artist)
}

pub(super) async fn handle_music_next(state: &AppState) -> String {
    crate::handlers::music::kill_player_pub(state).await;

    let mut ms = state.music.lock().await;
    if ms.queue.is_empty() {
        ms.current = None;
        ms.started_by = None;
        return "Cola vacia. Musica detenida.".to_string();
    }

    let next_track = ms.queue.remove(0);
    let next_id = next_track.id.clone();
    let next_by = next_track.added_by.clone().unwrap_or_default();
    let title = next_track.title.clone();
    let artist = next_track.artist.clone();
    let remaining = ms.queue.len();

    crate::handlers::music::add_to_history_pub(&mut ms, &next_track, &next_by);
    ms.current = Some(next_track);
    ms.started_by = Some(next_by);
    ms.paused = false;
    drop(ms);

    crate::handlers::music::spawn_player_pub(state, &next_id).await;

    format!("Siguiente: *{}* - {}\n{} en cola", title, artist, remaining)
}

pub(super) async fn handle_music_stop(state: &AppState) -> String {
    crate::handlers::music::kill_player_pub(state).await;
    let mut ms = state.music.lock().await;
    let history = ms.history.clone();
    let vol = ms.volume;
    *ms = crate::handlers::music::MusicState::default();
    ms.history = history;
    ms.volume = vol;
    "Musica detenida.".to_string()
}

pub(super) async fn handle_music_pause(state: &AppState) -> String {
    let mut ms = state.music.lock().await;
    if ms.current.is_none() {
        return "No hay musica sonando.".to_string();
    }
    ms.paused = !ms.paused;
    let paused = ms.paused;
    drop(ms);

    if paused {
        crate::handlers::music::pause_player_pub(state).await;
    } else {
        crate::handlers::music::resume_player_pub(state).await;
    }

    if paused { "Musica pausada.".to_string() } else { "Musica reanudada.".to_string() }
}

pub(super) async fn handle_music_mix(state: &AppState, username: &str) -> String {
    let ms = state.music.lock().await;
    let seed_id = ms.current.as_ref().map(|t| t.id.clone())
        .or_else(|| ms.history.last().map(|h| h.id.clone()));
    drop(ms);

    let Some(seed_id) = seed_id else {
        return "Reproduce algo primero para generar recomendaciones.".to_string();
    };

    let mix_url = format!("https://www.youtube.com/watch?v={}&list=RD{}", seed_id, seed_id);
    let output = match tokio::process::Command::new("yt-dlp")
        .args(["--flat-playlist", "--dump-json", "--no-warnings", "--ignore-errors", &mix_url])
        .output().await {
        Ok(o) => o,
        Err(_) => return "Error ejecutando yt-dlp.".to_string(),
    };

    let ms = state.music.lock().await;
    let mut existing: std::collections::HashSet<String> = ms.queue.iter().map(|t| t.id.clone()).collect();
    if let Some(ref c) = ms.current { existing.insert(c.id.clone()); }
    for h in &ms.history { existing.insert(h.id.clone()); }
    drop(ms);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut artist_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let recommended: Vec<crate::handlers::music::MusicTrack> = stdout.lines()
        .filter_map(|line| {
            let e: serde_json::Value = serde_json::from_str(line).ok()?;
            let id = e["id"].as_str()?.to_string();
            if id.is_empty() || existing.contains(&id) { return None; }
            let artist = e["uploader"].as_str().or(e["channel"].as_str()).unwrap_or("?").to_string();
            let key = artist.to_lowercase();
            let count = artist_count.entry(key).or_insert(0);
            *count += 1;
            if *count > 3 { return None; }
            Some(crate::handlers::music::MusicTrack {
                id,
                title: e["title"].as_str().unwrap_or("?").to_string(),
                artist,
                thumbnail: e["thumbnail"].as_str().unwrap_or("").to_string(),
                duration: e["duration"].as_f64().unwrap_or(0.0) as u32,
                added_by: Some(format!("Mix ({})", username)),
            })
        })
        .take(15)
        .collect();

    if recommended.is_empty() {
        return "No se encontraron recomendaciones.".to_string();
    }

    let count = recommended.len();
    let mut ms = state.music.lock().await;
    ms.queue.extend(recommended);

    format!("{} canciones agregadas a la cola.", count)
}

pub(super) async fn handle_music_volume(state: &AppState, vol_str: &str) -> String {
    let vol: u8 = match vol_str.parse() {
        Ok(v) if v <= 100 => v,
        _ => return "Uso: `/vol 0-100`".to_string(),
    };

    let mut ms = state.music.lock().await;
    ms.volume = vol;
    drop(ms);

    let vol_str = format!("{}%", vol);
    let amixer = tokio::process::Command::new("amixer")
        .args(["sset", "Master", &vol_str])
        .output().await;
    if amixer.is_err() || !amixer.as_ref().unwrap().status.success() {
        let _ = tokio::process::Command::new("pactl")
            .env("XDG_RUNTIME_DIR", "/run/user/1000")
            .args(["set-sink-volume", "@DEFAULT_SINK@", &vol_str])
            .output().await;
    }

    format!("Volumen: {}%", vol)
}
