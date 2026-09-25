//! Endpoints de cuenta, bandeja, clasificacion y correo a tarea

use super::*;

// =====================
// API Handlers
// =====================

/// POST /api/email/account - Configurar cuenta IMAP
pub async fn configure_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ConfigureAccountRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    if req.host.is_empty() || req.email.is_empty() || req.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Host, email y password son requeridos".to_string(),
        ));
    }

    let protocol_label = match req.protocol {
        MailProtocol::Imap => "IMAP",
        MailProtocol::Pop3 => "POP3",
    };

    let account = EmailAccount {
        username: username.clone(),
        host: req.host.trim().to_string(),
        port: req.port,
        protocol: req.protocol,
        email: req.email.trim().to_string(),
        password: req.password,
        filters: Vec::new(),
    };

    // Verificar conexion antes de guardar
    let test_account = account.clone();
    let test_result = tokio::task::spawn_blocking(move || fetch_emails_dispatch(&test_account))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error interno: {}", e),
            )
        })?;

    if let Err(e) = test_result {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("No se pudo conectar al servidor {}: {}", protocol_label, e),
        ));
    }

    let acct = account.clone();
    db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO email_accounts (username, host, port, protocol, email, password, filters)
             VALUES (?1, ?2, ?3, ?4, ?5, labnas_encrypt(?6), ?7)",
            params![
                acct.username,
                acct.host,
                acct.port as i64,
                serde_json::to_string(&acct.protocol).unwrap_or_default().trim_matches('"'),
                acct.email,
                acct.password,
                serde_json::to_string(&acct.filters).unwrap_or_else(|_| "[]".to_string()),
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await?;

    state
        .log_activity(
            "email_configurado",
            &format!("Cuenta {}: {}", protocol_label, req.email),
            &username,
        )
        .await;

    Ok((StatusCode::OK, format!("Cuenta {} configurada correctamente", protocol_label)))
}

/// DELETE /api/email/account - Eliminar mi cuenta IMAP
pub async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let deleted = db_op(&state.db, move |conn| {
        let changes = conn.execute(
            "DELETE FROM email_accounts WHERE username = ?1",
            params![&uname],
        ).map_err(|e| e.to_string())?;
        Ok(changes)
    }).await?;

    if deleted == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "No tienes cuenta de correo configurada".to_string(),
        ));
    }

    // Limpiar inbox del usuario
    let mut inbox = state.email_inbox.lock().await;
    inbox.remove(&username);

    state
        .log_activity("email_eliminado", "Cuenta de correo eliminada", &username)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/email/inbox - Ver mi bandeja
pub async fn get_inbox(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<EmailMessage>>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let inbox = state.email_inbox.lock().await;
    let emails = inbox.get(&username).cloned().unwrap_or_default();
    Ok(Json(emails))
}

/// POST /api/email/check - Forzar revision de correo ahora
pub async fn check_now(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let (account, groq_key) = db_op(&state.db, move |conn| {
        let acct = conn.query_row(
            "SELECT username, host, port, protocol, email, labnas_decrypt(password), filters FROM email_accounts WHERE username = ?1",
            params![&uname],
            |row| {
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
            },
        ).optional().map_err(|e| e.to_string())?;
        let gk = crate::db::get_secret_setting(conn, "groq_api_key");
        Ok((acct, gk))
    }).await?;

    let Some(account) = account else {
        return Err((
            StatusCode::NOT_FOUND,
            "No tienes cuenta de correo configurada".to_string(),
        ));
    };

    let cloned_account = account.clone();
    let emails_result = tokio::task::spawn_blocking(move || fetch_emails_dispatch(&cloned_account))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error interno: {}", e),
            )
        })?;

    let mut emails = emails_result.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error obteniendo correos: {}", e),
        )
    })?;

    // Si hay groq_key, clasificar emails nuevos
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
                        eprintln!("[Email] Error clasificando con Groq: {}", e);
                    }
                }
            }
        }
    }

    let count = emails.len();
    let mut inbox = state.email_inbox.lock().await;
    inbox.insert(username.clone(), emails);

    state
        .log_activity(
            "email_check",
            &format!("{} correos obtenidos", count),
            &username,
        )
        .await;

    Ok((
        StatusCode::OK,
        format!("{} correos obtenidos", count),
    ))
}

/// POST /api/email/classify/{uid} - Clasificar un email con IA
pub async fn classify_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(uid): Path<u32>,
) -> Result<Json<EmailMessage>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let groq_key = db_op(&state.db, |conn| {
        Ok(crate::db::get_secret_setting(conn, "groq_api_key"))
    }).await?
    .ok_or((StatusCode::BAD_REQUEST, "Groq API key no configurada".to_string()))?;

    let mut inbox = state.email_inbox.lock().await;
    let emails = inbox
        .get_mut(&username)
        .ok_or((StatusCode::NOT_FOUND, "No tienes correos".to_string()))?;
    let email = emails
        .iter_mut()
        .find(|e| e.uid == uid)
        .ok_or((StatusCode::NOT_FOUND, "Correo no encontrado".to_string()))?;

    // Clasificar con Groq (necesitamos clonar para la llamada async)
    let email_clone = email.clone();
    drop(inbox);

    let (classification, summary, action) =
        classify_with_groq(&state.http_client, &groq_key, &email_clone)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error Groq: {}", e)))?;

    // Guardar resultado
    let mut inbox = state.email_inbox.lock().await;
    let emails = inbox.get_mut(&username).unwrap();
    let email = emails.iter_mut().find(|e| e.uid == uid).unwrap();
    email.ai_classification = Some(classification);
    email.ai_summary = Some(summary);
    email.ai_action = Some(action);
    email.processed = true;

    Ok(Json(email.clone()))
}

/// POST /api/email/to-task/{uid} - Convertir email en tarea
pub async fn email_to_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(uid): Path<u32>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    // Buscar el email
    let mut inbox = state.email_inbox.lock().await;
    let emails = inbox
        .get_mut(&username)
        .ok_or((StatusCode::NOT_FOUND, "No tienes correos".to_string()))?;
    let email = emails
        .iter_mut()
        .find(|e| e.uid == uid)
        .ok_or((StatusCode::NOT_FOUND, "Correo no encontrado".to_string()))?;

    if email.task_created {
        return Err((
            StatusCode::CONFLICT,
            "Ya se creo una tarea para este correo".to_string(),
        ));
    }

    let title = if let Some(ref summary) = email.ai_summary {
        format!("[Email] {} - {}", email.subject, summary)
    } else {
        format!("[Email] {}", email.subject)
    };
    let title = if title.len() > 200 {
        format!("{}...", &title[..197])
    } else {
        title
    };

    email.task_created = true;
    drop(inbox);

    // Crear la tarea
    let from_str = email_from_str(uid, &state, &username).await;
    let task_id = uuid::Uuid::new_v4().to_string()[..6].to_string();
    let description = format!("De: {}", from_str);
    let assigned_json = serde_json::to_string(&vec![&username]).unwrap_or_default();
    let now = Utc::now().to_rfc3339();
    let tid = task_id.clone();
    let t = title.clone();
    let d = description.clone();
    let u = username.clone();
    let aj = assigned_json.clone();
    let n = now.clone();

    db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO tasks (id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                tid, Option::<String>::None, t, d,
                aj, "pendiente", u, Option::<String>::None, Option::<String>::None,
                false, false, 8i64,
                "[]", "[]", n, Option::<String>::None,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await?;

    state
        .log_activity("email_a_tarea", &title, &username)
        .await;

    Ok((StatusCode::CREATED, format!("Tarea creada: {}", title)))
}

/// Auxiliar para obtener el from de un email
pub(super) async fn email_from_str(uid: u32, state: &AppState, username: &str) -> String {
    let inbox = state.email_inbox.lock().await;
    inbox
        .get(username)
        .and_then(|emails| emails.iter().find(|e| e.uid == uid))
        .map(|e| e.from.clone())
        .unwrap_or_default()
}

/// POST /api/email/groq-key - Configurar API key de Groq (admin)
pub async fn set_groq_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetGroqKeyRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let key = req.key.trim().to_string();
    if key.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Key vacia".to_string()));
    }

    let k = key.clone();
    db_op(&state.db, move |conn| {
        crate::db::set_secret_setting(conn, "groq_api_key", &k)
    }).await?;

    state
        .log_activity("groq_key", "API key de Groq configurada", &username)
        .await;

    Ok((StatusCode::OK, "Groq API key configurada".to_string()))
}
