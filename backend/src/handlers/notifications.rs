use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::time::Duration;
use sysinfo::{Disks, System};

use chrono::Timelike;

use crate::config::save_config;
use crate::models::notifications::*;
use crate::state::AppState;

// =====================
// API Handlers
// =====================

pub async fn get_config(State(state): State<AppState>) -> Json<NotificationConfig> {
    let config = state.config.lock().await;
    Json(config.notifications.clone())
}

pub async fn set_bot_token(
    State(state): State<AppState>,
    Json(req): Json<SetBotTokenRequest>,
) -> Result<(StatusCode, Json<NotificationConfig>), (StatusCode, String)> {
    let token = req.token.trim().to_string();
    if token.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Token vacio".to_string()));
    }

    // Validate token by calling getMe
    let bot_info = call_telegram::<TgBotInfo>(
        &state.http_client,
        &token,
        "getMe",
        &serde_json::json!({}),
    )
    .await
    .map_err(|e| (StatusCode::BAD_REQUEST, format!("Token invalido: {}", e)))?;

    let mut config = state.config.lock().await;
    config.notifications.bot_token = Some(token);
    config.notifications.bot_username = Some(bot_info.username);

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let notif = config.notifications.clone();
    Ok((StatusCode::OK, Json(notif)))
}

pub async fn delete_bot_token(
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    config.notifications.bot_token = None;
    config.notifications.bot_username = None;
    config.notifications.telegram_chats.clear();

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_chat_role(
    State(state): State<AppState>,
    Path(chat_id): Path<i64>,
    Json(req): Json<SetRoleRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let chat = config
        .notifications
        .telegram_chats
        .iter_mut()
        .find(|c| c.chat_id == chat_id)
        .ok_or((StatusCode::NOT_FOUND, "Chat no encontrado".to_string()))?;

    chat.role = req.role;
    if let Some(perms) = req.permissions {
        chat.permissions = perms;
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::OK)
}

pub async fn delete_chat(
    State(state): State<AppState>,
    Path(chat_id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.notifications.telegram_chats.len();
    config
        .notifications
        .telegram_chats
        .retain(|c| c.chat_id != chat_id);

    if config.notifications.telegram_chats.len() == before {
        return Err((StatusCode::NOT_FOUND, "Chat no encontrado".to_string()));
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_schedule(
    State(state): State<AppState>,
    Json(req): Json<ScheduleRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    config.notifications.daily_enabled = req.daily_enabled;
    config.notifications.daily_hour = req.daily_hour.min(23);
    config.notifications.daily_minute = req.daily_minute.min(59);

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::OK)
}

pub async fn send_test(
    State(state): State<AppState>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let config = state.config.lock().await;
    let token = config.notifications.bot_token.clone();
    let chats = config.notifications.telegram_chats.clone();
    drop(config);

    let token = token.ok_or((StatusCode::BAD_REQUEST, "Bot no configurado".to_string()))?;
    if chats.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "No hay chats registrados. Envia /start al bot desde Telegram.".to_string(),
        ));
    }

    let message = build_status_message(&state).await;
    let mut sent = 0;
    let mut errors = Vec::new();

    for chat in &chats {
        match send_telegram_message(&state.http_client, &token, chat.chat_id, &message).await {
            Ok(_) => sent += 1,
            Err(e) => errors.push(format!("{}: {}", chat.name, e)),
        }
    }

    if errors.is_empty() {
        Ok((
            StatusCode::OK,
            format!("Mensaje enviado a {} chat(s)", sent),
        ))
    } else {
        Ok((
            StatusCode::OK,
            format!("Enviado a {}. Errores: {}", sent, errors.join(", ")),
        ))
    }
}

// =====================
// Telegram API helpers
// =====================

async fn call_telegram<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    token: &str,
    method: &str,
    body: &serde_json::Value,
) -> Result<T, String> {
    let url = format!("https://api.telegram.org/bot{}/{}", token, method);
    let resp = client
        .post(&url)
        .json(body)
        .timeout(Duration::from_secs(35))
        .send()
        .await
        .map_err(|e| format!("Error de conexion: {}", e))?;

    let tg_resp: TgResponse<T> = resp
        .json()
        .await
        .map_err(|e| format!("Error parseando respuesta: {}", e))?;

    if tg_resp.ok {
        tg_resp
            .result
            .ok_or_else(|| "Respuesta vacia de Telegram".to_string())
    } else {
        Err(tg_resp
            .description
            .unwrap_or_else(|| "Error desconocido".to_string()))
    }
}

async fn send_telegram_message(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "Markdown"
    });
    let _: serde_json::Value = call_telegram(client, token, "sendMessage", &body).await?;
    Ok(())
}

async fn get_updates(
    client: &reqwest::Client,
    token: &str,
    offset: i64,
) -> Result<Vec<TgUpdate>, String> {
    let body = serde_json::json!({
        "offset": offset,
        "timeout": 30,
        "allowed_updates": ["message"]
    });
    call_telegram(client, token, "getUpdates", &body).await
}

// =====================
// Bot polling loop
// =====================

pub async fn telegram_bot_loop(state: AppState) {
    let mut offset: i64 = 0;
    let mut startup_sent = false;

    loop {
        let config = state.config.lock().await;
        let token = config.notifications.bot_token.clone();
        let chats = config.notifications.telegram_chats.clone();
        drop(config);

        let Some(token) = token else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        // Send startup greeting once (retries if no internet)
        if !startup_sent && !chats.is_empty() {
            let local_ip = local_ip_address::local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|_| "?".to_string());
            let msg = format!(
                "*LabNAS encendido*\n\nIP: `{}`\nWeb: http://{}:3001\n\nUsa /ayuda para ver comandos.",
                local_ip, local_ip
            );
            let mut all_ok = true;
            for chat in &chats {
                if send_telegram_message(&state.http_client, &token, chat.chat_id, &msg).await.is_err() {
                    all_ok = false;
                }
            }
            if all_ok {
                startup_sent = true;
                println!("[Telegram] Saludo de inicio enviado");
                state.log_activity("Sistema", "LabNAS encendido", "sistema").await;
            } else {
                eprintln!("[Telegram] Sin internet, reintentando saludo en 15s...");
                tokio::time::sleep(Duration::from_secs(15)).await;
                continue;
            }
        } else if chats.is_empty() {
            startup_sent = true; // No chats, skip greeting
        }

        match get_updates(&state.http_client, &token, offset).await {
            Ok(updates) => {
                for update in updates {
                    offset = update.update_id + 1;
                    if let Some(message) = update.message {
                        handle_message(&state, &token, &message).await;
                    }
                }
            }
            Err(e) => {
                eprintln!("[Telegram] Error polling: {}", e);
                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        }
    }
}

async fn handle_message(state: &AppState, token: &str, msg: &TgMessage) {
    let chat_id = msg.chat.id;
    let text = msg.text.as_deref().unwrap_or("").trim();

    // Handle /start separately (always allowed)
    if text.starts_with("/start") {
        register_chat(state, token, msg).await;
        // Check if they're pending
        let config = state.config.lock().await;
        let role = config
            .notifications
            .telegram_chats
            .iter()
            .find(|c| c.chat_id == chat_id)
            .map(|c| c.role.clone())
            .unwrap_or(UserRole::Pendiente);
        drop(config);

        let response = match role {
            UserRole::Admin => "Hola! Eres el *administrador* de LabNAS.\n\nUsa /ayuda para ver los comandos.".to_string(),
            UserRole::Pendiente => "Solicitud enviada. El administrador debe aprobar tu acceso desde la web.".to_string(),
            _ => "Hola! Ya estas registrado en LabNAS.\n\nUsa /ayuda para ver los comandos.".to_string(),
        };
        let _ = send_telegram_message(&state.http_client, token, chat_id, &response).await;
        return;
    }

    // Check user role
    let config = state.config.lock().await;
    let chat = config
        .notifications
        .telegram_chats
        .iter()
        .find(|c| c.chat_id == chat_id);

    let Some(chat) = chat else {
        drop(config);
        let _ = send_telegram_message(&state.http_client, token, chat_id, "No estas registrado. Envia /start primero.").await;
        return;
    };

    if chat.role == UserRole::Pendiente {
        drop(config);
        let _ = send_telegram_message(&state.http_client, token, chat_id, "Tu acceso esta pendiente de aprobacion.").await;
        return;
    }

    let role = chat.role.clone();
    drop(config);

    let response = match text {
        s if s.starts_with("/horario") => handle_schedule_command(state, chat_id, s).await,
        s if s.starts_with("/actividad") => build_activity_message(state).await,
        s if s.starts_with("/estado") => build_status_message(state).await,
        s if s.starts_with("/discos") => build_disks_message().await,
        s if s.starts_with("/ram") => build_ram_message().await,
        s if s.starts_with("/cpu") => build_cpu_message().await,
        s if s.starts_with("/uptime") => build_uptime_message(state),
        s if s.starts_with("/red") => build_network_message(state).await,
        s if s.starts_with("/impresoras") => build_printers_message(state).await,
        s if s.starts_with("/mirol") => {
            let emoji = match role {
                UserRole::Admin => "👑",
                UserRole::Operador => "🔧",
                UserRole::Observador => "👁",
                UserRole::Pendiente => "⏳",
            };
            let role_name = match role {
                UserRole::Admin => "Administrador",
                UserRole::Operador => "Operador",
                UserRole::Observador => "Observador",
                UserRole::Pendiente => "Pendiente",
            };
            format!("{} Tu rol: *{}*", emoji, role_name)
        }
        s if s.starts_with("/ayuda") | s.starts_with("/help") => build_help_message(&role),
        _ => return,
    };

    if let Err(e) =
        send_telegram_message(&state.http_client, token, chat_id, &response).await
    {
        eprintln!("[Telegram] Error enviando a {}: {}", chat_id, e);
    }
}

async fn register_chat(state: &AppState, token: &str, msg: &TgMessage) {
    let chat_id = msg.chat.id;
    let name = msg
        .chat
        .title
        .clone()
        .or_else(|| {
            let first = msg.chat.first_name.as_deref().unwrap_or("");
            let last = msg.chat.last_name.as_deref().unwrap_or("");
            let full = format!("{} {}", first, last).trim().to_string();
            if full.is_empty() {
                None
            } else {
                Some(full)
            }
        })
        .unwrap_or_else(|| format!("Chat {}", chat_id));

    let username = msg.chat.username.clone();
    let mut config = state.config.lock().await;

    // Update existing
    if let Some(existing) = config
        .notifications
        .telegram_chats
        .iter_mut()
        .find(|c| c.chat_id == chat_id)
    {
        existing.name = name;
        existing.username = username;
        let _ = save_config(&config).await;
        return;
    }

    // First user ever = admin, rest = pendiente
    let is_first = config.notifications.telegram_chats.is_empty();
    let role = if is_first {
        UserRole::Admin
    } else {
        UserRole::Pendiente
    };

    let new_chat = TelegramChat {
        chat_id,
        name: name.clone(),
        username: username.clone(),
        role: role.clone(),
        permissions: if is_first {
            UserPermissions {
                terminal: true,
                impresion: true,
                archivos_escritura: true,
            }
        } else {
            UserPermissions::default()
        },
        daily_enabled: false,
        daily_hour: 8,
        daily_minute: 0,
    };

    // Notify admins about new pending user
    if !is_first {
        let admins: Vec<i64> = config
            .notifications
            .telegram_chats
            .iter()
            .filter(|c| c.role == UserRole::Admin)
            .map(|c| c.chat_id)
            .collect();
        let uname = username.as_deref().map(|u| format!(" (@{})", u)).unwrap_or_default();
        let alert = format!(
            "Nuevo usuario solicita acceso:\n*{}*{}\n\nApruebalo desde la web en Configuracion > Telegram.",
            name, uname
        );
        drop(config);
        for admin_id in &admins {
            let _ = send_telegram_message(&state.http_client, token, *admin_id, &alert).await;
        }
        let mut config = state.config.lock().await;
        config.notifications.telegram_chats.push(new_chat);
        let _ = save_config(&config).await;
    } else {
        config.notifications.telegram_chats.push(new_chat);
        let _ = save_config(&config).await;
    }
}

// =====================
// Command responses
// =====================

fn build_help_message(role: &UserRole) -> String {
    let mut msg = String::from("*LabNAS - Comandos*\n\n");
    msg.push_str("/estado - Resumen del sistema\n");
    msg.push_str("/discos - Uso de discos\n");
    msg.push_str("/ram - Memoria RAM\n");
    msg.push_str("/cpu - Procesador\n");
    msg.push_str("/uptime - Tiempo encendido\n");
    msg.push_str("/red - Dispositivos en la red\n");
    msg.push_str("/impresoras - Impresoras 3D\n");
    msg.push_str("/actividad - Actividad reciente\n");
    msg.push_str("/horario HH:MM - Reporte diario\n");
    msg.push_str("/mirol - Ver tu rol y permisos\n");

    let role_name = match role {
        UserRole::Admin => "Admin",
        UserRole::Operador => "Operador",
        UserRole::Observador => "Observador",
        UserRole::Pendiente => "Pendiente",
    };
    msg.push_str(&format!("\nTu rol: *{}*", role_name));
    msg
}

async fn handle_schedule_command(state: &AppState, chat_id: i64, text: &str) -> String {
    let arg = text.strip_prefix("/horario").unwrap_or("").trim();

    if arg.is_empty() {
        // Show current schedule
        let config = state.config.lock().await;
        if let Some(chat) = config.notifications.telegram_chats.iter().find(|c| c.chat_id == chat_id) {
            if chat.daily_enabled {
                return format!("Tu reporte diario esta a las *{:02}:{:02}*\n\nUsa `/horario HH:MM` para cambiar o `/horario off` para desactivar.", chat.daily_hour, chat.daily_minute);
            } else {
                return "Tu reporte diario esta *desactivado*.\n\nUsa `/horario HH:MM` para activar (ej: `/horario 08:00`).".to_string();
            }
        }
        return "No estas registrado. Envia /start primero.".to_string();
    }

    if arg == "off" {
        let mut config = state.config.lock().await;
        if let Some(chat) = config.notifications.telegram_chats.iter_mut().find(|c| c.chat_id == chat_id) {
            chat.daily_enabled = false;
            let _ = save_config(&config).await;
            return "Reporte diario *desactivado*.".to_string();
        }
        return "No estas registrado. Envia /start primero.".to_string();
    }

    // Parse HH:MM
    let parts: Vec<&str> = arg.split(':').collect();
    if parts.len() != 2 {
        return "Formato: `/horario HH:MM` (ej: `/horario 08:30`)\nPara desactivar: `/horario off`".to_string();
    }

    let hour: u8 = match parts[0].parse() {
        Ok(h) if h <= 23 => h,
        _ => return "Hora invalida (0-23)".to_string(),
    };
    let minute: u8 = match parts[1].parse() {
        Ok(m) if m <= 59 => m,
        _ => return "Minuto invalido (0-59)".to_string(),
    };

    let mut config = state.config.lock().await;
    if let Some(chat) = config.notifications.telegram_chats.iter_mut().find(|c| c.chat_id == chat_id) {
        chat.daily_enabled = true;
        chat.daily_hour = hour;
        chat.daily_minute = minute;
        let _ = save_config(&config).await;
        format!("Reporte diario activado a las *{:02}:{:02}*", hour, minute)
    } else {
        "No estas registrado. Envia /start primero.".to_string()
    }
}

async fn build_activity_message(state: &AppState) -> String {
    let log = state.activity_log.lock().await;

    if log.is_empty() {
        return "*Actividad*\n\nNo hay actividad registrada aun.".to_string();
    }

    let mut msg = String::from("*Actividad reciente*\n");
    // Show last 15 events
    let start = if log.len() > 15 { log.len() - 15 } else { 0 };

    for event in &log[start..] {
        let time = event.timestamp.with_timezone(&chrono::Local).format("%H:%M");
        msg.push_str(&format!("\n`{}` {} - {}", time, event.action, event.details));
    }

    msg
}

pub async fn build_status_message(state: &AppState) -> String {
    let uptime = state.start_time.elapsed();
    let secs = uptime.as_secs();
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;

    let uptime_str = if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else {
        format!("{}h {}m", hours, mins)
    };

    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "?".to_string());

    let disk_info = tokio::task::spawn_blocking(|| {
        let disks = Disks::new_with_refreshed_list();
        let total: u64 = disks.list().iter().map(|d| d.total_space()).sum();
        let available: u64 = disks.list().iter().map(|d| d.available_space()).sum();
        let used = total.saturating_sub(available);
        let pct = if total > 0 {
            (used as f64 / total as f64 * 100.0) as u64
        } else {
            0
        };
        format!("{}% ({} / {})", pct, format_bytes(used), format_bytes(total))
    })
    .await
    .unwrap_or_else(|_| "N/A".to_string());

    let sys_info = tokio::task::spawn_blocking(|| {
        let mut sys = System::new_all();
        sys.refresh_all();
        let hostname = System::host_name().unwrap_or_else(|| "?".to_string());
        let mem_used = format_bytes(sys.used_memory());
        let mem_total = format_bytes(sys.total_memory());
        (hostname, format!("{} / {}", mem_used, mem_total))
    })
    .await
    .unwrap_or_else(|_| ("?".to_string(), "N/A".to_string()));

    let hosts = state.scanned_hosts.lock().await;
    let active_hosts = hosts.iter().filter(|h| h.is_alive).count();
    drop(hosts);

    let config = state.config.lock().await;
    let printer_count = config.printers3d.len();
    drop(config);

    format!(
        "*LabNAS - Reporte*\n\n\
         Host: `{}`\n\
         IP: `{}`\n\
         Uptime: {}\n\n\
         Disco: {}\n\
         RAM: {}\n\
         Red: {} hosts activos\n\
         Impresoras 3D: {}",
        sys_info.0, local_ip, uptime_str, disk_info, sys_info.1, active_hosts, printer_count
    )
}

async fn build_disks_message() -> String {
    tokio::task::spawn_blocking(|| {
        let disks = Disks::new_with_refreshed_list();
        let mut msg = String::from("*Discos*\n");

        for d in disks.list() {
            let mount = d.mount_point().to_string_lossy();
            let total = d.total_space();
            let available = d.available_space();
            let used = total.saturating_sub(available);
            let pct = if total > 0 {
                (used as f64 / total as f64 * 100.0) as u64
            } else {
                0
            };
            let bar = progress_bar(pct as f64);

            msg.push_str(&format!(
                "\n`{}`\n{} {}%\n{} / {}\n",
                mount,
                bar,
                pct,
                format_bytes(used),
                format_bytes(total),
            ));
        }
        msg
    })
    .await
    .unwrap_or_else(|_| "Error obteniendo discos".to_string())
}

async fn build_ram_message() -> String {
    tokio::task::spawn_blocking(|| {
        let mut sys = System::new_all();
        sys.refresh_all();
        let total = sys.total_memory();
        let used = sys.used_memory();
        let pct = if total > 0 {
            (used as f64 / total as f64 * 100.0) as u64
        } else {
            0
        };
        let bar = progress_bar(pct as f64);
        let swap_total = sys.total_swap();
        let swap_used = sys.used_swap();

        format!(
            "*RAM*\n\n\
             {} {}%\n\
             {} / {}\n\n\
             Swap: {} / {}",
            bar,
            pct,
            format_bytes(used),
            format_bytes(total),
            format_bytes(swap_used),
            format_bytes(swap_total),
        )
    })
    .await
    .unwrap_or_else(|_| "Error obteniendo RAM".to_string())
}

async fn build_cpu_message() -> String {
    tokio::task::spawn_blocking(|| {
        let mut sys = System::new_all();
        sys.refresh_all();
        // Need a small delay for CPU usage to be accurate
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu_all();

        let cpu_count = sys.cpus().len();
        let global_usage = sys.global_cpu_usage();
        let cpu_name = sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "?".to_string());
        let bar = progress_bar(global_usage as f64);

        format!(
            "*CPU*\n\n\
             {} {:.0}%\n\
             {} nucleos\n\
             `{}`",
            bar, global_usage, cpu_count, cpu_name
        )
    })
    .await
    .unwrap_or_else(|_| "Error obteniendo CPU".to_string())
}

fn build_uptime_message(state: &AppState) -> String {
    let uptime = state.start_time.elapsed();
    let secs = uptime.as_secs();
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;

    let uptime_str = if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    };

    format!("*Uptime*\n\nLabNAS lleva encendido: {}", uptime_str)
}

async fn build_network_message(state: &AppState) -> String {
    let hosts = state.scanned_hosts.lock().await;
    let alive: Vec<_> = hosts.iter().filter(|h| h.is_alive).collect();

    if alive.is_empty() {
        return "*Red*\n\nNo hay hosts detectados.\nEjecuta un escaneo desde la UI primero."
            .to_string();
    }

    let mut msg = format!("*Red* - {} hosts activos\n", alive.len());
    for h in &alive {
        let name = h
            .hostname
            .as_deref()
            .unwrap_or("?");
        let ms = h
            .response_time_ms
            .map(|ms| format!(" ({}ms)", ms))
            .unwrap_or_default();
        msg.push_str(&format!("\n`{}` - {}{}", h.ip, name, ms));
    }
    msg
}

async fn build_printers_message(state: &AppState) -> String {
    let config = state.config.lock().await;
    let printers = config.printers3d.clone();
    drop(config);

    if printers.is_empty() {
        return "*Impresoras 3D*\n\nNo hay impresoras configuradas.".to_string();
    }

    let mut msg = format!("*Impresoras 3D* - {}\n", printers.len());
    for p in &printers {
        let ptype = match p.printer_type {
            crate::models::printers3d::Printer3DType::OctoPrint => "OctoPrint",
            crate::models::printers3d::Printer3DType::Moonraker => "Moonraker",
        };
        msg.push_str(&format!("\n`{}` - {} ({}:{})", p.name, ptype, p.ip, p.port));
    }
    msg
}

// =====================
// Utilities
// =====================

fn progress_bar(pct: f64) -> String {
    let filled = (pct / 10.0).round() as usize;
    let empty = 10_usize.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let k = 1024_f64;
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    let i = (bytes as f64).log(k).floor() as usize;
    let i = i.min(sizes.len() - 1);
    format!("{:.1} {}", bytes as f64 / k.powi(i as i32), sizes[i])
}

// =====================
// Daily scheduler
// =====================

pub async fn daily_notification_loop(state: AppState) {
    // Track last sent date per chat_id
    let mut last_sent: std::collections::HashMap<i64, chrono::NaiveDate> = std::collections::HashMap::new();

    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        let config = state.config.lock().await;
        let token = match &config.notifications.bot_token {
            Some(t) => t.clone(),
            None => { drop(config); continue; }
        };
        let chats = config.notifications.telegram_chats.clone();
        drop(config);

        let now = chrono::Local::now();
        let today = now.date_naive();
        let current_hour = now.hour() as u8;
        let current_minute = now.minute() as u8;

        for chat in &chats {
            if !chat.daily_enabled {
                continue;
            }

            // Already sent today?
            if last_sent.get(&chat.chat_id) == Some(&today) {
                continue;
            }

            if current_hour == chat.daily_hour && current_minute >= chat.daily_minute {
                let message = build_status_message(&state).await;
                let activity = build_activity_message(&state).await;
                let full_msg = format!("{}\n\n---\n{}", message, activity);

                if send_telegram_message(&state.http_client, &token, chat.chat_id, &full_msg).await.is_ok() {
                    last_sent.insert(chat.chat_id, today);
                    println!("[LabNAS] Reporte diario enviado a {}", chat.name);
                }
            }
        }
    }
}
