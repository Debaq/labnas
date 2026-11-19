//! Proveedor de IA generico: cualquier API compatible con OpenAI
//! (`POST {base_url}/chat/completions`): DeepSeek, Groq, OpenAI, Ollama, LM Studio...
//!
//! Configuracion global (solo admin) en `settings`:
//! - `ai_base_url` (por defecto DeepSeek)
//! - `ai_model`
//! - `ai_api_key` (cifrada; opcional para Ollama local)
//!
//! La IA solo se activa si hay clave o si la URL apunta a un servidor propio.

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::db::db_op;
use crate::state::{AppState, SessionInfo};

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-chat";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl AiConfig {
    /// Lee la configuracion. `None` si la IA no esta configurada.
    pub fn load(conn: &rusqlite::Connection) -> Option<Self> {
        let base_url = crate::db::get_setting(conn, "ai_base_url")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = crate::db::get_setting(conn, "ai_model")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let api_key = crate::db::get_secret_setting(conn, "ai_api_key").filter(|k| !k.is_empty());
        // Sin clave solo sirve un servidor propio (Ollama, LM Studio)
        if api_key.is_none() && !is_local_server(&base_url) {
            return None;
        }
        Some(Self { base_url, model, api_key })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

/// localhost, IP de la LAN/Tailscale o nombre .local: no exige clave
pub(crate) fn is_local_server(base_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else { return false };
    match url.host() {
        Some(url::Host::Domain(d)) => d == "localhost" || d.ends_with(".local") || !d.contains('.'),
        Some(url::Host::Ipv4(ip)) => !crate::netguard::is_public_ip(ip.into()),
        Some(url::Host::Ipv6(ip)) => !crate::netguard::is_public_ip(ip.into()),
        None => false,
    }
}

/// Pide una respuesta en JSON y la devuelve parseada.
/// `system` lleva las instrucciones; `user` los datos (p.ej. un correo).
pub async fn chat_json(
    client: &reqwest::Client,
    cfg: &AiConfig,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({
        "model": cfg.model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
        "temperature": 0.1,
        "max_tokens": max_tokens,
        "response_format": { "type": "json_object" },
        "stream": false,
    });

    let mut req = client.post(cfg.endpoint()).json(&body).timeout(REQUEST_TIMEOUT);
    if let Some(key) = &cfg.api_key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| format!("Error conectando con la IA: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("La IA respondio {}: {}", status, crate::textutil::ellipsize(&text, 300)));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| format!("Respuesta de la IA invalida: {}", e))?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "Respuesta de la IA sin contenido".to_string())?;
    parse_json_content(content)
}

/// Algunos modelos envuelven el JSON en ```json ... ``` aunque se pida lo contrario
fn parse_json_content(content: &str) -> Result<serde_json::Value, String> {
    let trimmed = content.trim();
    let inner = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(trimmed)
        .trim();
    serde_json::from_str(inner).map_err(|e| {
        format!("JSON de la IA invalido: {} ({})", e, crate::textutil::ellipsize(inner, 200))
    })
}

// ─── Endpoints de configuracion (admin) ───

#[derive(Serialize)]
pub struct AiConfigView {
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
    pub enabled: bool,
}

/// GET /api/ai/config
pub async fn get_config(State(state): State<AppState>) -> Result<Json<AiConfigView>, (StatusCode, String)> {
    db_op(&state.db, |conn| {
        let base_url = crate::db::get_setting(conn, "ai_base_url").filter(|s| !s.is_empty());
        let model = crate::db::get_setting(conn, "ai_model").filter(|s| !s.is_empty());
        let has_key = crate::db::get_secret_setting(conn, "ai_api_key").is_some_and(|k| !k.is_empty());
        Ok(AiConfigView {
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            has_key,
            enabled: AiConfig::load(conn).is_some(),
        })
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct SetAiConfig {
    pub base_url: String,
    pub model: String,
    /// `None`: no cambia la clave; `Some("")`: la borra
    #[serde(default)]
    pub api_key: Option<String>,
}

/// PUT /api/ai/config
pub async fn set_config(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<SetAiConfig>,
) -> Result<Json<AiConfigView>, (StatusCode, String)> {
    let base_url = req.base_url.trim().trim_end_matches('/').to_string();
    let parsed = reqwest::Url::parse(&base_url).map_err(|_| (StatusCode::BAD_REQUEST, "URL invalida".to_string()))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err((StatusCode::BAD_REQUEST, "La URL debe ser http(s)".to_string()));
    }
    let model = req.model.trim().to_string();
    if model.is_empty() || model.len() > 200 {
        return Err((StatusCode::BAD_REQUEST, "Modelo invalido".to_string()));
    }
    let key = req.api_key.map(|k| k.trim().to_string());

    db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, "ai_base_url", &base_url)?;
        crate::db::set_setting(conn, "ai_model", &model)?;
        if let Some(k) = key {
            crate::db::set_secret_setting(conn, "ai_api_key", &k)?;
        }
        Ok(())
    })
    .await?;

    state.log_activity("ia_config", "Proveedor de IA configurado", &session.username).await;
    get_config(State(state)).await
}

/// POST /api/ai/test: prueba la configuracion guardada con una consulta minima
pub async fn test_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let cfg = db_op(&state.db, |conn| Ok(AiConfig::load(conn)))
        .await?
        .ok_or((StatusCode::BAD_REQUEST, "IA no configurada".to_string()))?;
    let started = std::time::Instant::now();
    let r = chat_json(
        &state.http_client,
        &cfg,
        "Responde solo con un objeto JSON.",
        "Devuelve {\"ok\": true}",
        20,
    )
    .await
    .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(serde_json::json!({
        "ok": r.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
        "model": cfg.model,
        "ms": started.elapsed().as_millis() as u64,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_con_y_sin_cercas() {
        assert_eq!(parse_json_content("{\"a\":1}").unwrap()["a"], 1);
        assert_eq!(parse_json_content("```json\n{\"a\":2}\n```").unwrap()["a"], 2);
        assert_eq!(parse_json_content("  ```\n{\"a\":3}```  ").unwrap()["a"], 3);
        assert!(parse_json_content("no es json").is_err());
    }

    #[test]
    fn sin_clave_solo_servidor_local() {
        assert!(is_local_server("http://localhost:11434/v1"));
        assert!(is_local_server("http://192.168.1.20:11434/v1"));
        assert!(is_local_server("http://gpu-box:11434/v1"));
        assert!(is_local_server("http://ia.local:1234/v1"));
        assert!(!is_local_server("https://api.deepseek.com"));
        assert!(!is_local_server("https://1.1.1.1/v1"));
    }
}
