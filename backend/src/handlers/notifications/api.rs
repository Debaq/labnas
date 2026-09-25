//! Endpoints HTTP de configuracion del bot

use super::*;

// =====================
// API Handlers
// =====================

pub async fn get_config(State(state): State<AppState>) -> Result<Json<NotificationConfigResponse>, (StatusCode, String)> {
    let resp = crate::db::db_op(&state.db, |conn| {
        let (bot_token, bot_username, daily_enabled, daily_hour, daily_minute) = read_notif_config(conn)?;
        let chats = read_all_chats(conn)?;
        Ok(NotificationConfigResponse {
            bot_configured: bot_token.is_some(),
            bot_username,
            telegram_chats: chats,
            daily_enabled,
            daily_hour,
            daily_minute,
        })
    })
    .await?;
    Ok(Json(resp))
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

    let bot_username = bot_info.username.clone();
    let token_clone = token.clone();
    let resp = crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE notification_config SET bot_token = labnas_encrypt(?1), bot_username = ?2 WHERE id = 1",
            params![token_clone, bot_username],
        )
        .map_err(|e| format!("DB: {}", e))?;

        let (bot_token, bot_username, daily_enabled, daily_hour, daily_minute) = read_notif_config(conn)?;
        let chats = read_all_chats(conn)?;
        Ok(NotificationConfigResponse {
            bot_configured: bot_token.is_some(),
            bot_username,
            telegram_chats: chats,
            daily_enabled,
            daily_hour,
            daily_minute,
        })
    })
    .await?;

    Ok((StatusCode::OK, Json(resp)))
}

pub async fn delete_bot_token(
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op(&state.db, |conn| {
        conn.execute(
            "UPDATE notification_config SET bot_token = NULL, bot_username = NULL WHERE id = 1",
            [],
        )
        .map_err(|e| format!("DB: {}", e))?;
        conn.execute("DELETE FROM telegram_chats", [])
            .map_err(|e| format!("DB: {}", e))?;
        Ok(())
    })
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_chat_role(
    State(state): State<AppState>,
    Path(chat_id): Path<i64>,
    Json(req): Json<SetRoleRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let new_role = req.role;
    let new_perms = req.permissions.unwrap_or_else(|| default_permissions_for_role(&new_role));
    let role_str = role_to_str(&new_role).to_string();
    let perms_clone = new_perms.clone();
    let role_clone = new_role.clone();

    let linked_user = crate::db::db_op_status(&state.db, move |conn| {
        // Check chat exists
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM telegram_chats WHERE chat_id = ?1",
                params![chat_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
            > 0;

        if !exists {
            return Err((StatusCode::NOT_FOUND, "Chat no encontrado".to_string()));
        }

        conn.execute(
            "UPDATE telegram_chats SET role = ?1, perm_terminal = ?2, perm_impresion = ?3, perm_archivos_escritura = ?4 WHERE chat_id = ?5",
            params![role_str, perms_clone.terminal, perms_clone.impresion, perms_clone.archivos_escritura, chat_id],
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        // Get linked web user
        let linked: Option<String> = conn
            .query_row(
                "SELECT linked_web_user FROM telegram_chats WHERE chat_id = ?1",
                params![chat_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
            .flatten();

        // Sync role to linked web user
        if let Some(ref username) = linked {
            conn.execute(
                "UPDATE web_users SET role = ?1, perm_terminal = ?2, perm_impresion = ?3, perm_archivos_escritura = ?4 WHERE username = ?5",
                params![role_str, perms_clone.terminal, perms_clone.impresion, perms_clone.archivos_escritura, username],
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;
        }

        Ok(linked)
    })
    .await?;

    // Sync active web sessions
    if let Some(username) = linked_user {
        let mut sessions = state.sessions.lock().await;
        for session in sessions.values_mut() {
            if session.username == username {
                session.role = role_clone.clone();
                session.permissions = new_perms.clone();
            }
        }
    }

    Ok(StatusCode::OK)
}

pub async fn delete_chat(
    State(state): State<AppState>,
    Path(chat_id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op_status(&state.db, move |conn| {
        let deleted = conn
            .execute("DELETE FROM telegram_chats WHERE chat_id = ?1", params![chat_id])
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        if deleted == 0 {
            return Err((StatusCode::NOT_FOUND, "Chat no encontrado".to_string()));
        }
        Ok(())
    })
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_schedule(
    State(state): State<AppState>,
    Json(req): Json<ScheduleRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let hour = req.daily_hour.min(23);
    let minute = req.daily_minute.min(59);
    let enabled = req.daily_enabled;

    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE notification_config SET daily_enabled = ?1, daily_hour = ?2, daily_minute = ?3 WHERE id = 1",
            params![enabled, hour, minute],
        )
        .map_err(|e| format!("DB: {}", e))?;
        Ok(())
    })
    .await?;

    Ok(StatusCode::OK)
}

pub async fn send_test(
    State(state): State<AppState>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let (token, chats) = crate::db::db_op(&state.db, |conn| {
        let token = read_bot_token(conn)?;
        let chats = read_all_chats(conn)?;
        Ok((token, chats))
    })
    .await?;

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
