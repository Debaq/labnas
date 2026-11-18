//! Cuentas Microsoft (Outlook.com, Hotmail, Microsoft 365) con OAuth2.
//!
//! Microsoft ya no acepta usuario/contraseña por IMAP/POP. Se usa el flujo de
//! "codigo de dispositivo" (RFC 8628): LabNAS muestra un codigo, el usuario lo
//! ingresa en microsoft.com/devicelogin con su cuenta (y su propio 2FA) y LabNAS
//! recibe un refresh token. No necesita URL publica ni HTTPS: sirve en la LAN.
//!
//! El refresh token se guarda cifrado en `email_accounts.password` (con
//! `auth = 'microsoft'`); el access token (1 h) vive solo en memoria.
//!
//! El admin registra una app gratuita en Microsoft Entra (cliente publico, sin
//! secreto) y guarda su Client ID en `ms_oauth_client_id`.

use super::*;
use axum::Extension;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex as StdMutex, RwLock};
use std::time::Instant;

use super::store;
use crate::models::email::MailAuth;
use crate::state::SessionInfo;

pub const MS_IMAP_HOST: &str = "outlook.office365.com";
pub const MS_IMAP_PORT: u16 = 993;
const SCOPE: &str = "https://outlook.office.com/IMAP.AccessAsUser.All offline_access";
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Solo tests: servidor de login falso
pub static LOGIN_BASE_OVERRIDE: RwLock<Option<String>> = RwLock::new(None);

/// Solo tests: servidor IMAP en vez de Outlook
pub static IMAP_OVERRIDE: RwLock<Option<(String, u16)>> = RwLock::new(None);

fn imap_server() -> (String, u16) {
    IMAP_OVERRIDE
        .read()
        .ok()
        .and_then(|o| o.clone())
        .unwrap_or_else(|| (MS_IMAP_HOST.to_string(), MS_IMAP_PORT))
}

fn login_base() -> String {
    LOGIN_BASE_OVERRIDE
        .read()
        .ok()
        .and_then(|o| o.clone())
        // Desarrollo: servidor de login alternativo (p.ej. uno falso para probar la UI)
        .or_else(|| std::env::var("LABNAS_MS_LOGIN_BASE").ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "https://login.microsoftonline.com".to_string())
}

#[derive(Debug, Clone)]
pub struct MsConfig {
    pub client_id: String,
    pub tenant: String,
}

impl MsConfig {
    pub fn load(conn: &rusqlite::Connection) -> Option<Self> {
        let client_id = crate::db::get_setting(conn, "ms_oauth_client_id").filter(|s| !s.is_empty())?;
        let tenant = crate::db::get_setting(conn, "ms_oauth_tenant")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "common".to_string());
        Some(Self { client_id, tenant })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}/oauth2/v2.0/{}", login_base(), self.tenant, path)
    }
}

// ─── Estado de los flujos en curso y cache de access tokens ───

#[derive(Debug, Clone, Serialize)]
pub struct FlowStatus {
    /// pendiente | conectado | error
    pub status: String,
    pub message: String,
    pub user_code: Option<String>,
    pub verification_uri: Option<String>,
    #[serde(skip)]
    flow_id: u64,
}

static FLOWS: LazyLock<StdMutex<HashMap<String, FlowStatus>>> = LazyLock::new(Default::default);
static TOKENS: LazyLock<StdMutex<HashMap<String, (String, Instant)>>> = LazyLock::new(Default::default);
static NEXT_FLOW: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn set_flow(username: &str, f: FlowStatus) {
    if let Ok(mut m) = FLOWS.lock() {
        m.insert(username.to_string(), f);
    }
}

fn current_flow(username: &str) -> Option<u64> {
    FLOWS.lock().ok().and_then(|m| m.get(username).map(|f| f.flow_id))
}

pub fn forget_tokens(username: &str) {
    if let Ok(mut m) = TOKENS.lock() {
        m.remove(username);
    }
}

// ─── Endpoints ───

#[derive(Serialize)]
pub struct MsConfigView {
    pub configured: bool,
    pub client_id: String,
    pub tenant: String,
}

/// GET /api/email/oauth/microsoft/config (cualquier usuario: el Client ID no es secreto)
pub async fn ms_get_config(State(state): State<AppState>) -> Result<Json<MsConfigView>, (StatusCode, String)> {
    db_op(&state.db, |conn| {
        let cfg = MsConfig::load(conn);
        Ok(MsConfigView {
            configured: cfg.is_some(),
            client_id: cfg.as_ref().map(|c| c.client_id.clone()).unwrap_or_default(),
            tenant: cfg.map(|c| c.tenant).unwrap_or_else(|| "common".to_string()),
        })
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct SetMsConfig {
    pub client_id: String,
    #[serde(default)]
    pub tenant: Option<String>,
}

/// PUT /api/email/oauth/microsoft/config (admin)
pub async fn ms_set_config(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<SetMsConfig>,
) -> Result<Json<MsConfigView>, (StatusCode, String)> {
    let client_id = req.client_id.trim().to_string();
    if !client_id.is_empty() && !is_guid(&client_id) {
        return Err((StatusCode::BAD_REQUEST, "Client ID invalido (formato GUID)".to_string()));
    }
    let tenant = req.tenant.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| "common".to_string());
    let tenant_ok = ["common", "organizations", "consumers"].contains(&tenant.as_str())
        || is_guid(&tenant)
        || (tenant.contains('.') && tenant.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'));
    if !tenant_ok {
        return Err((StatusCode::BAD_REQUEST, "Tenant invalido".to_string()));
    }
    db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, "ms_oauth_client_id", &client_id)?;
        crate::db::set_setting(conn, "ms_oauth_tenant", &tenant)
    })
    .await?;
    state.log_activity("email_oauth_config", "OAuth de Microsoft configurado", &session.username).await;
    ms_get_config(State(state)).await
}

fn is_guid(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_hexdigit() })
}

#[derive(Deserialize)]
pub struct StartRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// POST /api/email/oauth/microsoft/start: pide un codigo y espera la autorizacion en segundo plano
pub async fn ms_start(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<StartRequest>,
) -> Result<Json<FlowStatus>, (StatusCode, String)> {
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 254 || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err((StatusCode::BAD_REQUEST, "Correo invalido".to_string()));
    }
    let cfg = db_op(&state.db, |conn| Ok(MsConfig::load(conn)))
        .await?
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Falta configurar el Client ID de Microsoft (lo hace el admin en Correo > Configuracion)".to_string(),
        ))?;

    let resp = state
        .http_client
        .post(cfg.url("devicecode"))
        .form(&[("client_id", cfg.client_id.as_str()), ("scope", SCOPE)])
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("No se pudo contactar a Microsoft: {}", e)))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err((StatusCode::BAD_GATEWAY, format!("Microsoft rechazo la solicitud: {}", ms_error(&text))));
    }
    let dc: DeviceCodeResponse = resp
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Respuesta de Microsoft invalida: {}", e)))?;

    let flow_id = NEXT_FLOW.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let status = FlowStatus {
        status: "pendiente".to_string(),
        message: format!("Abre {} e ingresa el codigo {}", dc.verification_uri, dc.user_code),
        user_code: Some(dc.user_code.clone()),
        verification_uri: Some(dc.verification_uri.clone()),
        flow_id,
    };
    set_flow(&session.username, status.clone());

    tokio::spawn(poll_device_code(state.clone(), session.username.clone(), email, cfg, dc, flow_id));
    Ok(Json(status))
}

/// GET /api/email/oauth/microsoft/status
pub async fn ms_status(Extension(session): Extension<SessionInfo>) -> Json<serde_json::Value> {
    let f = FLOWS.lock().ok().and_then(|m| m.get(&session.username).cloned());
    Json(match f {
        Some(f) => serde_json::to_value(f).unwrap_or_default(),
        None => serde_json::json!({ "status": "ninguno", "message": "" }),
    })
}

/// Mensaje legible de un error OAuth de Microsoft (sin repetir trazas largas)
fn ms_error(body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let desc = v["error_description"].as_str().or(v["error"].as_str()).unwrap_or(body);
    crate::textutil::ellipsize(desc.lines().next().unwrap_or(desc), 300)
}

async fn poll_device_code(state: AppState, username: String, email: String, cfg: MsConfig, dc: DeviceCodeResponse, flow_id: u64) {
    let deadline = Instant::now() + Duration::from_secs(dc.expires_in.min(1800));
    let mut interval = dc.interval.unwrap_or(5).clamp(1, 30);
    let fail = |msg: String| {
        set_flow(&username, FlowStatus { status: "error".into(), message: msg, user_code: None, verification_uri: None, flow_id });
    };

    let tokens = loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        // Otro intento del mismo usuario reemplazo a este
        if current_flow(&username) != Some(flow_id) {
            return;
        }
        if Instant::now() >= deadline {
            return fail("El codigo vencio. Vuelve a intentarlo.".into());
        }
        let resp = state
            .http_client
            .post(cfg.url("token"))
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", cfg.client_id.as_str()),
                ("device_code", dc.device_code.as_str()),
            ])
            .timeout(HTTP_TIMEOUT)
            .send()
            .await;
        let Ok(resp) = resp else { continue }; // red caida: reintentar hasta el plazo
        let Ok(t) = resp.json::<TokenResponse>().await else { continue };
        match t.error.as_deref() {
            None => break t,
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some("authorization_declined") => return fail("Rechazaste el acceso en Microsoft.".into()),
            Some("expired_token") => return fail("El codigo vencio. Vuelve a intentarlo.".into()),
            Some(other) => {
                let d = t.error_description.as_deref().unwrap_or(other);
                return fail(format!("Microsoft: {}", ms_error(&serde_json::json!({ "error_description": d }).to_string())));
            }
        }
    };

    let (Some(access), Some(refresh)) = (tokens.access_token, tokens.refresh_token) else {
        return fail("Microsoft no entrego un token renovable (falta el permiso offline_access en la app)".into());
    };

    let (u, e, r) = (username.clone(), email.clone(), refresh);
    let saved = db_op(&state.db, move |conn| save_ms_account(conn, &u, &e, &r)).await;
    if let Err((_, e)) = saved {
        return fail(format!("Error guardando la cuenta: {}", e));
    }
    if let Ok(mut m) = TOKENS.lock() {
        let ttl = tokens.expires_in.unwrap_or(3600);
        m.insert(username.clone(), (access, Instant::now() + Duration::from_secs(ttl)));
    }
    set_flow(
        &username,
        FlowStatus {
            status: "conectado".into(),
            message: format!("Cuenta {} conectada", email),
            user_code: None,
            verification_uri: None,
            flow_id,
        },
    );
    state.log_activity("email_configurado", &format!("Cuenta Microsoft: {}", email), &username).await;
    state.events.publish(
        "email.oauth",
        serde_json::json!({ "status": "conectado" }),
        crate::events::Audience::User(username.clone()),
        Some("email"),
    );

    // Primera revision de inmediato
    let u = username.clone();
    if let Ok(Some(account)) = db_op(&state.db, move |conn| store::load_account(conn, &u)).await {
        let _ = run_check(&state, &account).await;
    }
}

fn save_ms_account(conn: &rusqlite::Connection, username: &str, email: &str, refresh: &str) -> Result<(), String> {
    let (host, port) = imap_server();
    let previous: Option<(String, String)> = conn
        .query_row("SELECT host, email FROM email_accounts WHERE username = ?1", params![username], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let filters: String = conn
        .query_row("SELECT filters FROM email_accounts WHERE username = ?1", params![username], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "[]".to_string());
    conn.execute(
        "INSERT OR REPLACE INTO email_accounts (username, host, port, protocol, email, password, filters, auth, last_error, last_check)
         VALUES (?1, ?2, ?3, 'imap', ?4, labnas_encrypt(?5), ?6, 'microsoft', NULL, NULL)",
        params![username, host, port as i64, email, refresh, filters],
    )
    .map_err(|e| e.to_string())?;
    if previous.is_some_and(|(h, e)| h != host || e != email) {
        store::delete_user_emails(conn, username)?;
    }
    Ok(())
}

/// Access token vigente para una cuenta Microsoft (renueva con el refresh token si hace falta)
pub async fn access_token(state: &AppState, account: &EmailAccount) -> Result<String, String> {
    if let Some((tok, exp)) = TOKENS.lock().ok().and_then(|m| m.get(&account.username).cloned()) {
        if exp > Instant::now() + Duration::from_secs(120) {
            return Ok(tok);
        }
    }
    let cfg = db_op(&state.db, |conn| Ok(MsConfig::load(conn)))
        .await
        .map_err(|(_, e)| e)?
        .ok_or("Falta configurar el Client ID de Microsoft")?;
    let resp = state
        .http_client
        .post(cfg.url("token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", cfg.client_id.as_str()),
            ("refresh_token", account.password.as_str()),
            ("scope", SCOPE),
        ])
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("No se pudo contactar a Microsoft: {}", e))?;
    let t: TokenResponse = resp.json().await.map_err(|e| format!("Respuesta de Microsoft invalida: {}", e))?;
    if let Some(err) = t.error {
        forget_tokens(&account.username);
        return Err(if err == "invalid_grant" {
            "La autorizacion de Microsoft vencio o fue revocada: vuelve a conectar la cuenta".to_string()
        } else {
            format!("Microsoft: {}", t.error_description.as_deref().map(|d| ms_error(&serde_json::json!({"error_description": d}).to_string())).unwrap_or(err))
        });
    }
    let access = t.access_token.ok_or("Microsoft no entrego access token")?;
    // Microsoft puede rotar el refresh token: guardar el nuevo
    if let Some(new_refresh) = t.refresh_token.filter(|r| *r != account.password) {
        let u = account.username.clone();
        let _ = db_op(&state.db, move |conn| {
            conn.execute(
                "UPDATE email_accounts SET password = labnas_encrypt(?2) WHERE username = ?1 AND auth = 'microsoft'",
                params![u, new_refresh],
            )
            .map_err(|e| e.to_string())
        })
        .await;
    }
    if let Ok(mut m) = TOKENS.lock() {
        m.insert(account.username.clone(), (access.clone(), Instant::now() + Duration::from_secs(t.expires_in.unwrap_or(3600))));
    }
    Ok(access)
}

/// Cuenta lista para conectarse: en las de Microsoft cambia el refresh token por un access token
pub async fn with_credentials(state: &AppState, account: &EmailAccount) -> Result<EmailAccount, String> {
    let mut a = account.clone();
    if a.auth == MailAuth::Microsoft {
        a.password = access_token(state, account).await?;
    }
    Ok(a)
}

/// Mecanismo SASL XOAUTH2 para IMAP
pub(super) struct XOAuth2 {
    pub user: String,
    pub token: String,
}

impl async_imap::Authenticator for XOAuth2 {
    type Response = String;
    fn process(&mut self, _challenge: &[u8]) -> String {
        format!("user={}\x01auth=Bearer {}\x01\x01", self.user, self.token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_y_xoauth2() {
        assert!(is_guid("9e5f94bc-e8a4-4e73-b8be-63364c29d753"));
        assert!(!is_guid("no-es-un-guid"));
        let mut a = XOAuth2 { user: "yo@uach.cl".into(), token: "tok".into() };
        assert_eq!(async_imap::Authenticator::process(&mut a, b""), "user=yo@uach.cl\x01auth=Bearer tok\x01\x01");
    }
}
