//! Loop del bot: despacho de mensajes, registro, vinculacion, horario y ayuda

use super::*;
use crate::roles::{Grants, Level};

// =====================
// Bot polling loop
// =====================

pub async fn telegram_bot_loop(state: AppState) {
    let mut offset: i64 = 0;
    let mut startup_sent = false;

    loop {
        // Read bot_token and chats from DB (fresh each iteration)
        let (token, chats) = match crate::db::bg(&state.db, read_token_and_chats).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[Telegram] Error DB: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
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
        let Ok(role) = crate::db::bg(&state.db, move |conn| {
            read_chat(conn, chat_id)
                .ok()
                .flatten()
                .map(|c| c.role)
                .unwrap_or_default()
        })
        .await
        else {
            return;
        };

        let response = if role.is_admin() {
            "Hola! Eres el *administrador* de LabNAS.\n\nUsa /ayuda para ver los comandos.".to_string()
        } else if role.is_pending() {
            "Solicitud enviada. El administrador debe aprobar tu acceso desde la web.".to_string()
        } else {
            "Hola! Ya estas registrado en LabNAS.\n\nUsa /ayuda para ver los comandos.".to_string()
        };
        let _ = send_telegram_message(&state.http_client, token, chat_id, &response).await;
        return;
    }

    // Check user role
    let Ok(chat) = crate::db::bg(&state.db, move |conn| read_chat(conn, chat_id).ok().flatten()).await else {
        return;
    };

    let Some(chat) = chat else {
        let _ = send_telegram_message(&state.http_client, token, chat_id, "No estas registrado. Envia /start primero.").await;
        return;
    };

    if chat.role.is_pending() {
        let _ = send_telegram_message(&state.http_client, token, chat_id, "Tu acceso esta pendiente de aprobacion.").await;
        return;
    }

    let chat_name = chat.name.clone();
    // Permisos: los de la cuenta web vinculada (rol + excepciones) o, sin cuenta, los del rol del chat
    let c = chat.clone();
    let Ok((grants, expired, role_name)) = crate::db::bg(&state.db, move |conn| chat_grants(conn, &c)).await else {
        return;
    };
    if expired {
        let _ = send_telegram_message(&state.http_client, token, chat_id, "Tu cuenta llego a su fecha de termino.").await;
        return;
    }
    let has_terminal = grants.can("terminal", Level::Usar);
    let has_printer3d = grants.can("printers3d", Level::Gestionar);

    // Un modulo apagado lo esta tambien por Telegram (no solo en la web)
    let enabled = state.enabled_modules.lock().await.clone();
    if let Some(off) = command_modules(text).iter().find(|m| !enabled.contains(**m)) {
        let msg = format!("El modulo *{}* esta desactivado.", module_label(off));
        let _ = send_telegram_message(&state.http_client, token, chat_id, &msg).await;
        return;
    }
    let need = command_level(text);
    if let Some(m) = command_modules(text).iter().find(|m| !grants.can(m, need)) {
        let msg = format!("Tu rol no permite esto en *{}*.", module_label(m));
        let _ = send_telegram_message(&state.http_client, token, chat_id, &msg).await;
        return;
    }
    // Lo que el rol ve (para /ayuda)
    let modules: std::collections::HashSet<String> =
        enabled.into_iter().filter(|m| grants.can(m, Level::Ver)).collect();

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
        "/propuestas" => crate::handlers::email::telegram_list_proposals(state, &chat_name).await,
        s if s.starts_with("/aprobar ") => {
            let id = s.strip_prefix("/aprobar ").unwrap_or("").trim();
            crate::handlers::email::telegram_approve_proposal(state, &chat_name, id).await
        }
        s if s.starts_with("/descartar ") => {
            let id = s.strip_prefix("/descartar ").unwrap_or("").trim();
            crate::handlers::email::telegram_dismiss_proposal(state, &chat_name, id).await
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
            if !has_printer3d { "Sin permisos: hace falta gestionar Impresoras 3D.".to_string() }
            else { handle_printer_control(state, &chat_name, s, "start").await }
        }
        s if s.starts_with("/pausar") => {
            if !has_printer3d { "Sin permisos: hace falta gestionar Impresoras 3D.".to_string() }
            else { handle_printer_control(state, &chat_name, s, "pause").await }
        }
        s if s.starts_with("/cancelar3d") => {
            if !has_printer3d { "Sin permisos: hace falta gestionar Impresoras 3D.".to_string() }
            else { handle_printer_control(state, &chat_name, s, "cancel").await }
        }
        s if s.starts_with("/impresoras") => build_printers_message(state).await,
        s if s.starts_with("/mirol") => {
            let emoji = if grants.admin { "👑" } else { "👤" };
            format!("{} Tu rol: *{}*", emoji, role_name)
        }
        s if s.starts_with("/ayuda") | s.starts_with("/help") => build_help_message(&role_name, has_terminal, has_printer3d, &modules),
        _ => {
            // Check if user has active terminal session - pipe input
            if has_terminal && modules.contains("terminal") {
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
    let state_now = crate::db::bg(&state.db, move |conn| {
        let existing = read_chat(conn, chat_id).unwrap_or(None);
        let all_chats = read_all_chats(conn).unwrap_or_default();
        (existing, all_chats)
    })
    .await;
    let (existing, all_chats) = match state_now {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[Telegram] Error DB en register_chat: {}", e);
            return;
        }
    };

    if existing.is_some() {
        // Update existing chat name/username
        let _ = crate::db::bg(&state.db, move |conn| {
            conn.execute(
                "UPDATE telegram_chats SET name = ?1, username = ?2 WHERE chat_id = ?3",
                params![name, username, chat_id],
            )
        })
        .await;
        return;
    }

    // First user ever = admin, rest = pendiente
    let is_first = all_chats.is_empty();
    let role_str = if is_first { UserRole::admin() } else { UserRole::pending() }.0;

    // Notify admins about new pending user
    if !is_first {
        let admins: Vec<i64> = all_chats
            .iter()
            .filter(|c| c.role.is_admin())
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
    let _ = crate::db::bg(&state.db, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO telegram_chats (chat_id, name, username, role, linked_web_user, daily_enabled, daily_hour, daily_minute) VALUES (?1, ?2, ?3, ?4, NULL, 0, 8, 0)",
            params![chat_id, name, username, role_str],
        )
    })
    .await;
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

        // El chat toma el rol de la cuenta web
        let role_str: String = conn
            .query_row(
                "SELECT role FROM web_users WHERE username = ?1",
                params![username_clone],
                |row| row.get(0),
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
            "UPDATE telegram_chats SET linked_web_user = ?1, role = ?2 WHERE chat_id = ?3",
            params![username_clone, role_str, chat_id],
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


/// Prefijos de comandos y los modulos que necesitan. Cubren al menos lo que
/// acepta el `match` de `handle_message` (mismos prefijos o mas amplios).
const COMMAND_MODULES: &[(&str, &[&str])] = &[
    ("/cmd ", &["terminal"]),
    ("/evento", &["tasks"]),
    ("/aceptar ", &["tasks"]),
    ("/declinar ", &["tasks"]),
    ("/proyecto", &["tasks"]),
    ("/tarea", &["tasks"]),
    ("/avance", &["tasks"]),
    ("/confirmar ", &["tasks"]),
    ("/rechazar ", &["tasks"]),
    ("/hecho ", &["tasks"]),
    ("/correo2tarea ", &["email", "tasks"]),
    ("/aprobar ", &["email", "tasks"]),
    ("/leer ", &["email"]),
    ("/propuestas", &["email"]),
    ("/descartar ", &["email"]),
    ("/correos", &["email"]),
    ("/musica", &["music"]),
    ("/music", &["music"]),
    ("/play ", &["music"]),
    ("/next", &["music"]),
    ("/siguiente", &["music"]),
    ("/stop", &["music"]),
    ("/parar", &["music"]),
    ("/pause", &["music"]),
    ("/mix", &["music"]),
    ("/vol ", &["music"]),
    ("/pausar", &["printers3d"]),
    ("/camara", &["printers3d"]),
    ("/foto", &["printers3d"]),
    ("/temp", &["printers3d"]),
    ("/imprimir ", &["printers3d"]),
    ("/cancelar3d", &["printers3d"]),
    ("/impresoras", &["printers3d"]),
    ("/red", &["network"]),
];

/// Modulos que necesita un comando (vacio = siempre disponible)
pub(super) fn command_modules(text: &str) -> &'static [&'static str] {
    // "/pausar" a secas pausa la musica; con argumento, una impresora 3D
    if text == "/pausar" {
        return &["music"];
    }
    COMMAND_MODULES.iter().find(|(p, _)| text.starts_with(p)).map(|(_, m)| *m).unwrap_or(&[])
}

/// Comandos de solo consulta: basta con Ver el modulo; el resto pide Usar
/// (los de control de impresoras 3D y la terminal se revisan aparte)
fn command_level(text: &str) -> Level {
    const READ_ONLY: &[&str] = &[
        "/eventos", "/proyectos", "/tareas", "/avance", "/leer ", "/propuestas", "/correos",
        "/musica", "/music", "/temp", "/camara", "/foto", "/impresoras", "/red",
    ];
    if READ_ONLY.iter().any(|p| text.starts_with(p)) { Level::Ver } else { Level::Usar }
}

/// Permisos de un chat: (permisos, cuenta vencida, nombre del rol)
fn chat_grants(conn: &rusqlite::Connection, chat: &TelegramChat) -> (Grants, bool, String) {
    if let Some((role, grants, expires)) = chat.linked_web_user.as_deref().and_then(|u| crate::roles::load_user(conn, u)) {
        let name = crate::handlers::roles::role_name(conn, &role);
        return (grants, crate::roles::is_expired(expires), name);
    }
    let role = chat.role.as_str();
    (crate::roles::grants_for_role(conn, role), false, crate::handlers::roles::role_name(conn, role))
}

fn module_label(id: &str) -> &str {
    match id {
        "terminal" => "Terminal",
        "tasks" => "Tareas / Horario",
        "email" => "Correo",
        "music" => "Musica",
        "printers3d" => "Impresoras 3D",
        "network" => "Red",
        other => other,
    }
}

pub(super) fn build_help_message(
    role_name: &str,
    has_terminal: bool,
    has_printer3d: bool,
    modules: &std::collections::HashSet<String>,
) -> String {
    let on = |m: &str| modules.contains(m);
    let mut msg = String::from("*LabNAS - Comandos*\n\n");
    msg.push_str("*Sistema*\n");
    msg.push_str("/estado /discos /ram /cpu /uptime\n");
    if on("network") {
        msg.push_str("/red\n");
    }
    msg.push_str("/actividad /horario /mirol\n");
    msg.push_str("/vincular CODIGO - Vincular con web\n");
    if has_terminal && on("terminal") {
        msg.push_str("/cmd COMANDO - Ejecutar en terminal\n");
    }
    msg.push('\n');
    if on("printers3d") {
        msg.push_str("*Impresoras 3D*\n");
        msg.push_str("/impresoras - Estado\n");
        msg.push_str("/temp - Temperaturas\n");
        msg.push_str("/camara - Foto de la impresora\n");
        if has_printer3d {
            msg.push_str("/imprimir NOMBRE - Iniciar impresion\n");
            msg.push_str("/pausar NOMBRE - Pausar\n");
            msg.push_str("/cancelar3d NOMBRE - Cancelar\n");
        }
        msg.push('\n');
    }
    if on("email") {
        msg.push_str("*Correo*\n");
        msg.push_str("/correos - Resumen de bandeja\n");
        msg.push_str("/leer UID - Detalle de un correo\n");
        msg.push_str("/propuestas - Eventos y tareas que propone la IA\n");
        msg.push_str("/descartar ID - Descartar una propuesta\n");
        if on("tasks") {
            msg.push_str("/correo2tarea UID - Email a tarea\n");
            msg.push_str("/aprobar ID - Agendar una propuesta\n");
        }
        msg.push('\n');
    }
    if on("tasks") {
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
    }
    if on("music") {
        msg.push_str("*Musica*\n");
        msg.push_str("/musica - Que esta sonando\n");
        msg.push_str("/play BUSQUEDA - Buscar y reproducir\n");
        msg.push_str("/next - Siguiente cancion\n");
        msg.push_str("/stop - Detener musica\n");
        msg.push_str("/pause - Pausar/reanudar\n");
        msg.push_str("/mix - Llenar cola con recomendaciones\n");
        msg.push_str("/vol 0-100 - Ajustar volumen\n");
    }

    msg.push_str(&format!("\nTu rol: *{}*", role_name));
    msg
}

pub(super) async fn handle_schedule_command(state: &AppState, chat_id: i64, text: &str) -> String {
    let arg = text.strip_prefix("/horario").unwrap_or("").trim();

    if arg.is_empty() {
        // Show current schedule
        let chat = match crate::db::bg(&state.db, move |conn| read_chat(conn, chat_id).unwrap_or(None)).await {
            Ok(c) => c,
            Err(_) => return "Error de base de datos.".to_string(),
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
        let updated = match crate::db::bg(&state.db, move |conn| {
            conn.execute(
                "UPDATE telegram_chats SET daily_enabled = 0 WHERE chat_id = ?1",
                params![chat_id],
            )
            .unwrap_or(0)
        })
        .await
        {
            Ok(n) => n,
            Err(_) => return "Error de base de datos.".to_string(),
        };
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

    let updated = match crate::db::bg(&state.db, move |conn| {
        conn.execute(
            "UPDATE telegram_chats SET daily_enabled = 1, daily_hour = ?1, daily_minute = ?2 WHERE chat_id = ?3",
            params![hour, minute, chat_id],
        )
        .unwrap_or(0)
    })
    .await
    {
        Ok(n) => n,
        Err(_) => return "Error de base de datos.".to_string(),
    };

    if updated > 0 {
        format!("Reporte diario activado a las *{:02}:{:02}*", hour, minute)
    } else {
        "No estas registrado. Envia /start primero.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{build_help_message, command_modules};

    #[test]
    fn comandos_ligados_a_su_modulo() {
        assert_eq!(command_modules("/cmd ls"), &["terminal"]);
        assert_eq!(command_modules("/tareas"), &["tasks"]);
        assert_eq!(command_modules("/eventos"), &["tasks"]);
        assert_eq!(command_modules("/aprobar 3"), &["email", "tasks"]);
        assert_eq!(command_modules("/propuestas"), &["email"]);
        assert_eq!(command_modules("/pausar"), &["music"]);
        assert_eq!(command_modules("/pausar Prusa"), &["printers3d"]);
        assert_eq!(command_modules("/pausar3d"), &["printers3d"]);
        assert_eq!(command_modules("/camara 1"), &["printers3d"]);
        assert_eq!(command_modules("/red"), &["network"]);
        assert!(command_modules("/estado").is_empty());
        assert!(command_modules("/kill").is_empty());
        assert!(command_modules("/ayuda").is_empty());
    }

    #[test]
    fn ayuda_oculta_modulos_apagados() {
        let mods: std::collections::HashSet<String> = ["tasks".to_string()].into();
        let help = build_help_message("Administrador", true, true, &mods);
        assert!(help.contains("/tareas"));
        assert!(!help.contains("/cmd"));
        assert!(!help.contains("/propuestas"));
        assert!(!help.contains("/imprimir"));
        assert!(!help.contains("/play"));
    }
}
