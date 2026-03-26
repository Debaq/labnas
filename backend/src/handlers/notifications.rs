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
use crate::models::tasks::{Project, Task, TaskStatus};
use crate::state::AppState;

/// Respuesta sanitizada de NotificationConfig que nunca expone el bot_token
#[derive(serde::Serialize)]
pub struct NotificationConfigResponse {
    pub bot_configured: bool,
    pub bot_username: Option<String>,
    pub telegram_chats: Vec<TelegramChat>,
    pub daily_enabled: bool,
    pub daily_hour: u8,
    pub daily_minute: u8,
}

impl From<&NotificationConfig> for NotificationConfigResponse {
    fn from(config: &NotificationConfig) -> Self {
        Self {
            bot_configured: config.bot_token.is_some(),
            bot_username: config.bot_username.clone(),
            telegram_chats: config.telegram_chats.clone(),
            daily_enabled: config.daily_enabled,
            daily_hour: config.daily_hour,
            daily_minute: config.daily_minute,
        }
    }
}

// =====================
// API Handlers
// =====================

pub async fn get_config(State(state): State<AppState>) -> Json<NotificationConfigResponse> {
    let config = state.config.lock().await;
    Json(NotificationConfigResponse::from(&config.notifications))
}

pub async fn set_bot_token(
    State(state): State<AppState>,
    Json(req): Json<SetBotTokenRequest>,
) -> Result<(StatusCode, Json<NotificationConfigResponse>), (StatusCode, String)> {
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

    let resp = NotificationConfigResponse::from(&config.notifications);
    Ok((StatusCode::OK, Json(resp)))
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

pub async fn send_tg_public(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
) -> Result<(), String> {
    send_telegram_message(client, token, chat_id, text).await
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

            // Detectar Tailscale
            let tailscale_ip = tokio::process::Command::new("tailscale")
                .args(["ip", "-4"])
                .output()
                .await
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty());

            let remote = if let Some(ref ts) = tailscale_ip {
                format!("\nRemoto: http://{}:3001 (Tailscale)", ts)
            } else {
                String::new()
            };

            let msg = format!(
                "*LabNAS encendido*\n\nIP: `{}`\nWeb: http://{}:3001{}\n\nUsa /ayuda para ver comandos.",
                local_ip, local_ip, remote
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
    let chat_name = chat.name.clone();
    let has_terminal = chat.role == UserRole::Admin || chat.permissions.terminal;
    drop(config);

    let response = match text {
        s if s.starts_with("/cmd ") => {
            if !has_terminal {
                "Sin permiso de terminal.".to_string()
            } else {
                handle_cmd(state, chat_id, &chat_name, s).await
            }
        }
        "/kill" => {
            let mut terms = state.tg_terminals.lock().await;
            if let Some(mut session) = terms.remove(&chat_id) {
                let _ = session.child.kill().await;
                "Proceso terminado.".to_string()
            } else {
                "No hay proceso activo.".to_string()
            }
        }
        s if s.starts_with("/evento ") => handle_event_command(state, &chat_name, s).await,
        s if s.starts_with("/eventos") => handle_list_events(state, &chat_name).await,
        s if s.starts_with("/aceptar ") => handle_event_rsvp(state, &chat_name, s, true).await,
        s if s.starts_with("/declinar ") => handle_event_rsvp(state, &chat_name, s, false).await,
        s if s.starts_with("/vincular ") => handle_link_command(state, chat_id, &chat_name, s).await,
        s if s.starts_with("/correo2tarea ") => {
            let uid_str = s.strip_prefix("/correo2tarea ").unwrap_or("").trim();
            crate::handlers::email::telegram_email_to_task(state, &chat_name, uid_str).await
        }
        s if s.starts_with("/leer ") => {
            let uid_str = s.strip_prefix("/leer ").unwrap_or("").trim();
            crate::handlers::email::get_email_detail(state, &chat_name, uid_str).await
        }
        s if s.starts_with("/correos") => {
            crate::handlers::email::get_emails_summary(state, &chat_name).await
        }
        // Música
        "/musica" | "/music" => handle_music_status(state).await,
        s if s.starts_with("/play ") => {
            let query = s.strip_prefix("/play ").unwrap_or("").trim();
            handle_music_play(state, &chat_name, query).await
        }
        "/next" | "/siguiente" => handle_music_next(state).await,
        "/stop" | "/parar" => handle_music_stop(state).await,
        "/pause" | "/pausar" if !text.starts_with("/pausar3d") && !text.starts_with("/pausar ") => handle_music_pause(state).await,
        "/mix" => handle_music_mix(state, &chat_name).await,
        s if s.starts_with("/vol ") => {
            let vol_str = s.strip_prefix("/vol ").unwrap_or("").trim();
            handle_music_volume(state, vol_str).await
        }
        s if s.starts_with("/proyecto ") => handle_project_command(state, &chat_name, s).await,
        s if s.starts_with("/proyectos") => handle_list_projects(state, &chat_name).await,
        s if s.starts_with("/tarea ") => handle_task_command(state, &chat_name, s).await,
        s if s.starts_with("/tareas") => handle_list_tasks(state, &chat_name).await,
        s if s.starts_with("/avance") => handle_progress(state, &chat_name, s).await,
        s if s.starts_with("/confirmar ") => handle_confirm(state, &chat_name, s, true).await,
        s if s.starts_with("/rechazar ") => handle_confirm(state, &chat_name, s, false).await,
        s if s.starts_with("/hecho ") => handle_done(state, &chat_name, s).await,
        "/ip" => {
            let local_ip = local_ip_address::local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|_| "?".to_string());
            let ts = tokio::process::Command::new("tailscale")
                .args(["ip", "-4"]).output().await.ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty());
            let mut msg = format!("*IPs de LabNAS*\n\nLocal: `{}`\nWeb: http://{}:3001", local_ip, local_ip);
            if let Some(ts_ip) = ts {
                msg.push_str(&format!("\n\nTailscale: `{}`\nRemoto: http://{}:3001", ts_ip, ts_ip));
            }
            msg
        }
        s if s.starts_with("/horario") => handle_schedule_command(state, chat_id, s).await,
        s if s.starts_with("/actividad") => build_activity_message(state).await,
        s if s.starts_with("/estado") => build_status_message(state).await,
        s if s.starts_with("/discos") => build_disks_message().await,
        s if s.starts_with("/ram") => build_ram_message().await,
        s if s.starts_with("/cpu") => build_cpu_message().await,
        s if s.starts_with("/uptime") => build_uptime_message(state),
        s if s.starts_with("/red") => build_network_message(state).await,
        s if s.starts_with("/camara") | s.starts_with("/foto") => {
            handle_camera(state, &token, chat_id, s).await;
            return; // ya envió la foto directamente
        }
        s if s.starts_with("/temp") => handle_printer_temps(state).await,
        s if s.starts_with("/imprimir ") => handle_printer_control(state, &chat_name, s, "start").await,
        s if s.starts_with("/pausar") => handle_printer_control(state, &chat_name, s, "pause").await,
        s if s.starts_with("/cancelar3d") => handle_printer_control(state, &chat_name, s, "cancel").await,
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
        _ => {
            // Check if user has active terminal session - pipe input
            if has_terminal {
                if let Some(output) = pipe_terminal_input(state, chat_id, text).await {
                    output
                } else {
                    return; // No session, ignore non-command
                }
            } else {
                return;
            }
        }
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
        linked_web_user: None,
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

// =====================
// Remote terminal via Telegram
// =====================

async fn handle_cmd(state: &AppState, chat_id: i64, user: &str, text: &str) -> String {
    use tokio::io::AsyncBufReadExt;
    use tokio::process::Command as TokioCmd;
    use std::process::Stdio;

    let cmd = text.strip_prefix("/cmd ").unwrap_or("").trim();
    if cmd.is_empty() {
        return "Uso: `/cmd <comando>`\nEj: `/cmd df -h`\n`/cmd sudo pacman -Syu`\n\nSi pide input, envia texto normal (sin /).\n`/kill` para terminar proceso.".to_string();
    }

    // Kill existing session if any
    {
        let mut terms = state.tg_terminals.lock().await;
        if let Some(mut old) = terms.remove(&chat_id) {
            let _ = old.child.kill().await;
        }
    }

    state.log_activity("Terminal TG", cmd, user).await;

    // Spawn process with piped I/O
    let child_result = TokioCmd::new("bash")
        .args(["-c", cmd])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child_result {
        Ok(c) => c,
        Err(e) => return format!("Error: {}", e),
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let stdin = child.stdin.take().unwrap();

    // Channel for output
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(100);

    // Read stdout
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx2.send(line).await.is_err() { break; }
        }
    });

    // Read stderr
    tokio::spawn(async move {
        let reader = tokio::io::BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(format!("[err] {}", line)).await.is_err() { break; }
        }
    });

    // Wait a bit for initial output
    let output = collect_output(rx, child, stdin, state, chat_id, cmd).await;
    output
}

async fn collect_output(
    mut rx: tokio::sync::mpsc::Receiver<String>,
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    state: &AppState,
    chat_id: i64,
    cmd: &str,
) -> String {
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    loop {
        let timeout = tokio::time::timeout_at(deadline, rx.recv()).await;
        match timeout {
            Ok(Some(line)) => lines.push(line),
            _ => break,
        }
    }

    // Check if process is still running
    let mut terms = state.tg_terminals.lock().await;

    // Try to check if child exited
    let mut child = child;
    let still_alive = child.try_wait().map(|s| s.is_none()).unwrap_or(false);

    let mut msg = format!("$ `{}`\n", cmd);
    if !lines.is_empty() {
        let output = lines.join("\n");
        let output = if output.len() > 3500 {
            format!("{}...(truncado)", &output[..3500])
        } else {
            output
        };
        msg.push_str(&format!("```\n{}```", output));
    }

    if still_alive {
        // Process waiting for input - save session
        terms.insert(chat_id, crate::state::TgTerminal {
            stdin,
            output_rx: rx,
            child,
            created_at: std::time::Instant::now(),
        });
        msg.push_str("\n_Proceso activo. Envia texto para input o /kill para terminar._");
    } else {
        let code = child.wait().await.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        if lines.is_empty() {
            msg.push_str("_(sin salida)_");
        }
        if code != 0 {
            msg.push_str(&format!("\nExit: {}", code));
        }
    }

    msg
}

async fn pipe_terminal_input(state: &AppState, chat_id: i64, input: &str) -> Option<String> {
    use tokio::io::AsyncWriteExt;

    let mut terms = state.tg_terminals.lock().await;
    let session = terms.get_mut(&chat_id)?;

    // Check timeout (5 min)
    if session.created_at.elapsed().as_secs() > 300 {
        let mut session = terms.remove(&chat_id).unwrap();
        let _ = session.child.kill().await;
        return Some("Sesion expirada (5 min).".to_string());
    }

    // Write input to stdin
    let write_result = session.stdin.write_all(format!("{}\n", input).as_bytes()).await;
    if write_result.is_err() {
        let mut session = terms.remove(&chat_id).unwrap();
        let _ = session.child.kill().await;
        return Some("Proceso termino.".to_string());
    }

    // Collect output
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    loop {
        let timeout = tokio::time::timeout_at(deadline, session.output_rx.recv()).await;
        match timeout {
            Ok(Some(line)) => lines.push(line),
            _ => break,
        }
    }

    // Check if still alive
    let still_alive = session.child.try_wait().map(|s| s.is_none()).unwrap_or(false);

    let mut msg = String::new();
    if !lines.is_empty() {
        let output = lines.join("\n");
        let output = if output.len() > 3500 {
            format!("{}...(truncado)", &output[..3500])
        } else {
            output
        };
        msg.push_str(&format!("```\n{}```", output));
    }

    if !still_alive {
        let mut session = terms.remove(&chat_id).unwrap();
        let code = session.child.wait().await.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        if code != 0 {
            msg.push_str(&format!("\nExit: {}", code));
        }
        if msg.is_empty() {
            msg = "Proceso termino.".to_string();
        }
    }

    if msg.is_empty() {
        msg = "_Esperando..._".to_string();
    }

    Some(msg)
}

// =====================
// Calendar events (Telegram)
// =====================

use crate::models::tasks::CalendarEvent;

async fn handle_event_command(state: &AppState, creator: &str, text: &str) -> String {
    let args = text.strip_prefix("/evento ").unwrap_or("").trim();
    // Format: /evento 2026-03-20 14:30 Titulo @persona
    let parts: Vec<&str> = args.splitn(3, ' ').collect();
    if parts.len() < 3 {
        return "Uso: `/evento 2026-03-20 14:30 Titulo @persona`\nEj: `/evento 2026-03-25 10:00 Reunion equipo @all`".to_string();
    }
    let date = parts[0].to_string();
    let time = parts[1].to_string();
    let rest = parts[2];

    let mut title_parts = Vec::new();
    let mut invitees = Vec::new();
    for word in rest.split_whitespace() {
        if let Some(mention) = word.strip_prefix('@') {
            invitees.push(mention.to_string());
        } else {
            title_parts.push(word);
        }
    }
    let title = title_parts.join(" ");
    if title.is_empty() {
        return "El evento necesita un titulo.".to_string();
    }

    let event = CalendarEvent {
        id: uuid::Uuid::new_v4().to_string()[..6].to_string(),
        title: title.clone(),
        description: String::new(),
        date: date.clone(),
        time: time.clone(),
        created_by: creator.to_string(),
        invitees: invitees.clone(),
        accepted: Vec::new(),
        declined: Vec::new(),
        remind_before_min: 15,
        reminded: false,
        recurrence: String::new(),
        recurrence_end: None,
        created_at: chrono::Utc::now(),
    };
    let id = event.id.clone();

    let mut config = state.config.lock().await;
    config.tasks.events.push(event);
    let _ = save_config(&config).await;

    // Notify invitees
    let token = config.notifications.bot_token.clone();
    let chats = config.notifications.telegram_chats.clone();
    drop(config);

    if let Some(token) = token {
        let inv_str = if invitees.contains(&"all".to_string()) { "todos".to_string() } else { invitees.join(", ") };
        let alert = format!("📅 *Nuevo evento*\n\n*{}*\nFecha: {} {}\nDe: {}\nInvitados: {}\n\n`/aceptar {}` o `/declinar {}`",
            title, date, time, creator, inv_str, id, id);
        let targets: Vec<i64> = if invitees.contains(&"all".to_string()) {
            chats.iter().filter(|c| c.role != UserRole::Pendiente && c.name != creator).map(|c| c.chat_id).collect()
        } else {
            chats.iter().filter(|c| invitees.iter().any(|i| i.to_lowercase() == c.name.to_lowercase())).map(|c| c.chat_id).collect()
        };
        for cid in &targets {
            let _ = send_telegram_message(&state.http_client, &token, *cid, &alert).await;
        }
    }

    format!("📅 Evento *{}* creado\n{} {}\nID: `{}`", title, date, time, id)
}

async fn handle_list_events(state: &AppState, user: &str) -> String {
    let config = state.config.lock().await;
    let events: Vec<&CalendarEvent> = config.tasks.events.iter()
        .filter(|e| {
            e.created_by == user || e.invitees.contains(&"all".to_string()) ||
            e.invitees.iter().any(|i| i.to_lowercase() == user.to_lowercase())
        })
        .collect();

    if events.is_empty() {
        return "📅 *Mis eventos*\n\nNo tienes eventos. Crea uno con `/evento`".to_string();
    }

    let mut msg = format!("📅 *Mis eventos* ({})\n", events.len());
    for e in &events {
        let status = if e.accepted.iter().any(|a| a.to_lowercase() == user.to_lowercase()) {
            "aceptado"
        } else if e.declined.iter().any(|d| d.to_lowercase() == user.to_lowercase()) {
            "rechazado"
        } else if e.created_by == user {
            "creador"
        } else {
            "pendiente"
        };
        msg.push_str(&format!("\n`{}` *{}*\n  {} {} | {}\n", e.id, e.title, e.date, e.time, status));
    }
    msg
}

async fn handle_event_rsvp(state: &AppState, user: &str, text: &str, accept: bool) -> String {
    let cmd = if accept { "/aceptar " } else { "/declinar " };
    let id = text.strip_prefix(cmd).unwrap_or("").trim();
    if id.is_empty() {
        return format!("Uso: `{}<ID>`", cmd);
    }
    let mut config = state.config.lock().await;
    let event = config.tasks.events.iter_mut().find(|e| e.id == id);
    let Some(event) = event else {
        return format!("Evento `{}` no encontrado.", id);
    };
    let title = event.title.clone();
    if accept {
        event.declined.retain(|u| u != user);
        if !event.accepted.contains(&user.to_string()) {
            event.accepted.push(user.to_string());
        }
        let _ = save_config(&config).await;
        format!("Evento *{}* aceptado.", title)
    } else {
        event.accepted.retain(|u| u != user);
        if !event.declined.contains(&user.to_string()) {
            event.declined.push(user.to_string());
        }
        let _ = save_config(&config).await;
        format!("Evento *{}* rechazado.", title)
    }
}

// =====================
// Account linking
// =====================

async fn handle_link_command(state: &AppState, chat_id: i64, _chat_name: &str, text: &str) -> String {
    let code = text.strip_prefix("/vincular ").unwrap_or("").trim().to_uppercase();
    if code.is_empty() {
        return "Uso: `/vincular CODIGO`\n\nGenera el codigo desde la web en tu perfil.".to_string();
    }

    // Check code validity (expire after 5 min)
    let mut codes = state.link_codes.lock().await;
    let link = codes.remove(&code);
    drop(codes);

    let Some(link) = link else {
        return "Codigo invalido o expirado. Genera uno nuevo desde la web.".to_string();
    };

    if link.created_at.elapsed().as_secs() > 300 {
        return "Codigo expirado. Genera uno nuevo desde la web.".to_string();
    }

    let username = link.username;

    let mut config = state.config.lock().await;

    // Link web user -> telegram
    let web_user = config.web_users.iter_mut().find(|u| u.username == username);
    let Some(web_user) = web_user else {
        return "Usuario web no encontrado.".to_string();
    };
    web_user.linked_telegram = Some(chat_id);
    let role = web_user.role.clone();
    let perms = web_user.permissions.clone();

    // Link telegram -> web user
    if let Some(chat) = config.notifications.telegram_chats.iter_mut().find(|c| c.chat_id == chat_id) {
        chat.linked_web_user = Some(username.clone());
        chat.role = role;
        chat.permissions = perms;
    }

    let _ = save_config(&config).await;

    format!("Cuenta vinculada! Tu Telegram esta conectado con *{}*.", username)
}

// =====================
// Tasks & Projects
// =====================

async fn handle_project_command(state: &AppState, creator: &str, text: &str) -> String {
    let name = text.strip_prefix("/proyecto ").unwrap_or("").trim();
    if name.is_empty() {
        return "Uso: `/proyecto Nombre del proyecto`".to_string();
    }

    let project = Project {
        id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
        name: name.to_string(),
        description: String::new(),
        created_by: creator.to_string(),
        members: vec![creator.to_string()],
        member_tags: std::collections::HashMap::new(),
        created_at: chrono::Utc::now(),
    };

    let mut config = state.config.lock().await;
    let id = project.id.clone();
    config.tasks.projects.push(project);
    let _ = save_config(&config).await;

    format!("Proyecto *{}* creado (ID: `{}`)", name, id)
}

async fn handle_list_projects(state: &AppState, user: &str) -> String {
    let config = state.config.lock().await;
    let projects = &config.tasks.projects;

    if projects.is_empty() {
        return "*Proyectos*\n\nNo hay proyectos. Crea uno con `/proyecto Nombre`".to_string();
    }

    let mut msg = format!("*Proyectos* ({})\n", projects.len());
    for p in projects {
        let task_count = config.tasks.tasks.iter().filter(|t| t.project_id.as_deref() == Some(&p.id)).count();
        let done_count = config.tasks.tasks.iter().filter(|t| t.project_id.as_deref() == Some(&p.id) && t.status == TaskStatus::Completada).count();
        let is_member = p.members.contains(&user.to_string()) || p.created_by == user;
        let badge = if is_member { "" } else { " (no eres miembro)" };
        msg.push_str(&format!("\n`{}` *{}*{}\n  {}/{} tareas completadas\n", p.id, p.name, badge, done_count, task_count));
    }
    msg
}

async fn handle_task_command(state: &AppState, creator: &str, text: &str) -> String {
    let args = text.strip_prefix("/tarea ").unwrap_or("").trim();
    if args.is_empty() {
        return "Uso: `/tarea Titulo @persona !confirmar !insistente`\nEj: `/tarea Revisar servidor @all !confirmar`".to_string();
    }

    // Parse: title, @mentions, !flags
    let mut title_parts = Vec::new();
    let mut assigned = Vec::new();
    let mut requires_confirmation = false;
    let mut insistent = false;
    let mut reminder_minutes: u32 = 8;
    let mut project_id: Option<String> = None;

    for word in args.split_whitespace() {
        if let Some(mention) = word.strip_prefix('@') {
            assigned.push(mention.to_string());
        } else if word == "!confirmar" || word == "!confirmacion" {
            requires_confirmation = true;
        } else if word == "!insistente" || word == "!insistir" {
            insistent = true;
        } else if let Some(mins) = word.strip_prefix("!cada") {
            if let Ok(m) = mins.parse::<u32>() {
                if m >= 1 {
                    reminder_minutes = m;
                    insistent = true;
                }
            }
        } else if let Some(pid) = word.strip_prefix('#') {
            project_id = Some(pid.to_string());
        } else {
            title_parts.push(word);
        }
    }

    let title = title_parts.join(" ");
    if title.is_empty() {
        return "La tarea necesita un titulo.".to_string();
    }

    if assigned.is_empty() {
        assigned.push(creator.to_string());
    }

    // If insistent, it also requires confirmation
    if insistent {
        requires_confirmation = true;
    }

    let task = Task {
        id: uuid::Uuid::new_v4().to_string()[..6].to_string(),
        project_id,
        title: title.clone(),
        description: String::new(),
        assigned_to: assigned.clone(),
        status: TaskStatus::Pendiente,
        created_by: creator.to_string(),
        due_date: None,
        due_time: None,
        requires_confirmation,
        insistent,
        reminder_minutes,
        confirmed_by: Vec::new(),
        rejected_by: Vec::new(),
        created_at: chrono::Utc::now(),
        last_reminder: None,
    };

    let id = task.id.clone();
    let mut config = state.config.lock().await;
    config.tasks.tasks.push(task);
    let _ = save_config(&config).await;

    let assign_str = assigned.join(", ");
    let flags = [
        if requires_confirmation { "confirmar" } else { "" },
        if insistent { "insistente" } else { "" },
    ].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
    let flags_str = if flags.is_empty() { String::new() } else { format!(" ({})", flags) };

    format!("Tarea *{}* creada{}\nID: `{}`\nAsignada a: {}", title, flags_str, id, assign_str)
}

async fn handle_list_tasks(state: &AppState, user: &str) -> String {
    let config = state.config.lock().await;
    let my_tasks: Vec<&Task> = config.tasks.tasks.iter()
        .filter(|t| {
            t.status != TaskStatus::Completada && t.status != TaskStatus::Rechazada &&
            (t.assigned_to.contains(&"all".to_string()) || t.assigned_to.iter().any(|a| a.to_lowercase() == user.to_lowercase()) || t.created_by == user)
        })
        .collect();

    if my_tasks.is_empty() {
        return "*Mis tareas*\n\nNo tienes tareas pendientes.".to_string();
    }

    let mut msg = format!("*Mis tareas* ({})\n", my_tasks.len());
    for t in &my_tasks {
        let status = match t.status {
            TaskStatus::Pendiente => "pendiente",
            TaskStatus::EnProgreso => "en progreso",
            _ => "?",
        };
        let flags = [
            if t.requires_confirmation { "confirmar" } else { "" },
            if t.insistent { "insistente" } else { "" },
        ].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
        let flags_str = if flags.is_empty() { String::new() } else { format!(" [{}]", flags) };

        msg.push_str(&format!("\n`{}` *{}*{}\n  Estado: {} | Por: {}\n", t.id, t.title, flags_str, status, t.created_by));
    }
    msg.push_str("\nUsa `/hecho ID` o `/confirmar ID`");
    msg
}

async fn handle_confirm(state: &AppState, user: &str, text: &str, accept: bool) -> String {
    let cmd = if accept { "/confirmar " } else { "/rechazar " };
    let id = text.strip_prefix(cmd).unwrap_or("").trim();
    if id.is_empty() {
        return format!("Uso: `{}<ID>`", cmd);
    }

    let mut config = state.config.lock().await;
    let task = config.tasks.tasks.iter_mut().find(|t| t.id == id);

    let Some(task) = task else {
        return format!("Tarea `{}` no encontrada.", id);
    };

    let title = task.title.clone();
    if accept {
        if !task.confirmed_by.contains(&user.to_string()) {
            task.confirmed_by.push(user.to_string());
        }
        let _ = save_config(&config).await;
        format!("Tarea *{}* confirmada por ti.", title)
    } else {
        if !task.rejected_by.contains(&user.to_string()) {
            task.rejected_by.push(user.to_string());
        }
        task.status = TaskStatus::Rechazada;
        let _ = save_config(&config).await;
        format!("Tarea *{}* rechazada.", title)
    }
}

async fn handle_done(state: &AppState, user: &str, text: &str) -> String {
    let id = text.strip_prefix("/hecho ").unwrap_or("").trim();
    if id.is_empty() {
        return "Uso: `/hecho <ID>`".to_string();
    }

    let mut config = state.config.lock().await;
    let task = config.tasks.tasks.iter_mut().find(|t| t.id == id);

    let Some(task) = task else {
        return format!("Tarea `{}` no encontrada.", id);
    };

    // Only creator or assigned can mark done
    let is_assigned = task.assigned_to.contains(&"all".to_string()) || task.assigned_to.iter().any(|a| a.to_lowercase() == user.to_lowercase());
    if task.created_by != user && !is_assigned {
        return "No tienes permiso para completar esta tarea.".to_string();
    }

    let title = task.title.clone();
    task.status = TaskStatus::Completada;
    let _ = save_config(&config).await;
    format!("Tarea *{}* completada!", title)
}

async fn handle_progress(state: &AppState, user: &str, text: &str) -> String {
    let project_name = text.strip_prefix("/avance").unwrap_or("").trim();

    let config = state.config.lock().await;

    if project_name.is_empty() {
        // Show all projects progress
        if config.tasks.projects.is_empty() {
            return "*Avance*\n\nNo hay proyectos.".to_string();
        }
        let mut msg = String::from("*Avance de proyectos*\n");
        for p in &config.tasks.projects {
            let tasks: Vec<&Task> = config.tasks.tasks.iter().filter(|t| t.project_id.as_deref() == Some(&p.id)).collect();
            let total = tasks.len();
            let done = tasks.iter().filter(|t| t.status == TaskStatus::Completada).count();
            let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u64 } else { 0 };
            let bar = progress_bar(pct as f64);
            msg.push_str(&format!("\n*{}*\n{} {}% ({}/{})\n", p.name, bar, pct, done, total));
        }
        return msg;
    }

    // Find specific project
    let project = config.tasks.projects.iter().find(|p| p.name.to_lowercase().contains(&project_name.to_lowercase()) || p.id == project_name);
    let Some(project) = project else {
        return format!("Proyecto '{}' no encontrado.", project_name);
    };

    let tasks: Vec<&Task> = config.tasks.tasks.iter().filter(|t| t.project_id.as_deref() == Some(&project.id)).collect();
    let total = tasks.len();
    let done = tasks.iter().filter(|t| t.status == TaskStatus::Completada).count();
    let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u64 } else { 0 };
    let bar = progress_bar(pct as f64);

    let mut msg = format!("*{}*\n{} {}% ({}/{})\n", project.name, bar, pct, done, total);
    let _ = user; // suppress warning

    for t in &tasks {
        let icon = match t.status {
            TaskStatus::Completada => "done",
            TaskStatus::Rechazada => "x",
            TaskStatus::EnProgreso => ">>",
            TaskStatus::Pendiente => "  ",
        };
        msg.push_str(&format!("\n[{}] `{}` {}", icon, t.id, t.title));
    }
    msg
}

// =====================
// Reminder loop for insistent tasks
// =====================

pub async fn task_reminder_loop(state: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await; // Check every minute

        let mut config = state.config.lock().await;
        let token = match &config.notifications.bot_token {
            Some(t) => t.clone(),
            None => { drop(config); continue; }
        };
        let chats = config.notifications.telegram_chats.clone();

        let now = chrono::Utc::now();
        let local_now = chrono::Local::now();
        let today = local_now.format("%Y-%m-%d").to_string();
        let mut to_remind: Vec<(String, Vec<i64>)> = Vec::new(); // (message, chat_ids)

        for task in &mut config.tasks.tasks {
            if task.status == TaskStatus::Completada || task.status == TaskStatus::Rechazada {
                continue;
            }

            // Check due date
            let is_due_today = task
                .due_date
                .as_ref()
                .map(|d| d.as_str() == today.as_str())
                .unwrap_or(false);
            let is_overdue = task
                .due_date
                .as_ref()
                .map(|d| d.as_str() < today.as_str())
                .unwrap_or(false);

            // Skip if no reason to remind
            if !task.requires_confirmation && !task.insistent && !is_due_today && !is_overdue {
                continue;
            }

            // Interval: use task's reminder_minutes for insistent, 720 min (12h) for due date only
            let interval = if task.insistent || task.requires_confirmation {
                (task.reminder_minutes as i64) * 60
            } else {
                720 * 60 // 12 hours for due date reminders
            };

            let should_remind = task
                .last_reminder
                .map(|lr| (now - lr).num_seconds() >= interval)
                .unwrap_or(true);

            if !should_remind {
                continue;
            }

            // Find chat_ids for assigned users
            // @all solo aplica a operadores y admins, no observadores
            let target_ids: Vec<i64> = if task.assigned_to.contains(&"all".to_string()) {
                chats
                    .iter()
                    .filter(|c| {
                        (c.role == UserRole::Admin || c.role == UserRole::Operador)
                            && !task.confirmed_by.contains(&c.name)
                    })
                    .map(|c| c.chat_id)
                    .collect()
            } else {
                chats
                    .iter()
                    .filter(|c| {
                        (task.assigned_to.iter().any(|a| {
                            a.to_lowercase() == c.name.to_lowercase()
                        }) || c.linked_web_user.as_ref().map(|u| {
                            task.assigned_to.iter().any(|a| a.to_lowercase() == u.to_lowercase())
                        }).unwrap_or(false))
                            && !task.confirmed_by.contains(&c.name)
                    })
                    .map(|c| c.chat_id)
                    .collect()
            };

            if target_ids.is_empty() {
                continue;
            }

            let urgency = if is_overdue {
                "🚨 *TAREA VENCIDA*\n\n"
            } else if is_due_today {
                "⚠️ *Vence hoy*\n\n"
            } else {
                ""
            };
            let icon = if task.insistent { "🔔" } else { "📋" };
            let due_info = task
                .due_date
                .as_ref()
                .map(|d| format!("\nVence: {}", d))
                .unwrap_or_default();
            let msg = format!(
                "{}{} *Recordatorio*\n\nTarea: *{}*\nID: `{}`\nPor: {}{}\n\n`/confirmar {}` o `/rechazar {}`",
                urgency, icon, task.title, task.id, task.created_by, due_info, task.id, task.id
            );

            to_remind.push((msg, target_ids));
            task.last_reminder = Some(now);
        }

        let _ = save_config(&config).await;
        drop(config);

        // Check calendar events
        let mut config = state.config.lock().await;
        let local_now = chrono::Local::now();
        let today = local_now.format("%Y-%m-%d").to_string();
        let now_min = local_now.format("%H").to_string().parse::<u32>().unwrap_or(0) * 60
            + local_now.format("%M").to_string().parse::<u32>().unwrap_or(0);

        for event in &mut config.tasks.events {
            if event.reminded || event.date != today {
                continue;
            }
            // Parse event time
            let parts: Vec<&str> = event.time.split(':').collect();
            if parts.len() != 2 {
                continue;
            }
            let (eh, em) = match (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                (Ok(h), Ok(m)) => (h, m),
                _ => continue,
            };
            let event_min = eh * 60 + em;
            let remind_at = event_min.saturating_sub(event.remind_before_min);

            if now_min >= remind_at && now_min < event_min {
                let mins_left = event_min.saturating_sub(now_min);
                let msg = format!(
                    "📅 *Evento en {} min*\n\n*{}*\nHora: {}\n{}",
                    mins_left, event.title, event.time,
                    if event.description.is_empty() { String::new() } else { format!("\n{}", event.description) }
                );

                // Send to creator + invitees
                let targets: Vec<i64> = if event.invitees.contains(&"all".to_string()) {
                    chats.iter().filter(|c| c.role != UserRole::Pendiente).map(|c| c.chat_id).collect()
                } else {
                    let mut ids: Vec<i64> = chats.iter()
                        .filter(|c| event.invitees.iter().any(|i| i.to_lowercase() == c.name.to_lowercase()) || c.name == event.created_by)
                        .map(|c| c.chat_id)
                        .collect();
                    ids.dedup();
                    ids
                };

                to_remind.push((msg, targets));
                event.reminded = true;
            }
        }

        // Avanzar eventos recurrentes que ya pasaron
        let mut new_events: Vec<crate::models::tasks::CalendarEvent> = Vec::new();
        for event in &mut config.tasks.events {
            if !event.reminded || event.recurrence.is_empty() || event.recurrence == "none" {
                continue;
            }
            // Si el evento ya paso hoy, generar proxima ocurrencia
            let event_min_total = event.time.split(':')
                .map(|p| p.parse::<u32>().unwrap_or(0))
                .collect::<Vec<_>>();
            if event_min_total.len() != 2 { continue; }
            let ev_min = event_min_total[0] * 60 + event_min_total[1];
            if ev_min > now_min { continue; } // aun no paso

            // Calcular proxima fecha
            if let Ok(date) = chrono::NaiveDate::parse_from_str(&event.date, "%Y-%m-%d") {
                use chrono::Datelike;
                let next_date = match event.recurrence.as_str() {
                    "daily" => Some(date + chrono::Duration::days(1)),
                    "weekly" => Some(date + chrono::Duration::weeks(1)),
                    "monthly" => {
                        let m = if date.month() == 12 { 1 } else { date.month() + 1 };
                        let y = if date.month() == 12 { date.year() + 1 } else { date.year() };
                        chrono::NaiveDate::from_ymd_opt(y, m, date.day().min(28))
                    }
                    _ => None,
                };

                if let Some(next) = next_date {
                    let next_str = next.format("%Y-%m-%d").to_string();
                    // Verificar que no pase del fin de recurrencia
                    let within_range = event.recurrence_end.as_ref()
                        .map(|end| next_str.as_str() <= end.as_str())
                        .unwrap_or(true);

                    if within_range {
                        // Actualizar fecha del evento para la proxima ocurrencia
                        event.date = next_str;
                        event.reminded = false;
                        event.accepted.clear();
                        event.declined.clear();
                    }
                }
            }
        }
        config.tasks.events.extend(new_events);
        let _ = save_config(&config).await;
        drop(config);

        // Send all reminders
        for (msg, ids) in &to_remind {
            for id in ids {
                let _ = send_telegram_message(&state.http_client, &token, *id, msg).await;
            }
        }
    }
}

// =====================
// Comandos de impresoras 3D
// =====================

async fn handle_printer_temps(state: &AppState) -> String {
    let config = state.config.lock().await;
    let printers = config.printers3d.clone();
    drop(config);

    if printers.is_empty() {
        return "*Temperaturas*\n\nNo hay impresoras configuradas.".to_string();
    }

    let mut msg = String::from("*Temperaturas*\n");

    for printer in &printers {
        let status = super::printers3d::printer_status(
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
fn progress_bar_temp(actual: f64, target: f64) -> String {
    if target <= 0.0 {
        return "[░░░░░░░░░░]".to_string();
    }
    let pct = (actual / target * 100.0).min(100.0);
    let filled = (pct / 10.0).round() as usize;
    let empty = 10_usize.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

async fn handle_printer_control(state: &AppState, user: &str, text: &str, command: &str) -> String {
    let config = state.config.lock().await;
    let printers = config.printers3d.clone();
    drop(config);

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

async fn handle_camera(state: &AppState, token: &str, chat_id: i64, text: &str) {
    let arg = text.strip_prefix("/camara").or_else(|| text.strip_prefix("/foto")).unwrap_or("").trim();

    let config = state.config.lock().await;
    let printers = config.printers3d.clone();
    drop(config);

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

async fn send_printer_photo(state: &AppState, token: &str, chat_id: i64, printer: &crate::models::printers3d::Printer3DConfig) {
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
    let status = super::printers3d::printer_status(
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

fn build_help_message(role: &UserRole) -> String {
    let mut msg = String::from("*LabNAS - Comandos*\n\n");
    msg.push_str("*Sistema*\n");
    msg.push_str("/estado /discos /ram /cpu\n");
    msg.push_str("/uptime /red /impresoras\n");
    msg.push_str("/actividad /horario /mirol\n");
    msg.push_str("/vincular CODIGO - Vincular con web\n");
    msg.push_str("/cmd COMANDO - Ejecutar en terminal\n\n");
    msg.push_str("*Impresoras 3D*\n");
    msg.push_str("/temp - Temperaturas\n");
    msg.push_str("/camara - Foto de la impresora\n");
    msg.push_str("/imprimir NOMBRE - Iniciar impresion\n");
    msg.push_str("/pausar NOMBRE - Pausar\n");
    msg.push_str("/cancelar3d NOMBRE - Cancelar\n\n");
    msg.push_str("*Correo*\n");
    msg.push_str("/correos - Resumen de bandeja\n");
    msg.push_str("/leer UID - Detalle de un correo\n");
    msg.push_str("/correo2tarea UID - Email a tarea\n\n");
    msg.push_str("*Calendario*\n");
    msg.push_str("/evento FECHA HORA Titulo @persona\n");
    msg.push_str("/eventos - Mis eventos\n");
    msg.push_str("/aceptar ID - Aceptar invitacion\n");
    msg.push_str("/declinar ID - Rechazar invitacion\n\n");
    msg.push_str("*Tareas y Proyectos*\n");
    msg.push_str("/tarea Titulo @persona - Crear tarea\n");
    msg.push_str("/tarea Titulo @all !confirmar - Confirmar\n");
    msg.push_str("/tarea Titulo !insistente - Cada 8min\n");
    msg.push_str("/tarea Titulo !cada5 - Cada 5min\n");
    msg.push_str("/tareas - Mis tareas pendientes\n");
    msg.push_str("/confirmar ID - Confirmar tarea\n");
    msg.push_str("/rechazar ID - Rechazar tarea\n");
    msg.push_str("/hecho ID - Marcar completada\n");
    msg.push_str("/proyecto Nombre - Crear proyecto\n");
    msg.push_str("/proyectos - Ver proyectos\n");
    msg.push_str("/avance Proyecto - Progreso\n\n");
    msg.push_str("*Musica*\n");
    msg.push_str("/musica - Que esta sonando\n");
    msg.push_str("/play BUSQUEDA - Buscar y reproducir\n");
    msg.push_str("/next - Siguiente cancion\n");
    msg.push_str("/stop - Detener musica\n");
    msg.push_str("/pause - Pausar/reanudar\n");
    msg.push_str("/mix - Llenar cola con recomendaciones\n");
    msg.push_str("/vol 0-100 - Ajustar volumen\n");

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
            crate::models::printers3d::Printer3DType::CrealityStock => "Creality",
            crate::models::printers3d::Printer3DType::FlashForge => "FlashForge",
        };
        msg.push_str(&format!("\n`{}` - {} ({}:{})", p.name, ptype, p.ip, p.port));
    }
    msg
}

// =====================
// Music commands (Telegram)
// =====================

async fn handle_music_status(state: &AppState) -> String {
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

async fn handle_music_play(state: &AppState, username: &str, query: &str) -> String {
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

async fn handle_music_next(state: &AppState) -> String {
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

async fn handle_music_stop(state: &AppState) -> String {
    crate::handlers::music::kill_player_pub(state).await;
    let mut ms = state.music.lock().await;
    let history = ms.history.clone();
    let vol = ms.volume;
    *ms = crate::handlers::music::MusicState::default();
    ms.history = history;
    ms.volume = vol;
    "Musica detenida.".to_string()
}

async fn handle_music_pause(state: &AppState) -> String {
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

async fn handle_music_mix(state: &AppState, username: &str) -> String {
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

async fn handle_music_volume(state: &AppState, vol_str: &str) -> String {
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
