//! Cliente HTTP minimo (HTTP/1.1, `Connection: close`) sobre TCP o rustls.
//!
//! Los pedidos de la bandeja son pocos y chicos; un cliente propio evita arrastrar un
//! runtime async y permite fijar el certificado del servidor la primera vez (TOFU), como
//! hace `labnas-viewer`: LabNAS suele usar un certificado autofirmado.
//! Un certificado valido para una CA publica se acepta siempre.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::config;

const TIMEOUT: Duration = Duration::from_secs(6);
const MAX_BODY: usize = 4 * 1024 * 1024;
pub const USER_AGENT: &str = concat!("labnas-tray/", env!("CARGO_PKG_VERSION"), " tls-pin");

#[derive(Debug, Clone, PartialEq)]
pub struct Url {
    pub https: bool,
    pub host: String,
    pub port: u16,
}

impl Url {
    /// `http(s)://host[:puerto][/...]` (la ruta se ignora)
    pub fn parse(s: &str) -> Option<Url> {
        let s = s.trim();
        let (https, rest) = if let Some(r) = s.strip_prefix("https://") {
            (true, r)
        } else if let Some(r) = s.strip_prefix("http://") {
            (false, r)
        } else {
            (false, s)
        };
        let authority = rest.split(['/', '?', '#']).next()?;
        let default = if https { 443 } else { 80 };
        let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
            let (h, after) = v6.split_once(']')?;
            let port = match after.strip_prefix(':') {
                Some(p) => p.parse().ok()?,
                None => default,
            };
            (h.to_string(), port)
        } else {
            match authority.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), p.parse().ok()?),
                None => (authority.to_string(), default),
            }
        };
        if host.is_empty() {
            return None;
        }
        Some(Url { https, host, port })
    }

    fn host_for_url(&self) -> String {
        if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() }
    }

    pub fn base(&self) -> String {
        let scheme = if self.https { "https" } else { "http" };
        let default = if self.https { 443 } else { 80 };
        if self.port == default {
            format!("{scheme}://{}", self.host_for_url())
        } else {
            format!("{scheme}://{}:{}", self.host_for_url(), self.port)
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// No hubo conexion (servidor apagado, red caida...)
    Net(String),
    /// El certificado del host cambio respecto del guardado: (host, guardada, actual)
    CertChanged(String, String, String),
    /// Respuesta que no se entiende
    Proto(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Net(e) => write!(f, "sin conexion: {e}"),
            Error::CertChanged(h, _, _) => write!(f, "el certificado de {h} cambio"),
            Error::Proto(e) => write!(f, "respuesta invalida: {e}"),
        }
    }
}

pub struct Response {
    pub status: u16,
    pub location: Option<String>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).trim().to_string()
    }
}

/// Huella SHA-256 (DER) en el formato que muestra LabNAS: AA:BB:...
pub fn pct_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

pub fn request(url: &Url, method: &str, path: &str, token: Option<&str>, body: Option<&str>) -> Result<Response, Error> {
    request_timeout(url, method, path, token, body, TIMEOUT)
}

/// Igual que `request`, con otro plazo de lectura (buscar y reproducir llaman a yt-dlp)
pub fn request_timeout(
    url: &Url,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&str>,
    read_timeout: Duration,
) -> Result<Response, Error> {
    let addr = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| Error::Net(e.to_string()))?
        .next()
        .ok_or_else(|| Error::Net("no se pudo resolver el nombre".into()))?;
    let tcp = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|e| Error::Net(e.to_string()))?;
    let _ = tcp.set_read_timeout(Some(read_timeout));
    let _ = tcp.set_write_timeout(Some(TIMEOUT));

    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {USER_AGENT}\r\nAccept: application/json\r\nConnection: close\r\n",
        if (url.https && url.port == 443) || (!url.https && url.port == 80) { url.host_for_url() } else { format!("{}:{}", url.host_for_url(), url.port) }
    );
    if let Some(t) = token {
        head.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    let body = body.unwrap_or("");
    if method != "GET" {
        head.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    head.push_str(body);

    let raw = if url.https {
        let verifier = Arc::new(PinVerifier::new(url.host.clone()));
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::Proto(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(verifier.clone())
            .with_no_client_auth();
        let name = ServerName::try_from(url.host.clone()).map_err(|e| Error::Proto(e.to_string()))?;
        let conn = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| Error::Proto(e.to_string()))?;
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let res = tls.write_all(head.as_bytes()).and_then(|_| read_all(&mut tls));
        if let Some(e) = verifier.changed() {
            return Err(e);
        }
        res.map_err(|e| Error::Net(e.to_string()))?
    } else {
        let mut tcp = tcp;
        tcp.write_all(head.as_bytes()).map_err(|e| Error::Net(e.to_string()))?;
        read_all(&mut tcp).map_err(|e| Error::Net(e.to_string()))?
    };
    parse_response(&raw)
}

/// Lee hasta que el servidor cierra. Un cierre TLS sin `close_notify` cuenta como fin.
fn read_all(r: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if out.len() > MAX_BODY {
                    return Err(std::io::Error::other("respuesta demasiado grande"));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !out.is_empty() => break,
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

fn parse_response(raw: &[u8]) -> Result<Response, Error> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| Error::Proto("sin cabeceras".into()))?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let mut body = raw[split + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::Proto("linea de estado".into()))?;
    let mut location = None;
    let mut chunked = false;
    for l in lines {
        let Some((k, v)) = l.split_once(':') else { continue };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        match k.as_str() {
            "location" => location = Some(v.to_string()),
            "transfer-encoding" => chunked = v.eq_ignore_ascii_case("chunked"),
            "content-length" => {
                if let Ok(n) = v.parse::<usize>() {
                    body.truncate(n);
                }
            }
            _ => {}
        }
    }
    if chunked {
        body = dechunk(&body).ok_or_else(|| Error::Proto("chunked".into()))?;
    }
    Ok(Response { status, location, body })
}

fn dechunk(mut data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let eol = data.windows(2).position(|w| w == b"\r\n")?;
        let size_str = std::str::from_utf8(&data[..eol]).ok()?;
        let size = usize::from_str_radix(size_str.split(';').next()?.trim(), 16).ok()?;
        data = &data[eol + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(data.get(..size)?);
        data = data.get(size + 2..)?;
    }
}

// ─── Certificados: CA publica o huella fijada la primera vez ───

#[derive(Debug)]
struct PinVerifier {
    host: String,
    webpki: Option<Arc<WebPkiServerVerifier>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
    changed: std::sync::Mutex<Option<(String, String)>>,
}

impl PinVerifier {
    fn new(host: String) -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let webpki = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone()).build().ok();
        PinVerifier { host, webpki, provider, changed: std::sync::Mutex::new(None) }
    }

    fn changed(&self) -> Option<Error> {
        let g = self.changed.lock().ok()?;
        g.as_ref().map(|(old, new)| Error::CertChanged(self.host.clone(), old.clone(), new.clone()))
    }
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if let Some(w) = &self.webpki
            && w.verify_server_cert(end_entity, intermediates, server_name, ocsp, now).is_ok()
        {
            return Ok(ServerCertVerified::assertion());
        }
        let fp = fingerprint(end_entity.as_ref());
        match config::check_pin(&config::read_pins(), &self.host, &fp) {
            config::PinCheck::Trusted => Ok(ServerCertVerified::assertion()),
            config::PinCheck::New => {
                config::save_pin(&self.host, &fp);
                Ok(ServerCertVerified::assertion())
            }
            config::PinCheck::Changed(old) => {
                if let Ok(mut g) = self.changed.lock() {
                    *g = Some((old, fp));
                }
                Err(rustls::Error::General("certificado cambiado".into()))
            }
        }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let u = Url::parse("http://192.168.1.10:3001/dashboard").unwrap();
        assert_eq!(u, Url { https: false, host: "192.168.1.10".into(), port: 3001 });
        assert_eq!(u.base(), "http://192.168.1.10:3001");
        let u = Url::parse("https://labnas.local").unwrap();
        assert_eq!((u.port, u.base()), (443, "https://labnas.local".to_string()));
        let u = Url::parse("https://[fe80::1]:3443/x").unwrap();
        assert_eq!((u.host.as_str(), u.port), ("fe80::1", 3443));
        assert_eq!(u.base(), "https://[fe80::1]:3443");
        assert!(Url::parse("http://").is_none());
    }

    #[test]
    fn respuestas() {
        let r = parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}extra").unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, b"{}".as_slice()));
        let r = parse_response(b"HTTP/1.1 308 Permanent Redirect\r\nlocation: https://x:3443/api/health\r\n\r\n").unwrap();
        assert_eq!(r.location.as_deref(), Some("https://x:3443/api/health"));
        let r = parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nhola\r\n4\r\n que\r\n0\r\n\r\n").unwrap();
        assert_eq!(r.text(), "hola que");
    }
}
