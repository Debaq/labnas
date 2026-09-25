//! Cliente de la API de Telegram y envio a chats

use super::*;

// =====================
// Telegram API helpers
// =====================

pub(super) async fn call_telegram<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    token: &str,
    method: &str,
    body: &serde_json::Value,
) -> Result<T, String> {
    let url = format!("https://api.telegram.org/bot{}/{}", token, method);
    let resp = client
        .post(&url)
        .json(body)
        .timeout(Duration::from_secs(35))
        .send()
        .await
        .map_err(|e| format!("Error de conexion: {}", e))?;

    let tg_resp: TgResponse<T> = resp
        .json()
        .await
        .map_err(|e| format!("Error parseando respuesta: {}", e))?;

    if tg_resp.ok {
        tg_resp
            .result
            .ok_or_else(|| "Respuesta vacia de Telegram".to_string())
    } else {
        Err(tg_resp
            .description
            .unwrap_or_else(|| "Error desconocido".to_string()))
    }
}

/// Envia un mensaje (Markdown) a todos los chats de Telegram aprobados (no pendientes).
pub async fn notify_active_chats(state: &AppState, text: &str) {
    notify_chats_where(state, "role != 'pendiente'", text).await;
}

/// Envia un mensaje (Markdown) a todos los chats de Telegram con rol admin.
pub async fn notify_admins(state: &AppState, text: &str) {
    notify_chats_where(state, "role = 'admin'", text).await;
}

pub(super) async fn notify_chats_where(state: &AppState, where_sql: &'static str, text: &str) {
    let tg_data = crate::db::db_op(&state.db, move |conn| {
        let token: Option<String> = conn
            .query_row("SELECT labnas_decrypt(bot_token) FROM notification_config WHERE id = 1", [], |row| row.get(0))
            .ok()
            .flatten();
        let Some(token) = token.filter(|t| !t.is_empty()) else { return Ok(None) };
        let mut stmt = conn
            .prepare(&format!("SELECT chat_id FROM telegram_chats WHERE {}", where_sql))
            .map_err(|e| e.to_string())?;
        let chats: Vec<i64> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        Ok(Some((token, chats)))
    })
    .await
    .ok()
    .flatten();

    if let Some((token, chats)) = tg_data {
        for chat_id in chats {
            let _ = send_telegram_message(&state.http_client, &token, chat_id, text).await;
        }
    }
}

pub async fn send_tg_public(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
) -> Result<(), String> {
    send_telegram_message(client, token, chat_id, text).await
}

pub(super) async fn send_telegram_message(
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
    let _: serde_json::Value = call_telegram(client, token, "sendMessage", &body).await?;
    Ok(())
}

pub(super) async fn get_updates(
    client: &reqwest::Client,
    token: &str,
    offset: i64,
) -> Result<Vec<TgUpdate>, String> {
    let body = serde_json::json!({
        "offset": offset,
        "timeout": 30,
        "allowed_updates": ["message"]
    });
    call_telegram(client, token, "getUpdates", &body).await
}
