//! Control del proceso mpv y monitor de fin de pista

use super::*;

/// Mata el proceso mpv actual si existe
pub(super) async fn kill_player(state: &AppState) {
    let mut proc = state.music_process.lock().await;
    if let Some(ref mut child) = *proc {
        let _ = child.kill().await;
    }
    *proc = None;
}

/// Pausa el proceso mpv (SIGSTOP)
pub(super) async fn pause_player(state: &AppState) {
    let proc = state.music_process.lock().await;
    if let Some(ref child) = *proc {
        if let Some(pid) = child.id() {
            unsafe { libc::kill(pid as i32, libc::SIGSTOP); }
        }
    }
}

/// Resume el proceso mpv (SIGCONT)
pub(super) async fn resume_player(state: &AppState) {
    let proc = state.music_process.lock().await;
    if let Some(ref child) = *proc {
        if let Some(pid) = child.id() {
            unsafe { libc::kill(pid as i32, libc::SIGCONT); }
        }
    }
}

/// Detecta el usuario que tiene la sesión X en :0
pub(super) fn detect_x_user() -> Option<String> {
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
    // Solo hace falta si root abre ventanas en el display de otro usuario
    if !crate::config::is_root() {
        return;
    }
    if let Some(user) = detect_x_user() {
        // Ejecutar xhost +local:root como el usuario dueño de X
        let _ = Command::new("su")
            .args(["-", &user, "-c", "DISPLAY=:0 xhost +local:root"])
            .output().await;
    }
}

/// Lanza mpv en el NAS para reproducir audio/video
pub(super) async fn spawn_player(state: &AppState, video_id: &str) {
    kill_player(state).await;

    let ms = state.music.lock().await;
    let video = ms.video;
    let screen = ms.video_screen;
    let volume = ms.volume;
    drop(ms);

    // Limpiar socket anterior
    let _ = std::fs::remove_file(MPV_SOCKET);

    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let mut args: Vec<String> = vec![
        "--really-quiet".to_string(),
        format!("--volume={}", volume),
        format!("--input-ipc-server={}", MPV_SOCKET),
    ];

    // Args extra de mpv desde la DB
    {
        if let Ok(conn) = crate::db::get_conn(&state.db) {
            if let Some(args_json) = crate::db::get_setting(&conn, "mpv_extra_args") {
                if let Ok(extra_args) = serde_json::from_str::<Vec<String>>(&args_json) {
                    for arg in &extra_args {
                        let trimmed = arg.trim();
                        if !trimmed.is_empty() {
                            args.push(trimmed.to_string());
                        }
                    }
                }
            }
        }
    }

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

// Wrappers publicos para uso desde notifications
pub fn add_to_history_pub(ms: &mut MusicState, track: &MusicTrack, played_by: &str) {
    add_to_history(ms, track, played_by);
}
pub async fn spawn_player_pub(state: &AppState, video_id: &str) {
    spawn_player(state, video_id).await;
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

pub(super) fn add_to_history(ms: &mut MusicState, track: &MusicTrack, played_by: &str) {
    ms.history.push(HistoryEntry {
        id: track.id.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        thumbnail: track.thumbnail.clone(),
        played_by: played_by.to_string(),
        duration: track.duration,
    });
    if ms.history.len() > MAX_HISTORY {
        ms.history.remove(0);
    }
}

pub(super) async fn fetch_track_info(id: &str) -> Result<MusicTrack, (StatusCode, String)> {
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


/// Background loop que monitorea si mpv terminó y avanza la cola automáticamente
pub async fn music_monitor_loop(state: AppState) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        let (has_current, is_paused, started_at) = {
            let ms = state.music.lock().await;
            (ms.current.is_some(), ms.paused, ms.playback_started_at)
        };

        if !has_current || is_paused {
            continue;
        }

        let mut player_finished = false;
        {
            let mut proc = state.music_process.lock().await;
            if let Some(ref mut child) = *proc {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    *proc = None;
                    player_finished = true;
                }
            } else {
                // No hay proceso pero si hay current: solo tratar como terminado
                // si pasaron mas de 10s desde el inicio (evita race con spawn_player)
                if let Some(started) = started_at {
                    if now_epoch_secs() - started > 10 {
                        player_finished = true;
                    }
                }
            }
        }

        if player_finished {
            advance_queue(&state).await;
        }
    }
}
