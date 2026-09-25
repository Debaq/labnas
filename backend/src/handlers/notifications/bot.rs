//! Loop del bot: despacho de mensajes, registro, vinculacion, horario y ayuda

use super::*;

// =====================
// Bot polling loop
// =====================

pub async fn telegram_bot_loop(state: AppState) {
    let mut offset: i64 = 0;
    let mut startup_sent = false;

    loop {
        // Read bot_token and chats from DB (fresh each iteration)
        let (token, chats) = {
            let conn = match crate::db::get_conn(&state.db) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[Telegram] Error DB: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            let token = read_bot_token(&conn).unwrap_or(None);
            let chats = read_all_chats(&conn).unwrap_or_default();
            (token, chats)
        };

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

pub(super) async fn handle_message(state: &AppState, token: &str, msg: &TgMessage) {
    let chat_id = msg.chat.id;
    let text = msg.text.as_deref().unwrap_or("").trim();

    // Handle /start separately (always allowed)
    if text.starts_with("/start") {
        register_chat(state, token, msg).await;
        // Check if they're pending
        let role = {
            let conn = match crate::db::get_conn(&state.db) {
                Ok(c) => c,
                Err(_) => return,
            };
            read_chat(&conn, chat_id)
                .ok()
                .flatten()
                .map(|c| c.role)
                .unwrap_or(UserRole::Pendiente)
        };

        let response = match role {
            UserRole::Admin => "Hola! Eres el *administrador* de LabNAS.\n\nUsa /ayuda para ver los comandos.".to_string(),
            UserRole::Pendiente => "Solicitud enviada. El administrador debe aprobar tu acceso desde la web.".to_string(),
            _ => "Hola! Ya estas registrado en LabNAS.\n\nUsa /ayuda para ver los comandos.".to_string(),
        };
        let _ = send_telegram_message(&state.http_client, token, chat_id, &response).await;
        return;
    }

    // Check user role
    let chat = {
        let conn = match crate::db::get_conn(&state.db) {
            Ok(c) => c,
            Err(_) => return,
        };
        read_chat(&conn, chat_id).ok().flatten()
    };

    let Some(chat) = chat else {
        let _ = send_telegram_message(&state.http_client, token, chat_id, "No estas registrado. Envia /start primero.").await;
        return;
    };

    if chat.role == UserRole::Pendiente {
        let _ = send_telegram_message(&state.http_client, token, chat_id, "Tu acceso esta pendiente de aprobacion.").await;
        return;
    }

    let role = chat.role.clone();
    let chat_name = chat.name.clone();
    let is_admin = chat.role == UserRole::Admin;
    let is_operator = chat.role == UserRole::Operador;
    let has_terminal = is_admin || chat.permissions.terminal;
    let has_printer3d = is_admin || is_operator;

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
            handle_camera(state, token, chat_id, s).await;
            return; // ya envió la foto directamente
        }
        s if s.starts_with("/temp") => handle_printer_temps(state).await,
        s if s.starts_with("/imprimir ") => {
            if !has_printer3d { "Sin permisos. Se requiere rol Operador o Admin.".to_string() }
            else { handle_printer_control(state, &chat_name, s, "start").await }
        }
        s if s.starts_with("/pausar") => {
            if !has_printer3d { "Sin permisos. Se requiere rol Operador o Admin.".to_string() }
            else { handle_printer_control(state, &chat_name, s, "pause").await }
        }
        s if s.starts_with("/cancelar3d") => {
            if !has_printer3d { "Sin permisos. Se requiere rol Operador o Admin.".to_string() }
            else { handle_printer_control(state, &chat_name, s, "cancel").await }
        }
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
        s if s.starts_with("/ayuda") | s.starts_with("/help") => build_help_message(&role, has_terminal, has_printer3d),
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

pub(super) async fn register_chat(state: &AppState, token: &str, msg: &TgMessage) {
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

    // Read current state from DB
    let conn = match crate::db::get_conn(&state.db) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[Telegram] Error DB en register_chat: {}", e);
            return;
        }
    };

    let existing = read_chat(&conn, chat_id).unwrap_or(None);
    let all_chats = read_all_chats(&conn).unwrap_or_default();
    drop(conn);

    if let Some(_existing) = existing {
        // Update existing chat name/username
        let conn = match crate::db::get_conn(&state.db) {
            Ok(c) => c,
            Err(_) => return,
        };
        let _ = conn.execute(
            "UPDATE telegram_chats SET name = ?1, username = ?2 WHERE chat_id = ?3",
            params![name, username, chat_id],
        );
        return;
    }

    // First user ever = admin, rest = pendiente
    let is_first = all_chats.is_empty();
    let role = if is_first {
        UserRole::Admin
    } else {
        UserRole::Pendiente
    };
    let perms = if is_first {
        UserPermissions {
            terminal: true,
            impresion: true,
            archivos_escritura: true,
        }
    } else {
        UserPermissions::default()
    };
    let role_str = role_to_str(&role).to_string();

    // Notify admins about new pending user
    if !is_first {
        let admins: Vec<i64> = all_chats
            .iter()
            .filter(|c| c.role == UserRole::Admin)
            .map(|c| c.chat_id)
            .collect();
        let uname = username.as_deref().map(|u| format!(" (@{})", u)).unwrap_or_default();
        let alert = format!(
            "Nuevo usuario solicita acceso:\n*{}*{}\n\nApruebalo desde la web en Configuracion > Telegram.",
            name, uname
        );
        for admin_id in &admins {
            let _ = send_telegram_message(&state.http_client, token, *admin_id, &alert).await;
        }
    }

    // Insert new chat
    let conn = match crate::db::get_conn(&state.db) {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = conn.execute(
        "INSERT OR REPLACE INTO telegram_chats (chat_id, name, username, role, perm_terminal, perm_impresion, perm_archivos_escritura, linked_web_user, daily_enabled, daily_hour, daily_minute) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 0, 8, 0)",
        params![chat_id, name, username, role_str, perms.terminal, perms.impresion, perms.archivos_escritura],
    );
}


// =====================
// Account linking
// =====================

pub(super) async fn handle_link_command(state: &AppState, chat_id: i64, _chat_name: &str, text: &str) -> String {
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
    let username_clone = username.clone();

    let result = crate::db::db_op(&state.db, move |conn| {
        // Check web user exists
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM web_users WHERE username = ?1",
                params![username_clone],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| format!("DB: {}", e))?
            > 0;

        if !exists {
            return Err("Usuario web no encontrado.".to_string());
        }

        // Get web user's role and permissions
        let (role_str, perm_t, perm_i, perm_a): (String, bool, bool, bool) = conn
            .query_row(
                "SELECT role, perm_terminal, perm_impresion, perm_archivos_escritura FROM web_users WHERE username = ?1",
                params![username_clone],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|e| format!("DB: {}", e))?;

        // Update web user -> telegram link
        conn.execute(
            "UPDATE web_users SET linked_telegram = ?1 WHERE username = ?2",
            params![chat_id, username_clone],
        )
        .map_err(|e| format!("DB: {}", e))?;

        // Update telegram -> web user link + sync role/perms
        conn.execute(
            "UPDATE telegram_chats SET linked_web_user = ?1, role = ?2, perm_terminal = ?3, perm_impresion = ?4, perm_archivos_escritura = ?5 WHERE chat_id = ?6",
            params![username_clone, role_str, perm_t, perm_i, perm_a, chat_id],
        )
        .map_err(|e| format!("DB: {}", e))?;

        Ok(())
    })
    .await;

    match result {
        Ok(()) => format!("Cuenta vinculada! Tu Telegram esta conectado con *{}*.", username),
        Err(e) => e.1,
    }
}


pub(super) fn build_help_message(role: &UserRole, has_terminal: bool, has_printer3d: bool) -> String {
    let mut msg = String::from("*LabNAS - Comandos*\n\n");
    msg.push_str("*Sistema*\n");
    msg.push_str("/estado /discos /ram /cpu\n");
    msg.push_str("/uptime /red /impresoras\n");
    msg.push_str("/actividad /horario /mirol\n");
    msg.push_str("/vincular CODIGO - Vincular con web\n");
    if has_terminal {
        msg.push_str("/cmd COMANDO - Ejecutar en terminal\n");
    }
    msg.push('\n');
    msg.push_str("*Impresoras 3D*\n");
    msg.push_str("/temp - Temperaturas\n");
    msg.push_str("/camara - Foto de la impresora\n");
    if has_printer3d {
        msg.push_str("/imprimir NOMBRE - Iniciar impresion\n");
        msg.push_str("/pausar NOMBRE - Pausar\n");
        msg.push_str("/cancelar3d NOMBRE - Cancelar\n");
    }
    msg.push('\n');
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

pub(super) async fn handle_schedule_command(state: &AppState, chat_id: i64, text: &str) -> String {
    let arg = text.strip_prefix("/horario").unwrap_or("").trim();

    if arg.is_empty() {
        // Show current schedule
        let chat = {
            let conn = match crate::db::get_conn(&state.db) {
                Ok(c) => c,
                Err(_) => return "Error de base de datos.".to_string(),
            };
            read_chat(&conn, chat_id).unwrap_or(None)
        };

        if let Some(chat) = chat {
            if chat.daily_enabled {
                return format!("Tu reporte diario esta a las *{:02}:{:02}*\n\nUsa `/horario HH:MM` para cambiar o `/horario off` para desactivar.", chat.daily_hour, chat.daily_minute);
            } else {
                return "Tu reporte diario esta *desactivado*.\n\nUsa `/horario HH:MM` para activar (ej: `/horario 08:00`).".to_string();
            }
        }
        return "No estas registrado. Envia /start primero.".to_string();
    }

    if arg == "off" {
        let conn = match crate::db::get_conn(&state.db) {
            Ok(c) => c,
            Err(_) => return "Error de base de datos.".to_string(),
        };
        let updated = conn.execute(
            "UPDATE telegram_chats SET daily_enabled = 0 WHERE chat_id = ?1",
            params![chat_id],
        ).unwrap_or(0);
        if updated > 0 {
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

    let conn = match crate::db::get_conn(&state.db) {
        Ok(c) => c,
        Err(_) => return "Error de base de datos.".to_string(),
    };
    let updated = conn.execute(
        "UPDATE telegram_chats SET daily_enabled = 1, daily_hour = ?1, daily_minute = ?2 WHERE chat_id = ?3",
        params![hour, minute, chat_id],
    ).unwrap_or(0);

    if updated > 0 {
        format!("Reporte diario activado a las *{:02}:{:02}*", hour, minute)
    } else {
        "No estas registrado. Envia /start primero.".to_string()
    }
}
