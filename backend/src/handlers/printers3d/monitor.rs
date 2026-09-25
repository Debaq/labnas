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
        if !live && last_poll.is_some_and(|t| t.elapsed() < IDLE_EVERY) {
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
            let Some(status) = status else { continue };
            if !status.online {
                continue;
            }

            let job_state = status.current_job.as_ref().map(|j| j.state.to_lowercase()).unwrap_or_default();
            let is_printing = job_state.contains("printing");
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

                crate::handlers::notifications::notify_active_chats(
                    &state,
                    &format!("Impresion terminada en *{}*\nArchivo: `{}`\nTiempo: {}", printer.name, prev_file, elapsed),
                )
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
                crate::events::notify(
                    &state,
                    crate::events::Audience::All,
                    Some("printers3d"),
                    crate::events::Level::Error,
                    &format!("Error en {}", printer.name),
                    &error_state,
                );
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
                },
            );
        }
    }
}
