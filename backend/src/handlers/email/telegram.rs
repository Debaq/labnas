//! Correo desde Telegram: resumen, detalle, correo a tarea y propuestas de la IA

use super::*;
use crate::textutil::ellipsize;

use super::store;

/// Usuario web vinculado a un chat de Telegram
async fn linked_user(state: &AppState, chat_name: &str) -> Option<String> {
    let name = chat_name.to_string();
    db_op(&state.db, move |conn| {
        let user: Option<String> = conn
            .query_row("SELECT linked_web_user FROM telegram_chats WHERE name = ?1", params![&name], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())?
            .flatten();
        Ok(user)
    })
    .await
    .unwrap_or(None)
}

const NOT_LINKED: &str = "No tienes cuenta web vinculada. Usa `/vincular CODIGO` primero.";

/// Resumen de correos para un usuario de Telegram
pub async fn get_emails_summary(state: &AppState, chat_name: &str) -> String {
    let Some(username) = linked_user(state, chat_name).await else {
        return NOT_LINKED.to_string();
    };

    let u = username.clone();
    let (emails, pending) = match db_op(&state.db, move |conn| {
        Ok((store::list_inbox(conn, &u)?, store::count_pending(conn, &u)?))
    })
    .await
    {
        Ok(r) => r,
        Err((_, e)) => return format!("Error leyendo correos: {}", e),
    };
    if emails.is_empty() {
        return format!(
            "*Correos de {}*\n\nNo hay correos. Configura tu cuenta desde la web; se revisa cada 5 minutos.",
            username
        );
    }

    let count = |c: &str| emails.iter().filter(|e| e.ai_classification.as_deref() == Some(c)).count();
    let mut msg = format!(
        "*Correos* ({} recientes)\n\nUrgentes: {}\nTareas: {}\nInformativos: {}\nSpam: {}\nSin clasificar: {}\n",
        emails.len(),
        count("urgente"),
        count("tarea"),
        count("informativo"),
        count("spam"),
        emails.iter().filter(|e| e.ai_classification.is_none()).count()
    );
    if pending > 0 {
        msg.push_str(&format!("Propuestas de la IA: {} (`/propuestas`)\n", pending));
    }
    msg.push_str("\n*Ultimos correos:*\n");

    for email in emails.iter().rev().take(10) {
        let icon = match email.ai_classification.as_deref().unwrap_or("?") {
            "urgente" => "!",
            "tarea" => "T",
            "informativo" => "i",
            "spam" => "S",
            _ => "?",
        };
        msg.push_str(&format!("\n[{}] `{}` {}\n  De: {}", icon, email.uid, ellipsize(&email.subject, 40), email.from));
        if let Some(ref summary) = email.ai_summary {
            msg.push_str(&format!("\n  _{}_", ellipsize(summary, 60)));
        }
        msg.push('\n');
    }

    msg.push_str("\nUsa `/leer UID` para ver detalle o `/correo2tarea UID` para crear tarea.");
    msg
}

/// Leer detalle de un email por UID
pub async fn get_email_detail(state: &AppState, chat_name: &str, uid_str: &str) -> String {
    let Some(username) = linked_user(state, chat_name).await else {
        return NOT_LINKED.to_string();
    };
    let uid: u32 = match uid_str.trim().parse() {
        Ok(u) => u,
        Err(_) => return "UID invalido. Uso: `/leer 12345`".to_string(),
    };

    let found = db_op(&state.db, move |conn| store::get_email(conn, &username, uid)).await;
    let (email, body) = match found {
        Ok(Some(e)) => e,
        Ok(None) => return format!("Correo con UID {} no encontrado.", uid),
        Err((_, e)) => return format!("Error leyendo el correo: {}", e),
    };

    let mut msg = format!("*Correo {}*\n\n", email.uid);
    msg.push_str(&format!("*De:* {}\n", email.from));
    msg.push_str(&format!("*Asunto:* {}\n", email.subject));
    msg.push_str(&format!("*Fecha:* {}\n", email.date));
    if let Some(ref c) = email.ai_classification {
        msg.push_str(&format!("\n*Clasificacion:* {}\n", c));
    }
    if let Some(ref s) = email.ai_summary {
        msg.push_str(&format!("*Resumen IA:* {}\n", s));
    }
    if let Some(ref a) = email.ai_action {
        msg.push_str(&format!("*Accion sugerida:* {}\n", a));
    }
    msg.push_str(&format!("\n```\n{}\n```", ellipsize(&body.replace("```", "'''"), 1000)));
    if !email.task_created {
        msg.push_str(&format!("\n`/correo2tarea {}`", email.uid));
    }
    msg
}

/// Convertir email a tarea insistente desde Telegram
pub async fn telegram_email_to_task(state: &AppState, chat_name: &str, uid_str: &str) -> String {
    let Some(username) = linked_user(state, chat_name).await else {
        return NOT_LINKED.to_string();
    };
    let uid: u32 = match uid_str.trim().parse() {
        Ok(u) => u,
        Err(_) => return "UID invalido. Uso: `/correo2tarea 12345`".to_string(),
    };
    match super::api::create_task_from_email(state, &username, chat_name, uid, true).await {
        Ok(title) => {
            state.log_activity("email_a_tarea", &title, &username).await;
            format!("Tarea insistente creada!\n*{}*\n\nSe te recordara cada 8 min hasta confirmar.", title)
        }
        Err((_, e)) => e,
    }
}

/// /propuestas: pendientes del usuario
pub async fn telegram_list_proposals(state: &AppState, chat_name: &str) -> String {
    let Some(username) = linked_user(state, chat_name).await else {
        return NOT_LINKED.to_string();
    };
    let list = match db_op(&state.db, move |conn| store::list_proposals(conn, &username, "pendiente")).await {
        Ok(l) => l,
        Err((_, e)) => return format!("Error: {}", e),
    };
    if list.is_empty() {
        return "No tienes propuestas pendientes.".to_string();
    }
    let mut msg = format!("*Propuestas pendientes* ({})\n", list.len());
    for p in list.iter().take(15) {
        let icon = if p.kind == "evento" { "📅" } else { "✅" };
        let when = match (&p.date, &p.time) {
            (Some(d), Some(t)) => format!(" — {} {}", d, t),
            (Some(d), None) => format!(" — {}", d),
            _ => String::new(),
        };
        msg.push_str(&format!("\n{} {}{}", icon, ellipsize(&p.title, 80), when));
        if let Some(loc) = &p.location {
            msg.push_str(&format!(" ({})", ellipsize(loc, 40)));
        }
        if let Some(s) = &p.email_subject {
            msg.push_str(&format!("\n  _De: {}_", ellipsize(s, 50)));
        }
        msg.push_str(&format!("\n  `/aprobar {}`  `/descartar {}`\n", p.id, p.id));
    }
    msg
}

/// /aprobar ID
pub async fn telegram_approve_proposal(state: &AppState, chat_name: &str, id: &str) -> String {
    let Some(username) = linked_user(state, chat_name).await else {
        return NOT_LINKED.to_string();
    };
    match super::proposals::approve(state, &username, id.trim(), Default::default()).await {
        Ok(a) => a.message,
        Err((_, e)) => e,
    }
}

/// /descartar ID
pub async fn telegram_dismiss_proposal(state: &AppState, chat_name: &str, id: &str) -> String {
    let Some(username) = linked_user(state, chat_name).await else {
        return NOT_LINKED.to_string();
    };
    match super::proposals::dismiss(state, &username, id.trim()).await {
        Ok(()) => "Propuesta descartada.".to_string(),
        Err((_, e)) => e,
    }
}
