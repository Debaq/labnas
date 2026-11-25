//! Monitor: estado en vivo por el bus y avisos de fin de impresion/error

use super::*;

// =====================
// Monitor loop - notificaciones de fin de impresión
// =====================

/// Estado previo de cada impresora para detectar cambios
pub(super) struct PrinterMonitorState {
    was_printing: bool,
    file_name: String,
    start_time: Option<std::time::Instant>,
    /// Timelapse en curso: carpeta, fotogramas guardados y ultima foto
    timelapse: Option<(std::path::PathBuf, u32, std::time::Instant)>,
}

/// Guarda un fotograma; devuelve la sesion actualizada (o la misma si fallo)
async fn capture_frame(
    client: &reqwest::Client,
    printer: &Printer3DConfig,
    session: (std::path::PathBuf, u32, std::time::Instant),
) -> (std::path::PathBuf, u32, std::time::Instant) {
    let (dir, frames, _) = session.clone();
    match fetch_snapshot(client, printer).await {
        Ok((_, bytes)) => {
            let path = frame_path(&dir, frames + 1);
            let ok = tokio::task::spawn_blocking(move || {
                std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new("/")))
                    .and_then(|_| std::fs::write(&path, &bytes))
                    .is_ok()
            })
            .await
            .unwrap_or(false);
            if ok { (dir, frames + 1, std::time::Instant::now()) } else { session }
        }
        Err(_) => session,
    }
}

/// Fin de impresion con timelapse: arma el video (si hay ffmpeg) y avisa
fn finish_timelapse(state: &AppState, printer_name: String, dir: std::path::PathBuf, frames: u32) {
    let state = state.clone();
    tokio::spawn(async move {
        let d = dir.clone();
        let result = tokio::task::spawn_blocking(move || {
            if ffmpeg_available() { assemble(&d).map(Some) } else { Ok(None) }
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        let (level, body) = match result {
            Ok(Some(video)) => (crate::events::Level::Success, format!("{}", video.display())),
            Ok(None) => (crate::events::Level::Info, format!("{} fotos en {} (instala ffmpeg para armar el video)", frames, dir.display())),
            Err(e) => (crate::events::Level::Warning, format!("No se pudo armar el video: {} (fotos en {})", e, dir.display())),
        };
        state.log_activity("Timelapse", &format!("{}: {}", printer_name, body), "sistema").await;
        crate::events::notify(
            &state,
            crate::events::Audience::All,
            Some("printers3d"),
            level,
            &format!("Timelapse de {}", printer_name),
            &body,
        );
    });
}

/// Ultimo estado visto de cada impresora (imprimiendo, cuando): la deteccion de
/// impresion fallida solo analiza mientras imprime
static PRINTING: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, (bool, std::time::Instant)>>> =
    std::sync::LazyLock::new(Default::default);

/// ¿Esta imprimiendo? None si el monitor no la ha visto hace rato (modulo apagado, etc.)
pub fn printing_now(id: &str) -> Option<bool> {
    let m = PRINTING.lock().ok()?;
    let (printing, at) = m.get(id)?;
    (at.elapsed() < Duration::from_secs(120)).then_some(*printing)
}

fn set_printing(id: &str, printing: bool) {
    if let Ok(mut m) = PRINTING.lock() {
        m.insert(id.to_string(), (printing, std::time::Instant::now()));
    }
}

/// Espera antes de la foto de fin: muchas impresoras retiran el cabezal al terminar
#[cfg(not(test))]
const FINISH_PHOTO_DELAY: Duration = Duration::from_secs(12);
#[cfg(test)]
const FINISH_PHOTO_DELAY: Duration = Duration::from_millis(0);

/// Piezas de la cola que se estan imprimiendo en esa impresora: (quien la pidio, titulo)
async fn queue_printing(state: &AppState, printer_id: &str) -> Vec<(String, String)> {
    let pid = printer_id.to_string();
    db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare("SELECT requested_by, title FROM print_queue WHERE printer_id = ?1 AND status = 'imprimiendo'")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([&pid], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .await
    .unwrap_or_default()
}

/// Aviso personal a quien encargo la pieza (antes que el general: la bandeja muestra
/// uno solo si la persona tambien sigue los del laboratorio)
async fn tell_owners(state: &AppState, printer_id: &str, level: crate::events::Level, title: &str, body: impl Fn(&str) -> String) {
    for (owner, piece) in queue_printing(state, printer_id).await {
        crate::events::notify(state, crate::events::Audience::User(owner), Some("printers3d"), level, title, &body(&piece));
    }
}

/// Aviso de fin por Telegram con la foto de la pieza (si la impresora tiene camara).
/// La foto va al personal y a quien encargo la pieza en la cola; ver `notify_print_finished`.
pub fn announce_finished(state: &AppState, printer: &Printer3DConfig, file: &str, elapsed: &str) {
    let text = format!("Impresion terminada en *{}*\nArchivo: `{}`\nTiempo: {}", printer.name, file, elapsed);
    let caption = format!("Impresion terminada en {}\nArchivo: {}\nTiempo: {}", printer.name, file, elapsed);
    let (state, printer) = (state.clone(), printer.clone());
    tokio::spawn(async move {
        let mut owners: Vec<String> = queue_printing(&state, &printer.id).await.into_iter().map(|(o, _)| o).collect();
        owners.sort();
        owners.dedup();
        let photo = if camera_url_for(&printer).is_some() {
            tokio::time::sleep(FINISH_PHOTO_DELAY).await;
            fetch_snapshot(&state.http_client, &printer).await.ok().map(|(_, b)| b.to_vec())
        } else {
            None
        };
        crate::handlers::notifications::notify_print_finished(&state, photo, &text, &caption, &owners).await;
    });
}

/// Consulta todas las impresoras (en paralelo): cada 5 s si hay alguien conectado al
/// bus de eventos (publica "printers3d.status"), si no cada 30 s solo para detectar
/// fin de impresion / error y notificar (UI + Telegram).
pub async fn printer_monitor_loop(state: AppState) {
    const LIVE_EVERY: Duration = Duration::from_secs(5);
    const IDLE_EVERY: Duration = Duration::from_secs(30);

    let mut printer_states: std::collections::HashMap<String, PrinterMonitorState> =
        std::collections::HashMap::new();
    let mut last_poll: Option<std::time::Instant> = None;

    loop {
        tokio::time::sleep(LIVE_EVERY).await;

        let live = state.events.has_interest("printers3d.status");
        // con un timelapse en curso se consulta seguido para no perder fotos
        let recording = printer_states.values().any(|p| p.timelapse.is_some());
        if !live && !recording && last_poll.is_some_and(|t| t.elapsed() < IDLE_EVERY) {
            continue;
        }
        last_poll = Some(std::time::Instant::now());

        let printers: Vec<Printer3DConfig> = match db_op(&state.db, |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, ip, port, printer_type, labnas_decrypt(api_key), camera_url, power_watts, electricity_cost_kwh, section_id, \"order\" FROM printers3d",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], row_to_printer).map_err(|e| e.to_string())?;
            Ok(rows.filter_map(|r| r.ok()).collect())
        })
        .await
        {
            Ok(p) => p,
            Err(_) => continue,
        };
        if printers.is_empty() {
            continue;
        }

        let (tl_config, tl_dir) = db_op(&state.db, |conn| {
            let c = load_config(conn);
            let d = effective_dir(conn, &c);
            Ok((c, d))
        })
        .await
        .unwrap_or_default();
        let tl_interval = Duration::from_secs(tl_config.interval_secs.max(5));

        let client = &state.http_client;
        let results: Vec<Option<Printer3DStatus>> = futures_util::future::join_all(
            printers.iter().map(|p| async move { fetch_status(client, p).await.ok().map(|Json(s)| s) }),
        )
        .await;

        if live {
            let map: std::collections::HashMap<&str, &Printer3DStatus> = printers
                .iter()
                .zip(results.iter())
                .filter_map(|(p, s)| s.as_ref().map(|s| (p.id.as_str(), s)))
                .collect();
            state.events.publish("printers3d.status", &map, crate::events::Audience::All, Some("printers3d"));
        }

        for (printer, status) in printers.iter().zip(results) {
            let Some(status) = status else {
                set_printing(&printer.id, false);
                continue;
            };
            if !status.online {
                set_printing(&printer.id, false);
                continue;
            }

            let job_state = status.current_job.as_ref().map(|j| j.state.to_lowercase()).unwrap_or_default();
            let is_printing = job_state.contains("printing");
            set_printing(&printer.id, is_printing);
            let has_error = job_state.contains("error");
            let file_name = status.current_job.as_ref().map(|j| j.file_name.clone()).unwrap_or_default();

            let prev = printer_states.get(&printer.id);
            let was_printing = prev.map(|p| p.was_printing).unwrap_or(false);

            // Transicion: imprimiendo -> terminado
            if was_printing && !is_printing && !has_error {
                let prev_file = prev.map(|p| p.file_name.clone()).unwrap_or_else(|| "?".to_string());
                let elapsed = prev
                    .and_then(|p| p.start_time)
                    .map(|t| {
                        let secs = t.elapsed().as_secs();
                        let (h, m) = (secs / 3600, (secs % 3600) / 60);
                        if h > 0 { format!("{}h {}m", h, m) } else { format!("{}m", m) }
                    })
                    .unwrap_or_else(|| "?".to_string());

                announce_finished(&state, printer, &prev_file, &elapsed);
                tell_owners(&state, &printer.id, crate::events::Level::Success, "Tu pieza está lista", |piece| {
                    format!("«{}» terminó en {} ({})", piece, printer.name, elapsed)
                })
                .await;
                crate::events::notify(
                    &state,
                    crate::events::Audience::All,
                    Some("printers3d"),
                    crate::events::Level::Success,
                    &format!("Impresion terminada en {}", printer.name),
                    &format!("{} ({})", prev_file, elapsed),
                );
                state
                    .log_activity(
                        "Impresoras 3D",
                        &format!("Impresion terminada: {} en {}", prev_file, printer.name),
                        "sistema",
                    )
                    .await;
            }

            // Error durante una impresion
            if has_error && was_printing {
                let error_state = status.current_job.as_ref().map(|j| j.state.clone()).unwrap_or_else(|| "Error".to_string());
                crate::handlers::notifications::notify_active_chats(
                    &state,
                    &format!("Error en impresora *{}*\nEstado: {}", printer.name, error_state),
                )
                .await;
                tell_owners(&state, &printer.id, crate::events::Level::Error, "Tu pieza tuvo un problema", |piece| {
                    format!("«{}» en {}: {}", piece, printer.name, error_state)
                })
                .await;
                crate::events::notify(
                    &state,
                    crate::events::Audience::All,
                    Some("printers3d"),
                    crate::events::Level::Error,
                    &format!("Error en {}", printer.name),
                    &error_state,
                );
            }

            // Timelapse: fotos mientras imprime; al terminar, video
            let mut timelapse = prev.and_then(|p| p.timelapse.clone());
            let wants_tl = tl_config.printers.contains(&printer.id);
            if is_printing && wants_tl {
                if let Some(base) = &tl_dir {
                    let session = timelapse.take().unwrap_or_else(|| {
                        let never = std::time::Instant::now() - tl_interval * 2;
                        (session_dir(base, &printer.name), 0, never)
                    });
                    timelapse = Some(if session.2.elapsed() >= tl_interval {
                        capture_frame(client, printer, session).await
                    } else {
                        session
                    });
                }
            } else if !is_printing {
                if let Some((dir, frames, _)) = timelapse.take() {
                    if frames > 0 {
                        finish_timelapse(&state, printer.name.clone(), dir, frames);
                    }
                }
            }

            printer_states.insert(
                printer.id.clone(),
                PrinterMonitorState {
                    was_printing: is_printing,
                    file_name,
                    start_time: if is_printing && !was_printing {
                        Some(std::time::Instant::now())
                    } else if is_printing {
                        prev.and_then(|p| p.start_time)
                    } else {
                        None
                    },
                    timelapse,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printer(camera_url: String) -> Printer3DConfig {
        Printer3DConfig {
            id: "p1".into(),
            name: "Prusa".into(),
            ip: "127.0.0.1".into(),
            port: 7125,
            printer_type: Printer3DType::Moonraker,
            api_key: None,
            camera_url: Some(camera_url),
            power_watts: None,
            electricity_cost_kwh: None,
            section_id: None,
            order: 0,
        }
    }

    #[tokio::test]
    async fn captura_fotogramas_de_la_camara() {
        // "camara" local que devuelve un JPEG
        let app = axum::Router::new().route("/snap", axum::routing::get(|| async { (StatusCode::OK, vec![0xFFu8, 0xD8, 0xFF, 0xD9]) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.ok() });

        let dir = std::env::temp_dir().join(format!("labnas-tlcap-{}", uuid::Uuid::new_v4()));
        let client = reqwest::Client::new();
        let p = printer(format!("http://127.0.0.1:{}/snap", port));
        let start = (dir.clone(), 0, std::time::Instant::now());

        let s1 = capture_frame(&client, &p, start).await;
        let s2 = capture_frame(&client, &p, s1).await;
        assert_eq!(s2.1, 2);
        assert_eq!(std::fs::read(frame_path(&dir, 2)).unwrap(), vec![0xFF, 0xD8, 0xFF, 0xD9]);

        // camara caida: la sesion queda igual
        let caida = printer("http://127.0.0.1:1/no".into());
        let s3 = capture_frame(&client, &caida, s2).await;
        assert_eq!(s3.1, 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
