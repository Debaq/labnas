//! Revision periodica de correo y avisos

use super::*;

// =====================
// Background loop
// =====================

/// Loop que revisa correos cada 5 minutos
pub async fn email_check_loop(state: AppState) {
    // Esperar 30 segundos antes de la primera revision
    tokio::time::sleep(Duration::from_secs(30)).await;

    loop {
        let db_data = {
            let conn = match get_conn(&state.db) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[Email] Error obteniendo conexion DB: {}", e);
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    continue;
                }
            };

            // Leer cuentas de email
            let accounts: Vec<EmailAccount> = match conn.prepare(
                "SELECT username, host, port, protocol, email, labnas_decrypt(password), filters FROM email_accounts"
            ) {
                Ok(mut stmt) => {
                    stmt.query_map([], |row| {
                        let proto_str: String = row.get(3)?;
                        let filters_str: String = row.get(6)?;
                        Ok(EmailAccount {
                            username: row.get(0)?,
                            host: row.get(1)?,
                            port: row.get::<_, i64>(2)? as u16,
                            protocol: match proto_str.as_str() {
                                "pop3" => MailProtocol::Pop3,
                                _ => MailProtocol::Imap,
                            },
                            email: row.get(4)?,
                            password: row.get(5)?,
                            filters: serde_json::from_str(&filters_str).unwrap_or_default(),
                        })
                    }).unwrap().filter_map(|r| r.ok()).collect()
                }
                Err(e) => {
                    eprintln!("[Email] Error preparando query: {}", e);
                    Vec::new()
                }
            };

            let groq_key = crate::db::get_secret_setting(&conn, "groq_api_key");

            // Leer bot_token de notification_config
            let token: Option<String> = conn.query_row(
                "SELECT labnas_decrypt(bot_token) FROM notification_config WHERE id = 1",
                [],
                |row| row.get(0),
            ).ok().flatten();

            // Leer chats de telegram_chats
            let chats: Vec<(i64, Option<String>, String)> = match conn.prepare(
                "SELECT chat_id, linked_web_user, name FROM telegram_chats"
            ) {
                Ok(mut stmt) => {
                    stmt.query_map([], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    }).unwrap().filter_map(|r| r.ok()).collect()
                }
                Err(_) => Vec::new(),
            };

            (accounts, groq_key, token, chats)
        };

        let (accounts, groq_key, token, chats) = db_data;

        for account in &accounts {
            let account_clone = account.clone();
            let result =
                tokio::task::spawn_blocking(move || fetch_emails_dispatch(&account_clone)).await;

            let emails_result = match result {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "[Email] Error en spawn_blocking para {}: {}",
                        account.email, e
                    );
                    continue;
                }
            };

            let mut emails = match emails_result {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("[Email] Error {:?} para {}: {}", account.protocol, account.email, e);
                    continue;
                }
            };

            // Aplicar filtros del usuario
            let filters = &account.filters;
            emails.retain_mut(|email| {
                if let Some((label, action, auto_tag)) = apply_filters(filters, &email.from) {
                    email.filter_label = Some(label);
                    email.filter_action = Some(action.clone());
                    match action {
                        FilterAction::Ignorar => return false, // descartar
                        FilterAction::Prioritario => {
                            email.ai_classification = Some("urgente".to_string());
                            email.ai_summary = Some(format!("Correo prioritario ({})", email.filter_label.as_deref().unwrap_or("filtro")));
                            email.ai_action = auto_tag.or(Some("Responder".to_string()));
                            email.processed = true;
                        }
                        FilterAction::Silencioso => {
                            // Se clasificará con IA pero no notificará
                        }
                        FilterAction::Normal => {
                            // Clasificar normalmente con IA
                        }
                    }
                }
                true
            });

            // Clasificar con Groq los que no fueron procesados por filtros
            if let Some(ref key) = groq_key {
                for email in &mut emails {
                    if email.ai_classification.is_none() {
                        match classify_with_groq(&state.http_client, key, email).await {
                            Ok((classification, summary, action)) => {
                                email.ai_classification = Some(classification);
                                email.ai_summary = Some(summary);
                                email.ai_action = Some(action);
                                email.processed = true;
                            }
                            Err(e) => {
                                eprintln!("[Email] Error Groq para {}: {}", account.email, e);
                            }
                        }
                    }
                }
            }

            // Guardar en inbox
            let username = account.username.clone();
            let mut inbox = state.email_inbox.lock().await;

            // Detectar nuevos emails comparando UIDs
            let existing_uids: Vec<u32> = inbox
                .get(&username)
                .map(|existing| existing.iter().map(|e| e.uid).collect())
                .unwrap_or_default();
            let new_emails: Vec<&EmailMessage> = emails
                .iter()
                .filter(|e| !existing_uids.contains(&e.uid))
                .collect();
            let new_count = new_emails.len();
            let new_urgent: Vec<String> = new_emails
                .iter()
                .filter(|e| {
                    e.ai_classification.as_deref() == Some("urgente")
                        && e.filter_action != Some(FilterAction::Silencioso)
                })
                .map(|e| {
                    let label = e.filter_label.as_deref().map(|l| format!(" [{}]", l)).unwrap_or_default();
                    format!("- {}{}: {}", e.from, label, e.subject)
                })
                .collect();

            inbox.insert(username.clone(), emails);
            drop(inbox);

            // Notificar por Telegram si hay correos urgentes nuevos
            if !new_urgent.is_empty() {
                if let Some(ref token) = token {
                    // Buscar chat_id del usuario via linked_web_user
                    let target_chat = chats
                        .iter()
                        .find(|c| c.1.as_deref() == Some(&username));

                    if let Some((chat_id, _, _)) = target_chat {
                        let msg = format!(
                            "*Correo urgente!*\n\n{} correo(s) nuevo(s), {} urgente(s):\n{}",
                            new_count,
                            new_urgent.len(),
                            new_urgent.join("\n")
                        );
                        let _ = send_telegram_notification(
                            &state.http_client,
                            token,
                            *chat_id,
                            &msg,
                        )
                        .await;
                    }
                }
            }
        }

        // Esperar 5 minutos
        tokio::time::sleep(Duration::from_secs(300)).await;
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
