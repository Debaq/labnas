//! Resumen, detalle y correo a tarea desde Telegram

use super::*;

// =====================
// Telegram command helpers (publicas para notifications.rs)
// =====================

/// Resumen de correos para un usuario de Telegram
pub async fn get_emails_summary(state: &AppState, chat_name: &str) -> String {
    // Buscar el web user vinculado a este chat
    let name = chat_name.to_string();
    let linked_user = db_op(&state.db, move |conn| {
        let user: Option<String> = conn.query_row(
            "SELECT linked_web_user FROM telegram_chats WHERE name = ?1",
            params![&name],
            |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?.flatten();
        Ok(user)
    }).await.unwrap_or(None);

    let Some(username) = linked_user else {
        return "No tienes cuenta web vinculada. Usa `/vincular CODIGO` primero.".to_string();
    };

    let inbox = state.email_inbox.lock().await;
    let emails = match inbox.get(&username) {
        Some(e) if !e.is_empty() => e.clone(),
        _ => {
            return format!(
                "*Correos de {}*\n\nNo hay correos. Configura tu cuenta desde la web o usa `/correos check` para revisar.",
                username
            );
        }
    };
    drop(inbox);

    let urgent = emails
        .iter()
        .filter(|e| e.ai_classification.as_deref() == Some("urgente"))
        .count();
    let tasks = emails
        .iter()
        .filter(|e| e.ai_classification.as_deref() == Some("tarea"))
        .count();
    let info = emails
        .iter()
        .filter(|e| e.ai_classification.as_deref() == Some("informativo"))
        .count();
    let spam = emails
        .iter()
        .filter(|e| e.ai_classification.as_deref() == Some("spam"))
        .count();
    let unclassified = emails
        .iter()
        .filter(|e| e.ai_classification.is_none())
        .count();

    let mut msg = format!(
        "*Correos* ({} total)\n\nUrgentes: {}\nTareas: {}\nInformativos: {}\nSpam: {}\nSin clasificar: {}\n\n*Ultimos correos:*\n",
        emails.len(),
        urgent,
        tasks,
        info,
        spam,
        unclassified
    );

    // Mostrar los ultimos 10
    let start = if emails.len() > 10 {
        emails.len() - 10
    } else {
        0
    };
    for email in &emails[start..] {
        let classification = email
            .ai_classification
            .as_deref()
            .unwrap_or("?");
        let icon = match classification {
            "urgente" => "!",
            "tarea" => "T",
            "informativo" => "i",
            "spam" => "S",
            _ => "?",
        };
        let subject_short = if email.subject.len() > 40 {
            format!("{}...", &email.subject[..37])
        } else {
            email.subject.clone()
        };
        msg.push_str(&format!(
            "\n[{}] `{}` {}\n  De: {}",
            icon, email.uid, subject_short, email.from
        ));
        if let Some(ref summary) = email.ai_summary {
            let summary_short = if summary.len() > 60 {
                format!("{}...", &summary[..57])
            } else {
                summary.clone()
            };
            msg.push_str(&format!("\n  _{}_", summary_short));
        }
        msg.push('\n');
    }

    msg.push_str("\nUsa `/leer UID` para ver detalle o `/correo2tarea UID` para crear tarea.");
    msg
}

/// Leer detalle de un email por UID
pub async fn get_email_detail(state: &AppState, chat_name: &str, uid_str: &str) -> String {
    let name = chat_name.to_string();
    let linked_user = db_op(&state.db, move |conn| {
        let user: Option<String> = conn.query_row(
            "SELECT linked_web_user FROM telegram_chats WHERE name = ?1",
            params![&name],
            |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?.flatten();
        Ok(user)
    }).await.unwrap_or(None);

    let Some(username) = linked_user else {
        return "No tienes cuenta web vinculada. Usa `/vincular CODIGO` primero.".to_string();
    };

    let uid: u32 = match uid_str.trim().parse() {
        Ok(u) => u,
        Err(_) => return "UID invalido. Uso: `/leer 12345`".to_string(),
    };

    let inbox = state.email_inbox.lock().await;
    let emails = match inbox.get(&username) {
        Some(e) => e,
        None => return "No tienes correos.".to_string(),
    };

    let email = match emails.iter().find(|e| e.uid == uid) {
        Some(e) => e.clone(),
        None => return format!("Correo con UID {} no encontrado.", uid),
    };
    drop(inbox);

    let mut msg = format!("*Correo {}*\n\n", email.uid);
    msg.push_str(&format!("*De:* {}\n", email.from));
    msg.push_str(&format!("*Asunto:* {}\n", email.subject));
    msg.push_str(&format!("*Fecha:* {}\n", email.date));

    if let Some(ref classification) = email.ai_classification {
        msg.push_str(&format!("\n*Clasificacion:* {}\n", classification));
    }
    if let Some(ref summary) = email.ai_summary {
        msg.push_str(&format!("*Resumen IA:* {}\n", summary));
    }
    if let Some(ref action) = email.ai_action {
        msg.push_str(&format!("*Accion sugerida:* {}\n", action));
    }

    // Body preview (limitado para Telegram)
    let preview = if email.body_preview.len() > 1000 {
        format!("{}...", &email.body_preview[..997])
    } else {
        email.body_preview.clone()
    };
    msg.push_str(&format!("\n```\n{}\n```", preview));

    if !email.task_created {
        msg.push_str(&format!("\n`/correo2tarea {}`", email.uid));
    }

    msg
}

/// Convertir email a tarea insistente desde Telegram
pub async fn telegram_email_to_task(state: &AppState, chat_name: &str, uid_str: &str) -> String {
    let name = chat_name.to_string();
    let linked_user = db_op(&state.db, move |conn| {
        let user: Option<String> = conn.query_row(
            "SELECT linked_web_user FROM telegram_chats WHERE name = ?1",
            params![&name],
            |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?.flatten();
        Ok(user)
    }).await.unwrap_or(None);

    let Some(username) = linked_user else {
        return "No tienes cuenta web vinculada. Usa `/vincular CODIGO` primero.".to_string();
    };

    let uid: u32 = match uid_str.trim().parse() {
        Ok(u) => u,
        Err(_) => return "UID invalido. Uso: `/correo2tarea 12345`".to_string(),
    };

    let mut inbox = state.email_inbox.lock().await;
    let emails = match inbox.get_mut(&username) {
        Some(e) => e,
        None => return "No tienes correos.".to_string(),
    };

    let email = match emails.iter_mut().find(|e| e.uid == uid) {
        Some(e) => e,
        None => return format!("Correo con UID {} no encontrado.", uid),
    };

    if email.task_created {
        return "Ya se creo una tarea para este correo.".to_string();
    }

    let title = format!("[Email] {}", email.subject);
    let title = if title.len() > 200 {
        format!("{}...", &title[..197])
    } else {
        title
    };

    email.task_created = true;
    let from = email.from.clone();
    drop(inbox);

    // Crear tarea insistente
    let task_id = uuid::Uuid::new_v4().to_string()[..6].to_string();
    let description = format!("De: {}", from);
    let assigned_json = serde_json::to_string(&vec![chat_name]).unwrap_or_default();
    let now = Utc::now().to_rfc3339();
    let tid = task_id.clone();
    let t = title.clone();
    let d = description.clone();
    let cn = chat_name.to_string();
    let aj = assigned_json.clone();
    let n = now.clone();

    let insert_result = db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO tasks (id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                tid, Option::<String>::None, t, d,
                aj, "pendiente", cn, Option::<String>::None, Option::<String>::None,
                true, true, 8i64,
                "[]", "[]", n, Option::<String>::None,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await;

    if let Err(e) = insert_result {
        return format!("Error creando tarea: {}", e.1);
    }

    format!(
        "Tarea insistente creada!\n*{}*\nID: `{}`\n\nSe te recordara cada 8 min hasta confirmar.",
        title, task_id
    )
}
