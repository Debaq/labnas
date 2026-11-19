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
/// Devuelve a cuantos chats llego.
pub async fn notify_active_chats(state: &AppState, text: &str) -> usize {
    notify_chats_where(state, "role != 'pendiente'", text).await
}

/// Envia un mensaje (Markdown) a todos los chats de Telegram con rol admin.
/// Devuelve a cuantos chats llego (0 si no hay bot o ningun admin lo tiene vinculado).
pub async fn notify_admins(state: &AppState, text: &str) -> usize {
    notify_chats_where(state, ADMIN_CHATS, text).await
}

/// Chats de administradores: con rol admin o vinculados a una cuenta web de admin (el rol del
/// chat puede haber quedado desactualizado si se vinculo antes de ser admin)
const ADMIN_CHATS: &str =
    "role = 'admin' OR linked_web_user IN (SELECT username FROM web_users WHERE role = 'admin')";

/// Token del bot y chats que cumplen `where_sql` (None si no hay bot)
async fn tg_targets(state: &AppState, where_sql: &'static str) -> Option<(String, Vec<i64>)> {
    crate::db::db_op(&state.db, move |conn| {
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
    .flatten()
}

/// Envia una foto (JPEG) con texto a los chats de los administradores.
/// Devuelve a cuantos chats llego, o por que no llego a ninguno.
pub async fn notify_admins_photo(state: &AppState, jpeg: Vec<u8>, caption: &str) -> Result<usize, String> {
    let Some((token, chats)) = tg_targets(state, ADMIN_CHATS).await else {
        return Err("no hay bot de Telegram configurado (Configuracion > Notificaciones)".into());
    };
    if chats.is_empty() {
        return Err("ningun chat de administrador: en Configuracion > Notificaciones dale rol admin a tu chat o vinculalo a tu cuenta".into());
    }
    let url = format!("{}/bot{}/sendPhoto", telegram_base(), token);
    let (mut sent, mut last_err) = (0, String::new());
    for chat_id in chats {
        let part = match reqwest::multipart::Part::bytes(jpeg.clone()).file_name("foto.jpg").mime_str("image/jpeg") {
            Ok(p) => p,
            Err(e) => return Err(e.to_string()),
        };
        let form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .text("caption", crate::textutil::take_chars(caption, 1000))
            .part("photo", part);
        match state.http_client.post(&url).multipart(form).timeout(std::time::Duration::from_secs(30)).send().await {
            Ok(r) if r.status().is_success() => sent += 1,
            Ok(r) => {
                let status = r.status();
                let body: serde_json::Value = r.json().await.unwrap_or_default();
                last_err = format!("Telegram respondio {}: {}", status, body["description"].as_str().unwrap_or("sin detalle"));
            }
            Err(e) => last_err = format!("no se pudo contactar a Telegram: {}", e),
        }
    }
    if sent == 0 { Err(last_err) } else { Ok(sent) }
}

/// API de Telegram (en los tests, un servidor falso)
fn telegram_base() -> String {
    #[cfg(test)]
    if let Some(b) = TEST_TELEGRAM_BASE.lock().ok().and_then(|b| b.clone()) {
        return b;
    }
    "https://api.telegram.org".into()
}

#[cfg(test)]
pub static TEST_TELEGRAM_BASE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub(super) async fn notify_chats_where(state: &AppState, where_sql: &'static str, text: &str) -> usize {
    let tg_data = tg_targets(state, where_sql).await;
    let mut sent = 0;
    if let Some((token, chats)) = tg_data {
        for chat_id in chats {
            if send_telegram_message(&state.http_client, &token, chat_id, text).await.is_ok() {
                sent += 1;
            }
        }
    }
    sent
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
