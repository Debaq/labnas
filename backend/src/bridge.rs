//! Cliente del puente PHP (ver docs/PUENTE_PHP.md).
//!
//! LabNAS esta detras del NAT: es LabNAS quien sale hacia el conector PHP del
//! hosting, le pide los pedidos de los navegadores (`/_labnas/poll`, long-poll),
//! los ejecuta en este mismo proceso contra el router de axum y devuelve las
//! respuestas (`/_labnas/respond`). Todo el canal va firmado con HMAC-SHA256.
//!
//! Los pedidos que llegan por el puente llevan la extension [`BridgeOrigin`]: el
//! middleware les niega la terminal, los WebSocket, el registro de cuentas y
//! todo lo que exige rol admin.

use axum::{
    body::Body,
    extract::State,
    http::{HeaderName, HeaderValue, Method, Request, StatusCode},
    response::IntoResponse,
    Extension, Json, Router,
};
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::Duration;
use tower::ServiceExt;

use crate::db::db_op;
use crate::state::{AppState, SessionInfo};

/// Marca de los pedidos que llegaron por el puente
#[derive(Debug, Clone)]
pub struct BridgeOrigin {
    pub client_ip: String,
}

const POLLERS: usize = 2;
const POLL_TIMEOUT: Duration = Duration::from_secs(40);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// Respuestas mas grandes no caben en un POST del hosting (en B2 iran en partes)
const DEFAULT_MAX_BODY: usize = 6 * 1024 * 1024;
/// Cabeceras de respuesta que no se reenvian
const SKIP_RESPONSE_HEADERS: &[&str] = &["content-length", "transfer-encoding", "connection", "keep-alive"];
/// Cabeceras del navegador que se aceptan (las mismas que manda el conector)
const ALLOWED_REQUEST_HEADERS: &[&str] = &[
    "authorization", "cookie", "x-labnas-csrf", "content-type", "range", "if-none-match", "if-modified-since", "if-range", "accept", "accept-language",
];

#[derive(Debug, Clone)]
struct BridgeConfig {
    url: String,
    node_id: String,
    key: Vec<u8>,
    enabled: bool,
}

impl BridgeConfig {
    fn load(conn: &rusqlite::Connection) -> Option<Self> {
        let url = crate::db::get_setting(conn, "bridge_url").filter(|s| !s.is_empty())?;
        let node_id = crate::db::get_setting(conn, "bridge_node_id").filter(|s| !s.is_empty())?;
        let key = crate::db::get_secret_setting(conn, "bridge_key").and_then(|k| hex_decode(&k))?;
        let enabled = crate::db::get_setting(conn, "bridge_enabled").as_deref() != Some("0");
        Some(Self { url, node_id, key, enabled })
    }
}

// ─── Estado visible en la UI ───

#[derive(Debug, Clone, Default, Serialize)]
pub struct BridgeStatus {
    pub connected: bool,
    pub last_ok: Option<String>,
    pub last_error: Option<String>,
    pub served: u64,
    pub remote: Option<serde_json::Value>,
}

static STATUS: LazyLock<StdMutex<BridgeStatus>> = LazyLock::new(Default::default);

fn with_status(f: impl FnOnce(&mut BridgeStatus)) {
    if let Ok(mut s) = STATUS.lock() {
        f(&mut s);
    }
}

fn status_snapshot() -> BridgeStatus {
    STATUS.lock().map(|s| s.clone()).unwrap_or_default()
}

/// Tope de respuesta segun `post_max_size` del hosting (con margen)
fn max_body() -> usize {
    status_snapshot()
        .remote
        .and_then(|r| r["limits"]["post_max_size"].as_i64())
        .filter(|n| *n > 0)
        .map(|n| (n as usize).saturating_sub(64 * 1024) * 9 / 10)
        .unwrap_or(DEFAULT_MAX_BODY)
}

// ─── Firma ───

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// HMAC-SHA256(K, op \n ts \n nonce \n sha256(body) \n query), igual que `lb_verify` en PHP
pub fn sign(key: &[u8], op: &str, ts: i64, nonce: &str, body: &[u8], query: &str) -> String {
    let data = format!("{}\n{}\n{}\n{}\n{}", op, ts, nonce, hex(&Sha256::digest(body)), query);
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC acepta cualquier largo");
    mac.update(data.as_bytes());
    hex(&mac.finalize().into_bytes())
}

fn random_hex(n: usize) -> String {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).expect("getrandom");
    hex(&buf)
}

async fn call(
    client: &reqwest::Client,
    cfg: &BridgeConfig,
    op: &str,
    query: &str,
    body: Vec<u8>,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    let ts = chrono::Utc::now().timestamp();
    let nonce = random_hex(16);
    let sig = sign(&cfg.key, op, ts, &nonce, &body, query);
    let mut url = format!("{}/_labnas/{}", cfg.url.trim_end_matches('/'), op);
    if !query.is_empty() {
        url.push('?');
        url.push_str(query);
    }
    let resp = client
        .post(url)
        .header("X-Node", &cfg.node_id)
        .header("X-Ts", ts.to_string())
        .header("X-Nonce", nonce)
        .header("X-Sig", sig)
        .body(body)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("No se pudo contactar al puente: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("El puente respondio {}: {}", status, crate::textutil::ellipsize(text.trim(), 200)));
    }
    Ok(resp)
}

// ─── Bucle de pollers ───

#[derive(Debug, Deserialize)]
struct PolledRequest {
    id: String,
    method: String,
    path: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    client_ip: String,
    #[serde(default)]
    body_len: usize,
    body_b64: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PollResponse {
    requests: Vec<PolledRequest>,
}

/// Arranca los pollers. `app` es el router completo (con el middleware de permisos).
pub async fn bridge_loop(state: AppState, app: Router) {
    for _ in 0..POLLERS {
        tokio::spawn(worker(state.clone(), app.clone()));
    }
    // Estado del hosting (limites, version) cada 10 minutos
    loop {
        refresh_remote_status(&state).await;
        tokio::time::sleep(Duration::from_secs(600)).await;
    }
}

async fn load_config(state: &AppState) -> Option<BridgeConfig> {
    db_op(&state.db, |conn| Ok(BridgeConfig::load(conn))).await.ok().flatten()
}

async fn refresh_remote_status(state: &AppState) {
    let Some(cfg) = load_config(state).await.filter(|c| c.enabled) else { return };
    match call(&state.http_client, &cfg, "status", "", Vec::new(), CALL_TIMEOUT).await {
        Ok(r) => {
            if let Ok(v) = r.json::<serde_json::Value>().await {
                with_status(|s| s.remote = Some(v));
            }
        }
        Err(e) => with_status(|s| s.last_error = Some(e)),
    }
}

async fn worker(state: AppState, app: Router) {
    let mut backoff = 1u64;
    loop {
        let Some(cfg) = load_config(&state).await.filter(|c| c.enabled) else {
            with_status(|s| s.connected = false);
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };
        let polled = async {
            let r = call(&state.http_client, &cfg, "poll", "", Vec::new(), POLL_TIMEOUT).await?;
            r.json::<PollResponse>().await.map_err(|e| format!("Respuesta del puente invalida: {}", e))
        }
        .await;
        match polled {
            Ok(p) => {
                backoff = 1;
                with_status(|s| {
                    s.connected = true;
                    s.last_ok = Some(chrono::Utc::now().to_rfc3339());
                    s.last_error = None;
                });
                for req in p.requests {
                    tokio::spawn(handle(state.clone(), app.clone(), cfg.clone(), req));
                }
            }
            Err(e) => {
                with_status(|s| {
                    s.connected = false;
                    s.last_error = Some(e.clone());
                });
                eprintln!("[Puente] {}", e);
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
            }
        }
    }
}

/// Ejecuta un pedido del navegador y devuelve la respuesta al puente
async fn handle(state: AppState, app: Router, cfg: BridgeConfig, req: PolledRequest) {
    let id = req.id.clone();
    let (status, headers, body) = match execute(&state, app, &cfg, req).await {
        Ok(r) => r,
        Err((status, msg)) => (status, vec![("content-type".into(), "text/plain; charset=utf-8".into())], msg.into_bytes()),
    };
    let meta = serde_json::json!({ "status": status.as_u16(), "headers": headers });
    let meta = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(meta.to_string());
    let query = format!("id={}&meta={}", id, meta);
    match call(&state.http_client, &cfg, "respond", &query, body, CALL_TIMEOUT).await {
        Ok(_) => with_status(|s| s.served += 1),
        Err(e) => eprintln!("[Puente] No se pudo entregar la respuesta {}: {}", id, e),
    }
}

type Collected = (StatusCode, Vec<(String, String)>, Vec<u8>);

async fn execute(state: &AppState, app: Router, cfg: &BridgeConfig, req: PolledRequest) -> Result<Collected, (StatusCode, String)> {
    // Solo la API: el frontend lo sirve el conector y nada mas pasa por aca
    if !req.path.starts_with("/api/") || req.path.contains("..") || !is_hex_id(&req.id) {
        return Err((StatusCode::NOT_FOUND, "No encontrado".into()));
    }
    let method = Method::from_bytes(req.method.as_bytes()).map_err(|_| (StatusCode::BAD_REQUEST, "Metodo invalido".to_string()))?;

    let body = match req.body_b64 {
        Some(b) => base64::engine::general_purpose::STANDARD
            .decode(b)
            .map_err(|_| (StatusCode::BAD_REQUEST, "Cuerpo invalido".to_string()))?,
        None if req.body_len > 0 => {
            let q = format!("id={}", req.id);
            let r = call(&state.http_client, cfg, "body", &q, Vec::new(), CALL_TIMEOUT)
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
            r.bytes().await.map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?.to_vec()
        }
        None => Vec::new(),
    };

    let uri = if req.query.is_empty() { req.path.clone() } else { format!("{}?{}", req.path, req.query) };
    let mut builder = Request::builder().method(method).uri(&uri);
    for (name, value) in &req.headers {
        let lower = name.to_ascii_lowercase();
        if !ALLOWED_REQUEST_HEADERS.contains(&lower.as_str()) {
            continue;
        }
        if let (Ok(n), Ok(v)) = (HeaderName::from_bytes(lower.as_bytes()), HeaderValue::from_str(value)) {
            builder = builder.header(n, v);
        }
    }
    let mut request = builder.body(Body::from(body)).map_err(|_| (StatusCode::BAD_REQUEST, "Pedido invalido".to_string()))?;
    request.extensions_mut().insert(BridgeOrigin { client_ip: req.client_ip.clone() });

    let response = app.oneshot(request).await.unwrap_or_else(|e| match e {});
    let status = response.status();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .filter(|(n, _)| !SKIP_RESPONSE_HEADERS.contains(&n.as_str()))
        .filter_map(|(n, v)| v.to_str().ok().map(|v| (n.to_string(), v.to_string())))
        .collect();
    let limit = max_body();
    let body = axum::body::to_bytes(response.into_body(), limit).await.map_err(|_| {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("La respuesta supera lo que acepta el hosting por el puente ({} MB)", limit / (1024 * 1024)),
        )
    })?;
    Ok((status, headers, body.to_vec()))
}

fn is_hex_id(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

// ─── Endpoints de configuracion (admin, solo LAN: el puente niega admin) ───

#[derive(Serialize)]
pub struct BridgeView {
    pub linked: bool,
    pub url: Option<String>,
    pub enabled: bool,
    pub status: BridgeStatus,
}

/// GET /api/bridge
pub async fn get_bridge(State(state): State<AppState>) -> Result<Json<BridgeView>, (StatusCode, String)> {
    let cfg = db_op(&state.db, |conn| Ok(BridgeConfig::load(conn))).await?;
    Ok(Json(BridgeView {
        linked: cfg.is_some(),
        url: cfg.as_ref().map(|c| c.url.clone()),
        enabled: cfg.as_ref().is_some_and(|c| c.enabled),
        status: status_snapshot(),
    }))
}

#[derive(Deserialize)]
pub struct LinkRequest {
    pub url: String,
    pub code: String,
}

/// POST /api/bridge/link: enlaza con el codigo que muestra instalar.php
pub async fn link_bridge(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<LinkRequest>,
) -> Result<Json<BridgeView>, (StatusCode, String)> {
    let url = req.url.trim().trim_end_matches('/').to_string();
    let parsed = reqwest::Url::parse(&url).map_err(|_| (StatusCode::BAD_REQUEST, "URL invalida".to_string()))?;
    // HTTPS obligatorio fuera de la red local: por el puente viajan sesiones
    let local = crate::ai::is_local_server(&url);
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && local) {
        return Err((StatusCode::BAD_REQUEST, "La URL del puente debe ser https://".to_string()));
    }
    if parsed.path() != "/" && !parsed.path().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "El puente va en la raiz de un subdominio (p. ej. https://labnas.tudominio.cl)".to_string()));
    }
    let code: String = req.code.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    if code.len() != 16 {
        return Err((StatusCode::BAD_REQUEST, "El codigo tiene 16 caracteres (XXXX-XXXX-XXXX-XXXX)".to_string()));
    }

    let node_id = random_hex(16);
    let key = random_hex(32);
    let resp = state
        .http_client
        .post(format!("{}/_labnas/register", url))
        .json(&serde_json::json!({ "code": code, "node_id": node_id, "key": key }))
        .timeout(CALL_TIMEOUT)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("No se pudo contactar al puente: {}", e)))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let code = if status == reqwest::StatusCode::NOT_FOUND { StatusCode::BAD_GATEWAY } else { StatusCode::BAD_REQUEST };
        return Err((code, format!("El puente rechazo el enlace ({}): {}", status.as_u16(), crate::textutil::ellipsize(text.trim(), 200))));
    }
    let remote: serde_json::Value = resp.json().await.unwrap_or_default();

    let (u, n, k) = (url.clone(), node_id, key);
    db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, "bridge_url", &u)?;
        crate::db::set_setting(conn, "bridge_node_id", &n)?;
        crate::db::set_secret_setting(conn, "bridge_key", &k)?;
        crate::db::set_setting(conn, "bridge_enabled", "1")
    })
    .await?;
    with_status(|s| {
        *s = BridgeStatus::default();
        s.remote = Some(remote);
    });
    state.log_activity("puente_enlazado", &url, &session.username).await;
    get_bridge(State(state)).await
}

#[derive(Deserialize)]
pub struct EnabledRequest {
    pub enabled: bool,
}

/// PUT /api/bridge/enabled
pub async fn set_bridge_enabled(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<EnabledRequest>,
) -> Result<Json<BridgeView>, (StatusCode, String)> {
    let v = if req.enabled { "1" } else { "0" };
    db_op(&state.db, move |conn| crate::db::set_setting(conn, "bridge_enabled", v)).await?;
    state
        .log_activity("puente", if req.enabled { "Puente activado" } else { "Puente desactivado" }, &session.username)
        .await;
    get_bridge(State(state)).await
}

/// DELETE /api/bridge: olvida el enlace (la clave deja de valer de inmediato)
pub async fn unlink_bridge(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<StatusCode, (StatusCode, String)> {
    db_op(&state.db, |conn| {
        conn.execute(
            "DELETE FROM settings WHERE key IN ('bridge_url', 'bridge_node_id', 'bridge_key', 'bridge_enabled')",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;
    with_status(|s| *s = BridgeStatus::default());
    state.log_activity("puente_desenlazado", "Puente PHP olvidado", &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Paquete del conector ───

const PHP_FILES: &[(&str, &str)] = &[
    (".htaccess", include_str!("../../bridge/.htaccess")),
    ("index.php", include_str!("../../bridge/index.php")),
    ("instalar.php", include_str!("../../bridge/instalar.php")),
    ("app/bridge.php", include_str!("../../bridge/app/bridge.php")),
];

/// Arma el zip del conector: PHP + frontend de esta misma version
pub fn build_package(static_dir: Option<&std::path::Path>) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut add = |name: &str, data: &[u8]| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(data).map_err(|e| e.to_string())
    };
    for (name, content) in PHP_FILES {
        add(name, content.as_bytes())?;
    }
    add("VERSION", crate::updater::CURRENT_VERSION.as_bytes())?;
    if let Some(dir) = static_dir {
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            let entries = std::fs::read_dir(&d).map_err(|e| e.to_string())?;
            for entry in entries.flatten() {
                let path = entry.path();
                let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if meta.is_dir() {
                    stack.push(path);
                } else if meta.is_file() {
                    let rel = path.strip_prefix(dir).map_err(|e| e.to_string())?;
                    let name = format!("web/{}", rel.to_string_lossy().replace('\\', "/"));
                    add(&name, &std::fs::read(&path).map_err(|e| e.to_string())?)?;
                }
            }
        }
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// GET /api/bridge/package
pub async fn download_package() -> Result<impl IntoResponse, (StatusCode, String)> {
    let dir = crate::static_dir();
    let data = tokio::task::spawn_blocking(move || build_package(dir.as_deref()))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let name = format!("labnas-conector-{}.zip", crate::updater::CURRENT_VERSION);
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/zip".to_string()),
            (axum::http::header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", name)),
        ],
        data,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firma_estable() {
        // Mismo calculo que lb_verify (PHP): op \n ts \n nonce \n sha256(body) \n query
        let key = hex_decode(&"ab".repeat(32)).unwrap();
        let s1 = sign(&key, "poll", 1_800_000_000, &"0".repeat(32), b"", "");
        assert_eq!(s1.len(), 64);
        assert_ne!(s1, sign(&key, "poll", 1_800_000_000, &"0".repeat(32), b"", "id=1"));
        assert_ne!(s1, sign(&key, "status", 1_800_000_000, &"0".repeat(32), b"", ""));
        assert_eq!(hex_decode("0aff"), Some(vec![10, 255]));
        assert_eq!(hex_decode("0g"), None);
    }

    #[test]
    fn paquete_incluye_php_y_frontend() {
        let dir = std::env::temp_dir().join(format!("labnas-pkg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<html>").unwrap();
        std::fs::write(dir.join("assets/app.js"), "x").unwrap();
        let zip = build_package(Some(&dir)).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
        let names: Vec<String> = (0..archive.len()).map(|i| archive.by_index(i).unwrap().name().unwrap().to_string()).collect();
        for n in [".htaccess", "index.php", "instalar.php", "app/bridge.php", "VERSION", "web/index.html", "web/assets/app.js"] {
            assert!(names.contains(&n.to_string()), "falta {} en {:?}", n, names);
        }
    }
}
