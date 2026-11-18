//! Endpoints de cuenta, bandeja, clasificacion y correo a tarea

use super::*;
use axum::Extension;

use super::store;
use crate::state::SessionInfo;

/// POST /api/email/account - Configurar cuenta IMAP/POP3 (prueba el login antes de guardar)
pub async fn configure_account(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<ConfigureAccountRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let username = session.username.clone();
    if req.host.trim().is_empty() || req.email.trim().is_empty() || req.password.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Host, email y password son requeridos".to_string()));
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
        auth: crate::models::email::MailAuth::Password,
    };

    test_login(&account).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("No se pudo conectar al servidor {}: {}", protocol_label, e))
    })?;

    let acct = account.clone();
    db_op(&state.db, move |conn| {
        // Conserva los filtros si la cuenta ya existia
        let filters: String = conn
            .query_row("SELECT filters FROM email_accounts WHERE username = ?1", params![acct.username], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "[]".to_string());
        let previous: Option<(String, String)> = conn
            .query_row(
                "SELECT host, email FROM email_accounts WHERE username = ?1",
                params![acct.username],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO email_accounts (username, host, port, protocol, email, password, filters, auth)
             VALUES (?1, ?2, ?3, ?4, ?5, labnas_encrypt(?6), ?7, 'password')",
            params![
                acct.username,
                acct.host,
                acct.port as i64,
                if acct.protocol == MailProtocol::Pop3 { "pop3" } else { "imap" },
                acct.email,
                acct.password,
                filters,
            ],
        )
        .map_err(|e| e.to_string())?;
        // Otra casilla: los UID guardados ya no significan nada
        if previous.is_some_and(|(h, e)| h != acct.host || e != acct.email) {
            store::delete_user_emails(conn, &acct.username)?;
        }
        Ok(())
    })
    .await?;

    state
        .log_activity("email_configurado", &format!("Cuenta {}: {}", protocol_label, account.email), &username)
        .await;

    Ok((StatusCode::OK, format!("Cuenta {} configurada correctamente", protocol_label)))
}

/// DELETE /api/email/account - Eliminar mi cuenta (y los correos guardados)
pub async fn delete_account(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<StatusCode, (StatusCode, String)> {
    let uname = session.username.clone();
    let deleted = db_op(&state.db, move |conn| {
        let changes = conn
            .execute("DELETE FROM email_accounts WHERE username = ?1", params![&uname])
            .map_err(|e| e.to_string())?;
        store::delete_user_emails(conn, &uname)?;
        Ok(changes)
    })
    .await?;

    if deleted == 0 {
        return Err((StatusCode::NOT_FOUND, "No tienes cuenta de correo configurada".to_string()));
    }
    super::oauth::forget_tokens(&session.username);

    state.log_activity("email_eliminado", "Cuenta de correo eliminada", &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/email/account - Mi cuenta (sin contraseña) y el resultado de la ultima revision
pub async fn get_account(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<store::AccountView>, (StatusCode, String)> {
    let u = session.username.clone();
    db_op(&state.db, move |conn| store::account_view(conn, &u))
        .await?
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "No tienes cuenta de correo configurada".to_string()))
}

/// GET /api/email/inbox - Ver mi bandeja (ultimos correos procesados)
pub async fn get_inbox(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<Vec<EmailMessage>>, (StatusCode, String)> {
    let u = session.username.clone();
    db_op(&state.db, move |conn| store::list_inbox(conn, &u)).await.map(Json)
}

/// POST /api/email/check - Revisar correo ahora
pub async fn check_now(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let username = session.username.clone();
    let u = username.clone();
    let account = db_op(&state.db, move |conn| store::load_account(conn, &u))
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No tienes cuenta de correo configurada".to_string()))?;

    let r = run_check(&state, &account)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error obteniendo correos: {}", e)))?;

    let msg = match (r.new_emails, r.proposals.len()) {
        (0, _) => "Sin correos nuevos".to_string(),
        (n, 0) => format!("{} correos nuevos", n),
        (n, p) => format!("{} correos nuevos, {} propuestas de la IA", n, p),
    };
    state.log_activity("email_check", &msg, &username).await;
    Ok((StatusCode::OK, msg))
}

/// POST /api/email/classify/{uid} - (Re)analizar un correo con IA
pub async fn classify_email(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(uid): Path<u32>,
) -> Result<Json<EmailMessage>, (StatusCode, String)> {
    let u = session.username.clone();
    let (cfg, found) = db_op(&state.db, move |conn| {
        Ok((crate::ai::AiConfig::load(conn), store::get_email(conn, &u, uid)?))
    })
    .await?;
    let cfg = cfg.ok_or((StatusCode::BAD_REQUEST, "IA no configurada".to_string()))?;
    let (mut email, body) = found.ok_or((StatusCode::NOT_FOUND, "Correo no encontrado".to_string()))?;

    // Solo clasifica: re-proponer duplicaria eventos y tareas ya propuestos
    let a = super::ai::analyze_email(&state.http_client, &cfg, &email.from, &email.subject, &email.date, &body, false)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;

    let u = session.username.clone();
    let (c, s, ac) = (a.classification.clone(), a.summary.clone(), a.action.clone());
    db_op(&state.db, move |conn| store::set_ai(conn, &u, uid, &c, &s, &ac)).await?;

    email.ai_classification = Some(a.classification);
    email.ai_summary = Some(a.summary);
    email.ai_action = Some(a.action);
    email.processed = true;
    Ok(Json(email))
}

/// POST /api/email/to-task/{uid} - Convertir email en tarea
pub async fn email_to_task(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(uid): Path<u32>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let title = create_task_from_email(&state, &session.username, &session.username, uid, false)
        .await?;
    state.log_activity("email_a_tarea", &title, &session.username).await;
    Ok((StatusCode::CREATED, format!("Tarea creada: {}", title)))
}

/// Crea una tarea a partir de un correo guardado (web y Telegram). Devuelve el titulo.
pub(super) async fn create_task_from_email(
    state: &AppState,
    username: &str,
    assignee: &str,
    uid: u32,
    insistent: bool,
) -> Result<String, (StatusCode, String)> {
    let (u, a) = (username.to_string(), assignee.to_string());
    crate::db::db_op_status(&state.db, move |conn| {
        let internal = |e: String| (StatusCode::INTERNAL_SERVER_ERROR, e);
        let (email, _) = store::get_email(conn, &u, uid)
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Correo no encontrado".to_string()))?;

        let tx = conn.unchecked_transaction().map_err(|e| internal(e.to_string()))?;
        if !store::claim_task_created(&tx, &u, uid).map_err(internal)? {
            return Err((StatusCode::CONFLICT, "Ya se creo una tarea para este correo".to_string()));
        }
        let title = match &email.ai_summary {
            Some(summary) if !insistent => format!("[Email] {} - {}", email.subject, summary),
            _ => format!("[Email] {}", email.subject),
        };
        let title = crate::textutil::ellipsize(&title, 200);
        let task_id = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();
        let assigned = serde_json::to_string(&[&a]).unwrap_or_else(|_| "[]".into());
        tx.execute(
            "INSERT INTO tasks (id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder)
             VALUES (?1, NULL, ?2, ?3, ?4, 'pendiente', ?5, NULL, NULL, ?6, ?6, 8, '[]', '[]', ?7, NULL)",
            params![task_id, title, format!("De: {}", email.from), assigned, a, insistent, Utc::now().to_rfc3339()],
        )
        .map_err(|e| internal(e.to_string()))?;
        tx.commit().map_err(|e| internal(e.to_string()))?;
        Ok(title)
    })
    .await
}
