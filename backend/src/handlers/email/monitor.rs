//! Revision de correo: descarga lo nuevo, aplica filtros, lee invitaciones .ics,
//! analiza con IA, guarda todo y avisa (web, Telegram).

use super::*;

use super::ai::{analyze_email, ProposedItem};
use super::fetch::{fetch_new, FetchedMail};
use super::store::{self, NewEmail};
use crate::events::{self, Audience, Level};

const CHECK_EVERY: Duration = Duration::from_secs(300);
const DEFAULT_RETENTION_DAYS: u32 = 90;

/// Resultado de revisar una cuenta
#[derive(Debug, Default)]
pub struct CheckResult {
    pub new_emails: usize,
    pub proposals: Vec<(String, ProposedItem)>,
    /// "- remitente [filtro]: asunto" de los urgentes nuevos que si notifican
    pub urgent: Vec<String>,
}

/// Revisa una cuenta y guarda lo nuevo. Usado por el loop y por "Revisar ahora".
pub async fn check_account(state: &AppState, account: &EmailAccount) -> Result<CheckResult, String> {
    let user = account.username.clone();
    let (known, ai_cfg) = db_op(&state.db, move |conn| {
        Ok((store::known_uids(conn, &user)?, crate::ai::AiConfig::load(conn)))
    })
    .await
    .map_err(|(_, e)| e)?;

    let fetched = fetch_new(account, &known).await?;
    let mut result = CheckResult::default();

    for mail in fetched {
        let (msg, proposals) = process_mail(state, account, ai_cfg.as_ref(), &mail).await;

        let notify_urgent = msg.ai_classification.as_deref() == Some("urgente")
            && msg.filter_action != Some(FilterAction::Silencioso)
            && msg.filter_action != Some(FilterAction::Ignorar);
        if notify_urgent {
            let label = msg.filter_label.as_deref().map(|l| format!(" [{}]", l)).unwrap_or_default();
            result.urgent.push(format!("- {}{}: {}", msg.from, label, msg.subject));
        }
        if msg.filter_action != Some(FilterAction::Ignorar) {
            result.new_emails += 1;
        }

        let username = account.username.clone();
        let uid = mail.uid;
        let message_id = mail.message_id.clone();
        let body = mail.body_text.clone();
        let saved = db_op(&state.db, move |conn| {
            let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
            store::insert_email(
                &tx,
                &NewEmail { username: &username, msg: &msg, message_id: message_id.as_deref(), body_text: &body },
            )?;
            let mut ids = Vec::new();
            for (source, p) in &proposals {
                ids.push((store::insert_proposal(&tx, &username, Some(uid), source, p)?, p.clone()));
            }
            tx.commit().map_err(|e| e.to_string())?;
            Ok(ids)
        })
        .await;
        match saved {
            Ok(ids) => result.proposals.extend(ids),
            Err((_, e)) => eprintln!("[Email] Error guardando correo {} de {}: {}", uid, account.username, e),
        }
    }
    Ok(result)
}

/// Filtros + .ics + IA para un correo. Devuelve el mensaje y sus propuestas (origen, item).
async fn process_mail(
    state: &AppState,
    account: &EmailAccount,
    ai_cfg: Option<&crate::ai::AiConfig>,
    mail: &FetchedMail,
) -> (EmailMessage, Vec<(&'static str, ProposedItem)>) {
    let mut msg = EmailMessage {
        uid: mail.uid,
        from: mail.from.clone(),
        subject: mail.subject.clone(),
        date: mail.date.clone(),
        body_preview: String::new(),
        ai_classification: None,
        ai_summary: None,
        ai_action: None,
        filter_label: None,
        filter_action: None,
        processed: false,
        task_created: false,
        fetched_at: Utc::now(),
    };
    let mut proposals: Vec<(&'static str, ProposedItem)> = Vec::new();

    if let Some((label, action, auto_tag)) = apply_filters(&account.filters, &mail.from) {
        msg.filter_label = Some(label.clone());
        msg.filter_action = Some(action.clone());
        match action {
            // Se guarda (para no volver a bajarlo) pero no se muestra ni se analiza
            FilterAction::Ignorar => return (msg, proposals),
            FilterAction::Prioritario => {
                msg.ai_classification = Some("urgente".to_string());
                msg.ai_summary = Some(format!("Correo prioritario ({})", label));
                msg.ai_action = auto_tag.or(Some("Responder".to_string()));
                msg.processed = true;
            }
            FilterAction::Silencioso | FilterAction::Normal => {}
        }
    }

    // Invitaciones de calendario: exactas, sin IA
    for ev in mail.ics.iter().filter(|e| !e.cancelled) {
        proposals.push((
            "ics",
            ProposedItem {
                kind: "evento",
                title: ev.summary.clone(),
                description: ev.description.clone(),
                date: Some(ev.date.clone()),
                time: Some(ev.time.clone()),
                end_time: ev.end_time.clone(),
                location: ev.location.clone(),
            },
        ));
    }

    if let Some(cfg) = ai_cfg {
        let want_events = mail.ics.is_empty();
        match analyze_email(&state.http_client, cfg, &mail.from, &mail.subject, &mail.date, &mail.body_text, want_events).await {
            Ok(a) => {
                if !msg.processed {
                    msg.ai_classification = Some(a.classification);
                    msg.ai_summary = Some(a.summary);
                    msg.ai_action = Some(a.action);
                    msg.processed = true;
                }
                proposals.extend(a.proposals.into_iter().map(|p| ("ia", p)));
            }
            Err(e) => eprintln!("[Email] IA no pudo analizar el correo de {}: {}", account.email, e),
        }
    }
    (msg, proposals)
}

/// Avisos tras una revision: bus en vivo (web/visor) y Telegram
pub async fn announce(state: &AppState, username: &str, r: &CheckResult) {
    if r.new_emails > 0 {
        state.events.publish(
            "email.inbox",
            serde_json::json!({ "new": r.new_emails }),
            Audience::User(username.to_string()),
            Some("email"),
        );
    }
    if !r.proposals.is_empty() {
        super::proposals::publish_count(state, username).await;
        let n = r.proposals.len();
        events::notify(
            state,
            Audience::User(username.to_string()),
            Some("email"),
            Level::Info,
            "Propuestas de la IA",
            &format!("{} {} desde tu correo para revisar", n, if n == 1 { "propuesta" } else { "propuestas" }),
        );
    }

    if r.urgent.is_empty() && r.proposals.is_empty() {
        return;
    }
    let u = username.to_string();
    let target = db_op(&state.db, move |conn| {
        let token: Option<String> = conn
            .query_row("SELECT labnas_decrypt(bot_token) FROM notification_config WHERE id = 1", [], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())?
            .flatten();
        let chat: Option<i64> = conn
            .query_row("SELECT chat_id FROM telegram_chats WHERE linked_web_user = ?1", params![u], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(token.zip(chat))
    })
    .await
    .ok()
    .flatten();
    let Some((token, chat_id)) = target else { return };

    let mut msg = String::new();
    if !r.urgent.is_empty() {
        msg.push_str(&format!(
            "*Correo urgente!*\n\n{} correo(s) nuevo(s), {} urgente(s):\n{}\n",
            r.new_emails,
            r.urgent.len(),
            md_escape(&r.urgent.join("\n"))
        ));
    }
    if !r.proposals.is_empty() {
        if !msg.is_empty() {
            msg.push('\n');
        }
        msg.push_str("*Propuestas desde tu correo:*\n");
        for (id, p) in &r.proposals {
            let when = match (&p.date, &p.time) {
                (Some(d), Some(t)) => format!(" — {} {}", d, t),
                (Some(d), None) => format!(" — {}", d),
                _ => String::new(),
            };
            let icon = if p.kind == "evento" { "📅" } else { "✅" };
            msg.push_str(&format!("{} {}{}\n  `/aprobar {}`  `/descartar {}`\n", icon, md_escape(&p.title), when, id, id));
        }
    }
    let _ = send_telegram_notification(&state.http_client, &token, chat_id, &msg).await;
}

/// Escapa el Markdown de Telegram en texto ajeno (asuntos, remitentes): un "_" suelto
/// hace que Telegram rechace el mensaje completo
fn md_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '_' | '*' | '`' | '[') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Revisa todas las cuentas cada 5 minutos
pub async fn email_check_loop(state: AppState) {
    tokio::time::sleep(Duration::from_secs(30)).await;
    let mut last_purge = std::time::Instant::now() - Duration::from_secs(86_400);

    loop {
        let accounts = db_op(&state.db, store::load_accounts).await.unwrap_or_else(|(_, e)| {
            eprintln!("[Email] Error leyendo cuentas: {}", e);
            Vec::new()
        });

        for account in &accounts {
            match check_account(&state, account).await {
                Ok(r) => announce(&state, &account.username, &r).await,
                Err(e) => eprintln!("[Email] Error {:?} para {}: {}", account.protocol, account.email, e),
            }
        }

        if last_purge.elapsed() >= Duration::from_secs(86_400) {
            last_purge = std::time::Instant::now();
            let _ = db_op(&state.db, |conn| {
                let days = crate::db::get_setting_u32(conn, "email_retention_days", DEFAULT_RETENTION_DAYS);
                store::purge_old(conn, days)
            })
            .await;
        }

        tokio::time::sleep(CHECK_EVERY).await;
    }
}

/// Enviar notificacion por Telegram (wrapper simple)
pub(super) async fn send_telegram_notification(
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
    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
    client
        .post(&url)
        .json(&body)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Error Telegram: {}", e))?;
    Ok(())
}
