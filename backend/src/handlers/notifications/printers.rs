//! Comandos de impresoras 3D y camara

use super::*;

// =====================
// Comandos de impresoras 3D
// =====================

pub(super) async fn handle_printer_temps(state: &AppState) -> String {
    let printers = {
        let conn = match crate::db::get_conn(&state.db) {
            Ok(c) => c,
            Err(_) => return "Error de base de datos.".to_string(),
        };
        read_printers(&conn).unwrap_or_default()
    };

    if printers.is_empty() {
        return "*Temperaturas*\n\nNo hay impresoras configuradas.".to_string();
    }

    let mut msg = String::from("*Temperaturas*\n");

    for printer in &printers {
        let status = crate::handlers::printers3d::printer_status(
            axum::extract::State(state.clone()),
            axum::extract::Path(printer.id.clone()),
        )
        .await;

        match status {
            Ok(axum::Json(s)) if s.online => {
                msg.push_str(&format!("\n*{}*", printer.name));
                if let Some(temps) = &s.temperatures {
                    let hotend_bar = progress_bar_temp(temps.hotend_actual, temps.hotend_target);
                    let bed_bar = progress_bar_temp(temps.bed_actual, temps.bed_target);
                    msg.push_str(&format!(
                        "\n  Hotend: {} {:.0}/{:.0}°C\n  Cama:   {} {:.0}/{:.0}°C",
                        hotend_bar, temps.hotend_actual, temps.hotend_target,
                        bed_bar, temps.bed_actual, temps.bed_target,
                    ));
                } else {
                    msg.push_str("\n  Sin datos de temperatura");
                }
                if let Some(job) = &s.current_job {
                    let bar = progress_bar(job.progress);
                    msg.push_str(&format!(
                        "\n  {} {} {:.1}%",
                        job.file_name, bar, job.progress
                    ));
                }
            }
            Ok(_) => {
                msg.push_str(&format!("\n*{}* - Offline", printer.name));
            }
            Err(_) => {
                msg.push_str(&format!("\n*{}* - Error", printer.name));
            }
        }
    }

    msg
}

/// Barra de progreso para temperaturas (basada en target)
pub(super) fn progress_bar_temp(actual: f64, target: f64) -> String {
    if target <= 0.0 {
        return "[░░░░░░░░░░]".to_string();
    }
    let pct = (actual / target * 100.0).min(100.0);
    let filled = (pct / 10.0).round() as usize;
    let empty = 10_usize.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

/// Read printers from DB (for use in notification handlers)
pub(super) fn read_printers(conn: &rusqlite::Connection) -> Result<Vec<crate::models::printers3d::Printer3DConfig>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, ip, port, printer_type, labnas_decrypt(api_key), camera_url, power_watts, electricity_cost_kwh, section_id, \"order\" FROM printers3d ORDER BY \"order\"")
        .map_err(|e| format!("DB: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            let pt_str: String = row.get(4)?;
            Ok(crate::models::printers3d::Printer3DConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                ip: row.get(2)?,
                port: row.get(3)?,
                printer_type: match pt_str.as_str() {
                    "octoprint" => crate::models::printers3d::Printer3DType::OctoPrint,
                    "moonraker" => crate::models::printers3d::Printer3DType::Moonraker,
                    "creality_stock" => crate::models::printers3d::Printer3DType::CrealityStock,
                    "flashforge" => crate::models::printers3d::Printer3DType::FlashForge,
                    _ => crate::models::printers3d::Printer3DType::OctoPrint,
                },
                api_key: row.get(5)?,
                camera_url: row.get(6)?,
                power_watts: row.get(7)?,
                electricity_cost_kwh: row.get(8)?,
                section_id: row.get(9)?,
                order: row.get(10)?,
            })
        })
        .map_err(|e| format!("DB: {}", e))?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r.map_err(|e| format!("DB row: {}", e))?);
    }
    Ok(list)
}

pub(super) async fn handle_printer_control(state: &AppState, user: &str, text: &str, command: &str) -> String {
    let printers = {
        let conn = match crate::db::get_conn(&state.db) {
            Ok(c) => c,
            Err(_) => return "Error de base de datos.".to_string(),
        };
        read_printers(&conn).unwrap_or_default()
    };

    if printers.is_empty() {
        return "No hay impresoras configuradas.".to_string();
    }

    // Si solo hay una impresora, usarla directamente
    // Si hay varias, intentar parsear el nombre del argumento
    let printer = if printers.len() == 1 {
        printers[0].clone()
    } else {
        // Intentar extraer nombre de impresora del comando
        let arg = match command {
            "start" => text.strip_prefix("/imprimir ").unwrap_or("").trim(),
            "pause" => text.strip_prefix("/pausar").unwrap_or("").trim(),
            "cancel" => text.strip_prefix("/cancelar3d").unwrap_or("").trim(),
            _ => "",
        };

        if arg.is_empty() {
            // Si no se especifica, listar las disponibles
            let names: Vec<String> = printers.iter().map(|p| p.name.clone()).collect();
            return format!(
                "Especifica la impresora:\n{}\n\nEj: `/{} {}`",
                names.iter().map(|n| format!("  - {}", n)).collect::<Vec<_>>().join("\n"),
                match command { "start" => "imprimir", "pause" => "pausar", "cancel" => "cancelar3d", _ => "?" },
                names[0]
            );
        }

        // Buscar por nombre (match parcial case-insensitive)
        match printers.iter().find(|p| p.name.to_lowercase().contains(&arg.to_lowercase())) {
            Some(p) => p.clone(),
            None => return format!("Impresora '{}' no encontrada.", arg),
        }
    };

    let client = &state.http_client;
    let base = format!("http://{}:{}", printer.ip, printer.port);

    // Tipos no-HTTP: Creality WebSocket y FlashForge TCP
    match printer.printer_type {
        crate::models::printers3d::Printer3DType::CrealityStock => {
            let params = match command {
                "pause" => serde_json::json!({"pause": 1}),
                "cancel" => serde_json::json!({"stop": 1}),
                _ => return "Comando no soportado en Creality stock.".to_string(),
            };
            return match crate::handlers::printers3d::creality_ws_command(
                &printer.ip,
                serde_json::json!({"method": "set", "params": params}),
            )
            .await
            {
                Ok(_) => {
                    let action = if command == "pause" { "pausada" } else { "cancelada" };
                    state.log_activity("Impresoras 3D", &format!("Impresion {} en {} (por {})", action, printer.name, user), user).await;
                    format!("Impresion {} en *{}*", action, printer.name)
                }
                Err(e) => format!("Error: {}", e),
            };
        }
        crate::models::printers3d::Printer3DType::FlashForge => {
            let cmd = match command {
                "start" => "M24",
                "pause" => "M25",
                "cancel" => "M26",
                _ => return "Comando no soportado en FlashForge.".to_string(),
            };
            return match crate::handlers::printers3d::flashforge_command(&printer.ip, cmd).await {
                Ok(_) => {
                    let action = match command {
                        "start" => "iniciada",
                        "pause" => "pausada",
                        "cancel" => "cancelada",
                        _ => "ejecutada",
                    };
                    state.log_activity("Impresoras 3D", &format!("Impresion {} en {} (por {})", action, printer.name, user), user).await;
                    format!("Impresion {} en *{}*", action, printer.name)
                }
                Err(e) => format!("Error: {}", e),
            };
        }
        _ => {} // OctoPrint y Moonraker se manejan abajo via HTTP
    }

    let result = match printer.printer_type {
        crate::models::printers3d::Printer3DType::OctoPrint => {
            let octo_body = match command {
                "start" => serde_json::json!({"command": "start"}),
                "pause" => serde_json::json!({"command": "pause", "action": "pause"}),
                "cancel" => serde_json::json!({"command": "cancel"}),
                _ => return "Comando no soportado.".to_string(),
            };

            let mut req = client
                .post(format!("{}/api/job", base))
                .json(&octo_body)
                .timeout(Duration::from_secs(10));
            if let Some(key) = &printer.api_key {
                req = req.header("X-Api-Key", key);
            }
            req.send().await
        }
        crate::models::printers3d::Printer3DType::Moonraker => {
            let endpoint = match command {
                "start" => "/printer/print/start",
                "pause" => "/printer/print/pause",
                "cancel" => "/printer/print/cancel",
                _ => return "Comando no soportado.".to_string(),
            };
            client
                .post(format!("{}{}", base, endpoint))
                .timeout(Duration::from_secs(10))
                .send()
                .await
        }
        // CrealityStock y FlashForge ya retornaron arriba
        _ => return "Error interno.".to_string(),
    };

    match result {
        Ok(resp) if resp.status().is_success() => {
            let action = match command {
                "start" => "iniciada",
                "pause" => "pausada",
                "cancel" => "cancelada",
                _ => "ejecutada",
            };
            state.log_activity("Impresoras 3D", &format!("Impresion {} en {} (por {})", action, printer.name, user), user).await;
            format!("Impresion {} en *{}*", action, printer.name)
        }
        Ok(resp) => {
            let st = resp.status();
            let body = resp.text().await.unwrap_or_default();
            format!("Error {}: {}", st, body)
        }
        Err(e) => format!("Error de conexion: {}", e),
    }
}

pub(super) async fn handle_camera(state: &AppState, token: &str, chat_id: i64, text: &str) {
    let arg = text.strip_prefix("/camara").or_else(|| text.strip_prefix("/foto")).unwrap_or("").trim();

    let printers = {
        let conn = match crate::db::get_conn(&state.db) {
            Ok(c) => c,
            Err(_) => {
                let _ = send_telegram_message(&state.http_client, token, chat_id, "Error de base de datos.").await;
                return;
            }
        };
        read_printers(&conn).unwrap_or_default()
    };

    if printers.is_empty() {
        let _ = send_telegram_message(&state.http_client, token, chat_id, "No hay impresoras configuradas.").await;
        return;
    }

    // Find printer
    let printer = if printers.len() == 1 {
        &printers[0]
    } else if arg.is_empty() {
        // Send all cameras
        for p in &printers {
            send_printer_photo(state, token, chat_id, p).await;
        }
        return;
    } else {
        match printers.iter().find(|p| p.name.to_lowercase().contains(&arg.to_lowercase())) {
            Some(p) => p,
            None => {
                let _ = send_telegram_message(&state.http_client, token, chat_id, &format!("Impresora '{}' no encontrada.", arg)).await;
                return;
            }
        }
    };

    send_printer_photo(state, token, chat_id, printer).await;
}

pub(super) async fn send_printer_photo(state: &AppState, token: &str, chat_id: i64, printer: &crate::models::printers3d::Printer3DConfig) {
    // Get camera URL
    let cam_url = if let Some(ref url) = printer.camera_url {
        url.clone()
    } else {
        // Default: try common webcam endpoints
        let base = format!("http://{}:{}", printer.ip, printer.port);
        match printer.printer_type {
            crate::models::printers3d::Printer3DType::OctoPrint => format!("{}/webcam/?action=snapshot", base),
            crate::models::printers3d::Printer3DType::Moonraker
            | crate::models::printers3d::Printer3DType::CrealityStock => {
                format!("http://{}:8080/?action=snapshot", printer.ip)
            }
            crate::models::printers3d::Printer3DType::FlashForge => return,
        }
    };

    // Download image
    let resp = state.http_client
        .get(&cam_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    let bytes = match resp {
        Ok(r) if r.status().is_success() => {
            match r.bytes().await {
                Ok(b) => b,
                Err(_) => {
                    let _ = send_telegram_message(&state.http_client, token, chat_id, &format!("Error leyendo imagen de {}", printer.name)).await;
                    return;
                }
            }
        }
        _ => {
            let _ = send_telegram_message(&state.http_client, token, chat_id, &format!("No se pudo obtener imagen de {}.\nURL: `{}`", printer.name, cam_url)).await;
            return;
        }
    };

    // Get status for caption
    let status = crate::handlers::printers3d::printer_status(
        axum::extract::State(state.clone()),
        axum::extract::Path(printer.id.clone()),
    ).await;

    let caption = match status {
        Ok(axum::Json(s)) if s.online => {
            let mut cap = format!("📷 {}", printer.name);
            if let Some(temps) = &s.temperatures {
                cap.push_str(&format!("\n🔥 {:.0}°C / 🛏 {:.0}°C", temps.hotend_actual, temps.bed_actual));
            }
            if let Some(job) = &s.current_job {
                cap.push_str(&format!("\n📄 {} ({:.1}%)", job.file_name, job.progress));
            }
            cap
        }
        _ => format!("📷 {}", printer.name),
    };

    // Send photo via Telegram
    let url = format!("https://api.telegram.org/bot{}/sendPhoto", token);
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name("camera.jpg")
        .mime_str("image/jpeg")
        .unwrap_or_else(|_| reqwest::multipart::Part::bytes(bytes.to_vec()));

    let form = reqwest::multipart::Form::new()
        .text("chat_id", chat_id.to_string())
        .text("caption", caption)
        .part("photo", part);

    let _ = state.http_client
        .post(&url)
        .multipart(form)
        .timeout(Duration::from_secs(15))
        .send()
        .await;
}
