//! Comandos de eventos de calendario

use super::*;

// =====================
// Calendar events (Telegram)
// =====================

pub(super) async fn handle_event_command(state: &AppState, creator: &str, text: &str) -> String {
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

    let id = uuid::Uuid::new_v4().to_string()[..6].to_string();
    let invitees_json = serde_json::to_string(&invitees).unwrap_or_else(|_| "[]".to_string());
    let created_at = chrono::Utc::now().to_rfc3339();

    let id_clone = id.clone();
    let title_clone = title.clone();
    let date_clone = date.clone();
    let time_clone = time.clone();
    let creator_str = creator.to_string();
    let invitees_json_clone = invitees_json.clone();

    let insert_result = crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO calendar_events (id, title, description, date, time, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, created_at) VALUES (?1, ?2, '', ?3, ?4, ?5, ?6, '[]', '[]', 15, 0, 1, '', ?7)",
            params![id_clone, title_clone, date_clone, time_clone, creator_str, invitees_json_clone, created_at],
        )
        .map_err(|e| format!("DB: {}", e))?;
        Ok(())
    })
    .await;

    if let Err(e) = insert_result {
        return format!("Error creando evento: {}", e.1);
    }

    // Notify invitees - read token and chats from DB
    let (token, chats) = {
        let conn = match crate::db::get_conn(&state.db) {
            Ok(c) => c,
            Err(_) => return format!("📅 Evento *{}* creado\n{} {}\nID: `{}`", title, date, time, id),
        };
        let token = read_bot_token(&conn).unwrap_or(None);
        let chats = read_all_chats(&conn).unwrap_or_default();
        (token, chats)
    };

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

pub(super) async fn handle_list_events(state: &AppState, user: &str) -> String {
    let user_str = user.to_string();
    let result = crate::db::db_op(&state.db, move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, date, time, created_by, invitees, accepted, declined FROM calendar_events"
        ).map_err(|e| format!("DB: {}", e))?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        }).map_err(|e| format!("DB: {}", e))?;

        let mut events = Vec::new();
        for r in rows {
            let (id, title, date, time, created_by, invitees_json, accepted_json, declined_json) = r.map_err(|e| format!("DB row: {}", e))?;
            let invitees: Vec<String> = serde_json::from_str(&invitees_json).unwrap_or_default();
            let accepted: Vec<String> = serde_json::from_str(&accepted_json).unwrap_or_default();
            let declined: Vec<String> = serde_json::from_str(&declined_json).unwrap_or_default();

            let is_relevant = created_by == user_str
                || invitees.contains(&"all".to_string())
                || invitees.iter().any(|i| i.to_lowercase() == user_str.to_lowercase());

            if is_relevant {
                events.push((id, title, date, time, created_by, accepted, declined));
            }
        }
        Ok(events)
    }).await;

    let events = match result {
        Ok(e) => e,
        Err(e) => return format!("Error: {}", e.1),
    };

    if events.is_empty() {
        return "📅 *Mis eventos*\n\nNo tienes eventos. Crea uno con `/evento`".to_string();
    }

    let mut msg = format!("📅 *Mis eventos* ({})\n", events.len());
    for (id, title, date, time, created_by, accepted, declined) in &events {
        let status = if accepted.iter().any(|a| a.to_lowercase() == user.to_lowercase()) {
            "aceptado"
        } else if declined.iter().any(|d| d.to_lowercase() == user.to_lowercase()) {
            "rechazado"
        } else if created_by == user {
            "creador"
        } else {
            "pendiente"
        };
        msg.push_str(&format!("\n`{}` *{}*\n  {} {} | {}\n", id, title, date, time, status));
    }
    msg
}

pub(super) async fn handle_event_rsvp(state: &AppState, user: &str, text: &str, accept: bool) -> String {
    let cmd = if accept { "/aceptar " } else { "/declinar " };
    let id = text.strip_prefix(cmd).unwrap_or("").trim();
    if id.is_empty() {
        return format!("Uso: `{}<ID>`", cmd);
    }

    let id_str = id.to_string();
    let user_str = user.to_string();

    let result = crate::db::db_op(&state.db, move |conn| {
        // Read current event
        let row = conn.query_row(
            "SELECT title, accepted, declined FROM calendar_events WHERE id = ?1",
            params![id_str],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        ).optional().map_err(|e| format!("DB: {}", e))?;

        let Some((title, accepted_json, declined_json)) = row else {
            return Ok(format!("Evento `{}` no encontrado.", id_str));
        };

        let mut accepted: Vec<String> = serde_json::from_str(&accepted_json).unwrap_or_default();
        let mut declined: Vec<String> = serde_json::from_str(&declined_json).unwrap_or_default();

        if accept {
            declined.retain(|u| u != &user_str);
            if !accepted.contains(&user_str) {
                accepted.push(user_str.clone());
            }
        } else {
            accepted.retain(|u| u != &user_str);
            if !declined.contains(&user_str) {
                declined.push(user_str.clone());
            }
        }

        let accepted_json = serde_json::to_string(&accepted).unwrap_or_else(|_| "[]".to_string());
        let declined_json = serde_json::to_string(&declined).unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            "UPDATE calendar_events SET accepted = ?1, declined = ?2 WHERE id = ?3",
            params![accepted_json, declined_json, id_str],
        ).map_err(|e| format!("DB: {}", e))?;

        if accept {
            Ok(format!("Evento *{}* aceptado.", title))
        } else {
            Ok(format!("Evento *{}* rechazado.", title))
        }
    }).await;

    match result {
        Ok(msg) => msg,
        Err(e) => format!("Error: {}", e.1),
    }
}
