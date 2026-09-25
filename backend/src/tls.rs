//! HTTPS: segundo listener en 3443 (y 443 si hay permiso) junto al HTTP de siempre.
//!
//! Certificado:
//! - propio (p.ej. `tailscale cert`, valido sin advertencias), si se configura; o
//! - autofirmado generado aqui para localhost, el nombre mDNS, el hostname y las IPs
//!   del equipo (se regenera si falta o vence en menos de 30 dias).
//!
//! Se recarga en caliente al cambiarlo. Redirigir HTTP a HTTPS es opcional y excluye
//! la ingesta de sensores (un ESP32 rara vez maneja TLS), /api/health y localhost.

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::state::{AppState, SessionInfo};

pub const HTTPS_PORT: u16 = 3443;
const SETTING: &str = "tls_config";
/// Vigencia del certificado autofirmado
const SELF_SIGNED_YEARS: i32 = 10;
/// Se regenera si vence antes de esto
const RENEW_BEFORE_DAYS: i64 = 30;

type ApiError = (StatusCode, String);

/// Config viva del listener HTTPS (para recargar el certificado sin reiniciar)
static LIVE: OnceLock<axum_server::tls_rustls::RustlsConfig> = OnceLock::new();

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TlsSettings {
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Redirigir HTTP -> HTTPS
    #[serde(default)]
    pub redirect: bool,
    /// Certificado propio (vacio = autofirmado)
    #[serde(default)]
    pub cert_path: String,
    #[serde(default)]
    pub key_path: String,
}

impl Default for TlsSettings {
    fn default() -> Self {
        Self { enabled: true, redirect: false, cert_path: String::new(), key_path: String::new() }
    }
}

pub fn load_settings(conn: &Connection) -> TlsSettings {
    crate::db::get_setting(conn, SETTING)
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

fn tls_dir() -> PathBuf {
    crate::db::data_dir().join("tls")
}

fn self_signed_paths() -> (PathBuf, PathBuf) {
    let d = tls_dir();
    (d.join("cert.pem"), d.join("key.pem"))
}

/// Nombres e IPs para el certificado autofirmado
fn local_names(conn: &Connection) -> Vec<String> {
    let mut names = vec!["localhost".to_string(), "127.0.0.1".to_string(), "::1".to_string()];
    let mdns = crate::db::get_setting(conn, "mdns_hostname")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "labnas".to_string());
    names.push(format!("{}.local", mdns));
    if let Some(h) = sysinfo::System::host_name().filter(|h| !h.is_empty()) {
        names.push(h.clone());
        if !h.contains('.') {
            names.push(format!("{}.local", h));
        }
    }
    if let Ok(ifaces) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in ifaces {
            if !ip.is_loopback() {
                names.push(ip.to_string());
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names
}

/// Genera un autofirmado (cert.pem + key.pem, clave 0600)
fn generate_self_signed(names: Vec<String>, cert: &Path, key: &Path) -> Result<(), String> {
    use chrono::Datelike;
    use std::os::unix::fs::PermissionsExt;

    let mut params = rcgen::CertificateParams::new(names).map_err(|e| e.to_string())?;
    params.distinguished_name.push(rcgen::DnType::CommonName, "LabNAS");
    params.distinguished_name.push(rcgen::DnType::OrganizationName, "LabNAS (autofirmado)");
    let now = chrono::Utc::now();
    params.not_before = rcgen::date_time_ymd(now.year(), now.month() as u8, now.day() as u8);
    params.not_after = rcgen::date_time_ymd(now.year() + SELF_SIGNED_YEARS, now.month() as u8, now.day().min(28) as u8);

    let key_pair = rcgen::KeyPair::generate().map_err(|e| e.to_string())?;
    let certificate = params.self_signed(&key_pair).map_err(|e| e.to_string())?;

    if let Some(dir) = cert.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    std::fs::write(cert, certificate.pem()).map_err(|e| e.to_string())?;
    std::fs::write(key, key_pair.serialize_pem()).map_err(|e| e.to_string())?;
    std::fs::set_permissions(key, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct CertInfo {
    pub fingerprint_sha256: String,
    pub not_after: String,
    pub days_left: i64,
    pub names: Vec<String>,
    pub issuer: String,
}

/// Datos del certificado PEM: huella, vencimiento, nombres
pub fn cert_info(pem: &[u8]) -> Result<CertInfo, String> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(pem).map_err(|e| format!("PEM invalido: {}", e))?;
    let x509 = pem.parse_x509().map_err(|e| format!("Certificado invalido: {}", e))?;
    let fingerprint = Sha256::digest(&pem.contents)
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":");
    let not_after = x509.validity().not_after.timestamp();
    let days_left = (not_after - chrono::Utc::now().timestamp()) / 86400;
    let mut names = Vec::new();
    if let Ok(Some(san)) = x509.subject_alternative_name() {
        for n in &san.value.general_names {
            match n {
                x509_parser::extensions::GeneralName::DNSName(d) => names.push(d.to_string()),
                x509_parser::extensions::GeneralName::IPAddress(b) => {
                    let ip = match b.len() {
                        4 => std::net::IpAddr::from(<[u8; 4]>::try_from(*b).unwrap_or_default()).to_string(),
                        16 => std::net::IpAddr::from(<[u8; 16]>::try_from(*b).unwrap_or_default()).to_string(),
                        _ => continue,
                    };
                    names.push(ip);
                }
                _ => {}
            }
        }
    }
    Ok(CertInfo {
        fingerprint_sha256: fingerprint,
        not_after: chrono::DateTime::from_timestamp(not_after, 0).map(|d| d.to_rfc3339()).unwrap_or_default(),
        days_left,
        names,
        issuer: x509.issuer().to_string(),
    })
}

/// Certificado activo: (cert, clave, origen "propio" | "autofirmado")
pub fn active_paths(conn: &Connection) -> Result<(PathBuf, PathBuf, &'static str), String> {
    let s = load_settings(conn);
    if !s.cert_path.trim().is_empty() && !s.key_path.trim().is_empty() {
        return Ok((PathBuf::from(s.cert_path.trim()), PathBuf::from(s.key_path.trim()), "propio"));
    }
    let (cert, key) = self_signed_paths();
    let renew = match std::fs::read(&cert) {
        Ok(pem) => cert_info(&pem).map(|i| i.days_left < RENEW_BEFORE_DAYS).unwrap_or(true),
        Err(_) => true,
    };
    if renew || !key.exists() {
        generate_self_signed(local_names(conn), &cert, &key)?;
        println!("[TLS] Certificado autofirmado generado en {}", cert.display());
    }
    Ok((cert, key, "autofirmado"))
}

pub async fn rustls_config(cert: &Path, key: &Path) -> Result<axum_server::tls_rustls::RustlsConfig, String> {
    axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key)
        .await
        .map_err(|e| format!("No se pudo cargar el certificado: {}", e))
}

/// Deja la config viva para poder recargarla desde la API
pub fn set_live(config: axum_server::tls_rustls::RustlsConfig) {
    let _ = LIVE.set(config);
}

async fn reload_live(cert: &Path, key: &Path) -> Result<(), String> {
    if let Some(live) = LIVE.get() {
        live.reload_from_pem_file(cert, key).await.map_err(|e| format!("No se pudo recargar: {}", e))?;
    }
    Ok(())
}

// ─── Redireccion HTTP -> HTTPS ───

#[derive(Clone)]
pub struct Redirect {
    pub https_port: u16,
}

/// Marca que el visor de escritorio agrega a su User-Agent
pub const VIEWER_UA: &str = "LabNAS-Viewer";

/// Excepciones: sensores (ESP32 sin TLS), health, clientes locales y el visor de
/// escritorio (WebKitGTK no acepta el autofirmado). No es una barrera de seguridad:
/// el HTTP sigue disponible; solo evita romper a esos clientes.
fn exempt(req: &Request) -> bool {
    let path = req.uri().path();
    if path == "/api/sensors/data" || path == "/api/health" {
        return true;
    }
    let ua = req.headers().get(header::USER_AGENT).and_then(|h| h.to_str().ok()).unwrap_or("");
    if ua.contains(VIEWER_UA) {
        return true;
    }
    let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
    let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

pub async fn redirect_middleware(Extension(r): Extension<Redirect>, req: Request, next: Next) -> Response {
    if exempt(&req) {
        return next.run(req).await;
    }
    let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("localhost");
    let host = if host.starts_with('[') {
        host.split_once(']').map(|(h, _)| format!("{}]", h)).unwrap_or_else(|| host.to_string())
    } else {
        host.split(':').next().unwrap_or(host).to_string()
    };
    let port = if r.https_port == 443 { String::new() } else { format!(":{}", r.https_port) };
    let pq = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/");
    let location = format!("https://{}{}{}", host, port, pq);
    let mut resp = StatusCode::PERMANENT_REDIRECT.into_response();
    if let Ok(v) = HeaderValue::from_str(&location) {
        resp.headers_mut().insert(header::LOCATION, v);
    }
    resp
}

// ─── Endpoints ───

#[derive(Serialize)]
pub struct TlsStatus {
    #[serde(flatten)]
    pub settings: TlsSettings,
    pub port: u16,
    pub source: String,
    pub cert: Option<CertInfo>,
    pub error: Option<String>,
}

async fn status(state: &AppState) -> Result<TlsStatus, ApiError> {
    crate::db::db_op(&state.db, |conn| {
        let settings = load_settings(conn);
        let (cert, error, source) = match active_paths(conn) {
            Ok((c, _, src)) => match std::fs::read(&c).map_err(|e| e.to_string()).and_then(|p| cert_info(&p)) {
                Ok(info) => (Some(info), None, src.to_string()),
                Err(e) => (None, Some(e), src.to_string()),
            },
            Err(e) => (None, Some(e), String::new()),
        };
        Ok(TlsStatus { settings, port: HTTPS_PORT, source, cert, error })
    })
    .await
}

/// GET /api/system/tls (admin)
pub async fn get_tls(State(state): State<AppState>) -> Result<Json<TlsStatus>, ApiError> {
    Ok(Json(status(&state).await?))
}

/// PUT /api/system/tls (admin). Certificado: se recarga en caliente;
/// activar/desactivar HTTPS o la redireccion se aplica al reiniciar.
pub async fn set_tls(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(mut req): Json<TlsSettings>,
) -> Result<Json<TlsStatus>, ApiError> {
    req.cert_path = req.cert_path.trim().to_string();
    req.key_path = req.key_path.trim().to_string();
    if req.cert_path.is_empty() != req.key_path.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Indica certificado y clave, o ninguno (autofirmado)".to_string()));
    }
    if !req.cert_path.is_empty() {
        let pem = std::fs::read(&req.cert_path)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("No se pudo leer el certificado: {}", e)))?;
        cert_info(&pem).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        // valida que certificado y clave se puedan cargar juntos
        rustls_config(Path::new(&req.cert_path), Path::new(&req.key_path))
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    }

    let json = serde_json::to_string(&req).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::db::db_op(&state.db, move |conn| crate::db::set_setting(conn, SETTING, &json)).await?;

    let (cert, key) = crate::db::db_op(&state.db, |conn| active_paths(conn).map(|(c, k, _)| (c, k))).await?;
    reload_live(&cert, &key).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    state
        .log_activity(
            "HTTPS",
            &format!(
                "https {}, redirigir {}, certificado {}",
                if req.enabled { "activo" } else { "desactivado" },
                if req.redirect { "si" } else { "no" },
                if req.cert_path.is_empty() { "autofirmado".to_string() } else { req.cert_path.clone() }
            ),
            &session.username,
        )
        .await;
    Ok(Json(status(&state).await?))
}

/// POST /api/system/tls/regenerate (admin): nuevo autofirmado (p.ej. si cambio la IP)
pub async fn regenerate(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<TlsStatus>, ApiError> {
    let (cert, key) = self_signed_paths();
    let (c2, k2) = (cert.clone(), key.clone());
    crate::db::db_op(&state.db, move |conn| generate_self_signed(local_names(conn), &c2, &k2)).await?;
    let (active_cert, active_key) = crate::db::db_op(&state.db, |conn| active_paths(conn).map(|(c, k, _)| (c, k))).await?;
    reload_live(&active_cert, &active_key).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    state.log_activity("HTTPS", "certificado autofirmado regenerado", &session.username).await;
    Ok(Json(status(&state).await?))
}

/// GET /api/tls/cert.pem (publico): para instalarlo como confiable en los equipos
pub async fn download_cert(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let pem = crate::db::db_op(&state.db, |conn| {
        let (c, _, _) = active_paths(conn)?;
        std::fs::read(c).map_err(|e| e.to_string())
    })
    .await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-pem-file"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"labnas.crt\""),
        ],
        pem,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autofirmado_con_nombres_e_ips() {
        let dir = std::env::temp_dir().join(format!("labnas-tls-{}", uuid::Uuid::new_v4()));
        let (cert, key) = (dir.join("cert.pem"), dir.join("key.pem"));
        generate_self_signed(vec!["localhost".into(), "labnas.local".into(), "192.168.1.10".into()], &cert, &key).unwrap();

        let info = cert_info(&std::fs::read(&cert).unwrap()).unwrap();
        assert!(info.names.contains(&"labnas.local".to_string()));
        assert!(info.names.contains(&"192.168.1.10".to_string()), "{:?}", info.names);
        assert!(info.days_left > 365 * 9, "vigencia larga: {}", info.days_left);
        assert_eq!(info.fingerprint_sha256.len(), 32 * 3 - 1);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&key).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(dir);
    }
}
