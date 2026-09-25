//! Recomendaciones, radio, voy a tener suerte y Last.fm

use super::*;

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

    // Recoger titulos/artistas de las seeds para fallback de busqueda
    let seed_info: Vec<(String, String)> = {
        let ms = state.music.lock().await;
        let mut info = Vec::new();
        if let Some(ref c) = ms.current {
            info.push((c.artist.clone(), c.title.clone()));
        }
        for h in ms.history.iter().rev() {
            if info.len() < 4 {
                let key = h.artist.to_lowercase();
                if !info.iter().any(|(a, _)| a.to_lowercase() == key) {
                    info.push((h.artist.clone(), h.title.clone()));
                }
            }
        }
        info
    };

    // Buscar recomendaciones de cada seed en paralelo (YouTube Mix playlists)
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

    // Fallback: si YouTube Mix no devolvio nada, buscar por artista
    if all_candidates.is_empty() {
        let mut fb_handles = Vec::new();
        for (artist, _title) in &seed_info {
            let a = artist.clone();
            let user = username.clone();
            let existing_clone = existing.clone();
            fb_handles.push(tokio::spawn(async move {
                let search = format!("ytsearch8:{} mix", a);
                let output = Command::new("yt-dlp")
                    .args(["--flat-playlist", "--dump-json", "--no-warnings", "--ignore-errors", &search])
                    .output().await.ok()?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut tracks = Vec::new();
                for line in stdout.lines() {
                    if let Ok(entry) = serde_json::from_str::<YtDlpEntry>(line) {
                        if !entry.id.is_empty() && !existing_clone.contains(&entry.id) {
                            let (thumb, artist) = extract_track_info(&entry);
                            tracks.push(MusicTrack {
                                id: entry.id, title: entry.title, artist, thumbnail: thumb,
                                duration: entry.duration.unwrap_or(0.0) as u32,
                                added_by: Some(format!("Mix ({})", user)),
                            });
                        }
                    }
                }
                Some(tracks)
            }));
        }
        for handle in fb_handles {
            if let Ok(Some(tracks)) = handle.await {
                all_candidates.extend(tracks);
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

/// Limpiar titulo de YouTube para Last.fm
/// Quita "(Official Video)", "(HD)", "(Lyrics)", "[MV]", etc.
pub(super) fn clean_track_title(title: &str) -> String {
    let mut clean = title.to_string();
    // Quitar contenido entre parentesis y corchetes con palabras clave
    let patterns = [
        r"\((?i)official\s*(music\s*)?video\)",
        r"\((?i)oficial\s*(hd\s*)?(video|music)?\)",
        r"\((?i)video\s*(oficial|clip|musical)\)",
        r"\((?i)lyric(s)?\s*(video)?\)",
        r"\((?i)audio\s*(oficial|official)?\)",
        r"\((?i)hd\s*(video)?\)",
        r"\((?i)live\)",
        r"\((?i)remastered\s*\d*\)",
        r"\((?i)version\s*\w*\)",
        r"\[(?i)official\s*(music\s*)?video\]",
        r"\[(?i)mv\]",
        r"\[(?i)hd\]",
        r"\[(?i)lyrics?\]",
        r"(?i)\|\s*official\s*video",
        r"(?i)official\s*(music\s*)?video",
        r"(?i)video\s*oficial",
        r"(?i)\bft\.?\s",
        r"(?i)\bfeat\.?\s",
    ];
    for pat in &patterns {
        if let Ok(re) = regex_lite::Regex::new(pat) {
            clean = re.replace_all(&clean, "").to_string();
        }
    }
    // Limpiar espacios extra y guiones sueltos
    clean = clean.trim().trim_end_matches('-').trim().to_string();
    // Quitar dobles espacios
    while clean.contains("  ") {
        clean = clean.replace("  ", " ");
    }
    clean
}

/// Separa el nombre del artista del titulo si esta en formato "Artista - Cancion"
/// YouTube suele poner titulos como "Queen - Bohemian Rhapsody (Official Video)"
pub(super) fn strip_artist_from_title(title: &str, artist: &str) -> String {
    let title_lower = title.to_lowercase();
    let artist_lower = artist.to_lowercase().trim().to_string();

    for sep in [" - ", " – ", " — ", " | "] {
        if let Some(pos) = title_lower.find(sep) {
            let before = title_lower[..pos].trim();
            // Si la parte antes del separador coincide con el artista (o es substring significativo)
            if before == artist_lower
                || artist_lower.starts_with(before)
                || before.starts_with(&artist_lower)
            {
                let after = title[pos + sep.len()..].trim();
                if !after.is_empty() {
                    return after.to_string();
                }
            }
        }
    }

    title.to_string()
}

/// POST /api/music/lastfm-key - Guardar API key de Last.fm (admin only)
pub async fn set_lastfm_key(
    State(state): State<AppState>,
    Json(req): Json<SetLastfmKeyRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    if req.key.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "API key vacia".to_string()));
    }

    let key = req.key.trim().to_string();
    crate::db::db_op(&state.db, move |conn| {
        crate::db::set_secret_setting(conn, "lastfm_api_key", &key)
    }).await?;

    Ok((StatusCode::OK, "API key de Last.fm guardada".to_string()))
}

/// POST /api/music/radio - Radio basada en Last.fm (canciones similares)
pub async fn radio(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RadioRequest>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    use std::time::Duration;

    // 1. Obtener API key
    let api_key = crate::db::db_op(&state.db, |conn| {
        crate::db::get_secret_setting(conn, "lastfm_api_key")
            .ok_or("API key de Last.fm no configurada. Configurala en Ajustes.".to_string())
    }).await.map_err(|(_status, msg)| (StatusCode::BAD_REQUEST, msg))?;

    // 2. Limpiar titulo y separar artista del titulo (YouTube pone "Artista - Cancion")
    let clean_artist = clean_track_title(&req.artist);
    let clean_track = strip_artist_from_title(&clean_track_title(&req.track), &clean_artist);

    // Intentar primero con track.getSimilar, si falla probar con artist.getSimilar
    let url = format!(
        "https://ws.audioscrobbler.com/2.0/?method=track.getSimilar&artist={}&track={}&api_key={}&format=json&limit=20",
        urlencoding::encode(&clean_artist),
        urlencoding::encode(&clean_track),
        api_key,
    );

    let resp = state
        .http_client
        .get(&url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error Last.fm: {}", e)))?;

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error JSON Last.fm: {}", e)))?;

    // Si track.getSimilar no devuelve resultados, intentar con artist.getTopTracks como fallback
    let similar = match json["similartracks"]["track"].as_array() {
        Some(arr) if !arr.is_empty() => arr.clone(),
        _ => {
            let fallback_url = format!(
                "https://ws.audioscrobbler.com/2.0/?method=artist.getTopTracks&artist={}&api_key={}&format=json&limit=20",
                urlencoding::encode(&clean_artist),
                api_key,
            );
            let fb_resp = state.http_client.get(&fallback_url)
                .timeout(Duration::from_secs(10))
                .send().await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error Last.fm fallback: {}", e)))?;
            let fb_json: serde_json::Value = fb_resp.json().await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error JSON Last.fm: {}", e)))?;
            fb_json["toptracks"]["track"].as_array()
                .ok_or((StatusCode::NOT_FOUND, format!("Last.fm no encontro resultados para '{}' - '{}'", clean_artist, clean_track)))?
                .clone()
        }
    };

    if similar.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            "Last.fm no devolvio canciones similares".to_string(),
        ));
    }

    // 3. Buscar cada cancion en YouTube en paralelo
    let username = {
        let sessions = state.sessions.lock().await;
        extract_username(&sessions, &headers)
    };

    let mut handles = Vec::new();
    for track in similar {
        let artist = track["artist"]["name"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let name = track["name"].as_str().unwrap_or("").to_string();
        if artist.is_empty() || name.is_empty() {
            continue;
        }

        let user = username.clone();
        handles.push(tokio::spawn(async move {
            let search_term = format!("ytsearch1:{} {}", artist, name);
            let output = Command::new("yt-dlp")
                .args([
                    "--flat-playlist",
                    "--dump-json",
                    "--no-warnings",
                    "--ignore-errors",
                    &search_term,
                ])
                .output()
                .await
                .ok()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Ok(entry) = serde_json::from_str::<YtDlpEntry>(line) {
                    if !entry.id.is_empty() {
                        let (thumb, found_artist) = extract_track_info(&entry);
                        return Some(MusicTrack {
                            id: entry.id,
                            title: entry.title,
                            artist: found_artist,
                            thumbnail: thumb,
                            duration: entry.duration.unwrap_or(0.0) as u32,
                            added_by: Some(format!("Radio ({})", user)),
                        });
                    }
                }
            }
            None
        }));
    }

    let mut tracks: Vec<MusicTrack> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for handle in handles {
        if let Ok(Some(track)) = handle.await {
            if seen_ids.insert(track.id.clone()) {
                tracks.push(track);
            }
        }
    }

    if tracks.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            "Ninguna cancion similar se encontro en YouTube".to_string(),
        ));
    }

    // 4. Reemplazar cola
    let mut ms = state.music.lock().await;
    let count = tracks.len();
    ms.queue = tracks;

    state
        .log_activity(
            "musica",
            &format!(
                "Radio: {} canciones similares a {} - {}",
                count, req.artist, req.track
            ),
            &username,
        )
        .await;
    Ok(Json(ms.clone()))
}

/// POST /api/music/lucky - Voy a tener suerte: reproduce una cancion similar al azar
pub async fn lucky(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<LuckyRequest>,
) -> Result<Json<MusicState>, (StatusCode, String)> {
    use std::time::Duration;

    let api_key = crate::db::db_op(&state.db, |conn| {
        crate::db::get_secret_setting(conn, "lastfm_api_key")
            .ok_or("API key de Last.fm no configurada. Configurala en Ajustes.".to_string())
    }).await.map_err(|(_status, msg)| (StatusCode::BAD_REQUEST, msg))?;

    // Limpiar titulo, separar artista, y pedir muchas similares para tener variedad
    let clean_artist = clean_track_title(&req.artist);
    let clean_track = strip_artist_from_title(&clean_track_title(&req.track), &clean_artist);
    let url = format!(
        "https://ws.audioscrobbler.com/2.0/?method=track.getSimilar&artist={}&track={}&api_key={}&format=json&limit=50",
        urlencoding::encode(&clean_artist),
        urlencoding::encode(&clean_track),
        api_key,
    );

    let resp = state
        .http_client
        .get(&url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error Last.fm: {}", e)))?;

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error JSON Last.fm: {}", e)))?;

    // Fallback a artist.getTopTracks si track.getSimilar no devuelve nada
    let similar = match json["similartracks"]["track"].as_array() {
        Some(arr) if !arr.is_empty() => arr.clone(),
        _ => {
            let fallback_url = format!(
                "https://ws.audioscrobbler.com/2.0/?method=artist.getTopTracks&artist={}&api_key={}&format=json&limit=50",
                urlencoding::encode(&clean_artist),
                api_key,
            );
            let fb_resp = state.http_client.get(&fallback_url)
                .timeout(Duration::from_secs(10))
                .send().await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error Last.fm: {}", e)))?;
            let fb_json: serde_json::Value = fb_resp.json().await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error JSON: {}", e)))?;
            fb_json["toptracks"]["track"].as_array()
                .ok_or((StatusCode::NOT_FOUND, "Sin canciones similares".to_string()))?
                .clone()
        }
    };

    if similar.is_empty() {
        return Err((StatusCode::NOT_FOUND, "Sin canciones similares".to_string()));
    }

    let username = {
        let sessions = state.sessions.lock().await;
        extract_username(&sessions, &headers)
    };

    // Elegir candidatos en orden aleatorio usando timestamp como seed
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as usize;
    let len = similar.len();
    // Generar indices pseudo-aleatorios
    let mut indices: Vec<usize> = (0..len).collect();
    for i in (1..len).rev() {
        let j = (seed.wrapping_mul(i).wrapping_add(7)) % (i + 1);
        indices.swap(i, j);
    }

    for &idx in indices.iter().take(10) {
        let candidate = &similar[idx];
        let artist = candidate["artist"]["name"].as_str().unwrap_or("");
        let name = candidate["name"].as_str().unwrap_or("");
        if artist.is_empty() || name.is_empty() {
            continue;
        }

        let search_term = format!("ytsearch1:{} {}", artist, name);
        let output = Command::new("yt-dlp")
            .args([
                "--flat-playlist",
                "--dump-json",
                "--no-warnings",
                "--ignore-errors",
                &search_term,
            ])
            .output()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("yt-dlp: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Ok(entry) = serde_json::from_str::<YtDlpEntry>(line) {
                if entry.id.is_empty() {
                    continue;
                }

                // Encontrada — reproducir ahora
                kill_player(&state).await;
                let mut ms = state.music.lock().await;
                let (thumb, found_artist) = extract_track_info(&entry);
                let track = MusicTrack {
                    id: entry.id.clone(),
                    title: entry.title.clone(),
                    artist: found_artist,
                    thumbnail: thumb,
                    duration: entry.duration.unwrap_or(0.0) as u32,
                    added_by: Some(format!("Suerte ({})", username)),
                };

                if let Some(current) = ms.current.take() {
                    ms.queue.insert(0, current);
                }

                add_to_history(&mut ms, &track, &username);
                ms.current = Some(track);
                ms.playback_started_at = Some(now_epoch_secs());
                ms.elapsed = 0;
                ms.started_by = Some(username.clone());
                ms.paused = false;
                drop(ms);

                spawn_player(&state, &entry.id).await;

                state
                    .log_activity(
                        "musica",
                        &format!("Suerte: {} - {}", entry.title, artist),
                        &username,
                    )
                    .await;

                return Ok(Json(state.music.lock().await.clone()));
            }
        }
    }

    Err((
        StatusCode::NOT_FOUND,
        "No se encontro ninguna cancion similar en YouTube".to_string(),
    ))
}
