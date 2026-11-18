//! Descarga de correo por IMAP y POP3 sobre TLS (rustls), sin bloquear el runtime.
//! Solo descarga los mensajes que aun no estan guardados (`known`).

use super::*;
use std::collections::HashSet;
use std::sync::{Arc, LazyLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use super::ics::IcsEvent;

/// Mensajes nuevos por revision (los mas recientes)
const MAX_NEW_PER_CHECK: usize = 20;
/// Texto guardado por correo (lo que ve la IA)
const BODY_MAX_CHARS: usize = 6000;
/// Una revision completa no puede tardar mas que esto
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Tope por mensaje (POP3 los baja enteros)
const MAX_MESSAGE_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct FetchedMail {
    pub uid: u32,
    pub message_id: Option<String>,
    pub from: String,
    pub subject: String,
    pub date: String,
    pub body_text: String,
    pub ics: Vec<IcsEvent>,
}

/// Descarga los correos nuevos de la cuenta (no leidos en IMAP; los ultimos en POP3)
pub async fn fetch_new(account: &EmailAccount, known: &HashSet<u32>) -> Result<Vec<FetchedMail>, String> {
    let work = async {
        match account.protocol {
            MailProtocol::Imap => fetch_imap(account, known).await,
            MailProtocol::Pop3 => fetch_pop3(account, known).await,
        }
    };
    tokio::time::timeout(FETCH_TIMEOUT, work)
        .await
        .map_err(|_| "Tiempo de espera agotado con el servidor de correo".to_string())?
}

/// Solo prueba conexion y login (al configurar la cuenta)
pub async fn test_login(account: &EmailAccount) -> Result<(), String> {
    let work = async {
        match account.protocol {
            MailProtocol::Imap => imap_session(account).await.map(|mut s| {
                tokio::spawn(async move {
                    let _ = s.logout().await;
                });
            }),
            MailProtocol::Pop3 => pop3_login(account).await.map(|_| ()),
        }
    };
    tokio::time::timeout(Duration::from_secs(30), work)
        .await
        .map_err(|_| "Tiempo de espera agotado con el servidor de correo".to_string())?
}

// ─── TLS ───

fn tls_connector() -> tokio_rustls::TlsConnector {
    static CONFIG: LazyLock<Arc<rustls::ClientConfig>> = LazyLock::new(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("versiones TLS")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(cfg)
    });
    tokio_rustls::TlsConnector::from(CONFIG.clone())
}

type Tls = tokio_rustls::client::TlsStream<TcpStream>;

async fn tls_connect(host: &str, port: u16) -> Result<Tls, String> {
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| format!("Sin respuesta de {}:{}", host, port))?
        .map_err(|e| format!("Error conectando a {}:{}: {}", host, port, e))?;
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|_| format!("Nombre de servidor invalido: {}", host))?;
    tls_connector()
        .connect(name, tcp)
        .await
        .map_err(|e| format!("Error TLS con {}: {}", host, e))
}

// ─── IMAP ───

async fn imap_session(account: &EmailAccount) -> Result<async_imap::Session<Tls>, String> {
    let tls = tls_connect(&account.host, account.port).await?;
    let mut client = async_imap::Client::new(tls);
    // El cliente no lee el saludo del servidor por su cuenta
    client
        .read_response()
        .await
        .ok_or("El servidor IMAP cerro la conexion")?
        .map_err(|e| format!("Error leyendo saludo IMAP: {}", e))?;
    client
        .login(&account.email, &account.password)
        .await
        .map_err(|(e, _)| format!("Error de login IMAP: {}", e))
}

async fn fetch_imap(account: &EmailAccount, known: &HashSet<u32>) -> Result<Vec<FetchedMail>, String> {
    use futures_util::TryStreamExt;

    let mut session = imap_session(account).await?;
    session.select("INBOX").await.map_err(|e| format!("Error seleccionando INBOX: {}", e))?;

    let unseen = session
        .uid_search("UNSEEN")
        .await
        .map_err(|e| format!("Error buscando correos: {}", e))?;
    let mut uids: Vec<u32> = unseen.into_iter().filter(|u| !known.contains(u)).collect();
    uids.sort_unstable();
    let uids = &uids[uids.len().saturating_sub(MAX_NEW_PER_CHECK)..];

    let mut out = Vec::new();
    if !uids.is_empty() {
        let set = uids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        // BODY.PEEK no marca como leido
        let fetches: Vec<async_imap::types::Fetch> = session
            .uid_fetch(&set, "(UID BODY.PEEK[])")
            .await
            .map_err(|e| format!("Error obteniendo correos: {}", e))?
            .try_collect()
            .await
            .map_err(|e| format!("Error obteniendo correos: {}", e))?;
        for f in &fetches {
            if let (Some(uid), Some(body)) = (f.uid, f.body()) {
                if let Some(mail) = parse_raw(uid, body) {
                    out.push(mail);
                }
            }
        }
    }
    let _ = session.logout().await;
    Ok(out)
}

// ─── POP3 ───

struct Pop3 {
    io: BufReader<Tls>,
}

impl Pop3 {
    async fn line(&mut self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        let n = self.io.read_until(b'\n', &mut buf).await.map_err(|e| format!("Error leyendo POP3: {}", e))?;
        if n == 0 {
            return Err("El servidor POP3 cerro la conexion".into());
        }
        while buf.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            buf.pop();
        }
        Ok(buf)
    }

    async fn cmd(&mut self, cmd: &str) -> Result<String, String> {
        let s = self.io.get_mut();
        s.write_all(format!("{}\r\n", cmd).as_bytes()).await.map_err(|e| format!("Error enviando POP3: {}", e))?;
        s.flush().await.map_err(|e| format!("Error enviando POP3: {}", e))?;
        let line = String::from_utf8_lossy(&self.line().await?).to_string();
        if !line.starts_with("+OK") {
            // No repetir la contraseña en el error
            let shown = if cmd.starts_with("PASS ") { "PASS ***" } else { cmd };
            return Err(format!("POP3 rechazo {}: {}", shown, line.trim()));
        }
        Ok(line)
    }

    /// Lineas de una respuesta multilinea hasta ".", deshaciendo el byte-stuffing
    async fn multiline(&mut self, max_bytes: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        loop {
            let line = self.line().await?;
            if line == b"." {
                return Ok(out);
            }
            let line = if line.starts_with(b"..") { &line[1..] } else { &line[..] };
            if out.len() + line.len() > max_bytes {
                return Err("Mensaje POP3 demasiado grande".into());
            }
            out.extend_from_slice(line);
            out.extend_from_slice(b"\r\n");
        }
    }
}

async fn pop3_login(account: &EmailAccount) -> Result<Pop3, String> {
    let tls = tls_connect(&account.host, account.port).await?;
    let mut p = Pop3 { io: BufReader::new(tls) };
    let greeting = String::from_utf8_lossy(&p.line().await?).to_string();
    if !greeting.starts_with("+OK") {
        return Err(format!("Saludo POP3 inesperado: {}", greeting.trim()));
    }
    p.cmd(&format!("USER {}", account.email)).await?;
    p.cmd(&format!("PASS {}", account.password)).await?;
    Ok(p)
}

/// POP3 no tiene UID numerico: se deriva del UIDL (mismo calculo que versiones anteriores)
fn pop3_uid(uidl: &str) -> u32 {
    uidl.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32))
}

async fn fetch_pop3(account: &EmailAccount, known: &HashSet<u32>) -> Result<Vec<FetchedMail>, String> {
    let mut p = pop3_login(account).await?;

    p.cmd("UIDL").await?;
    let listing = p.multiline(4 * 1024 * 1024).await?;
    let mut pending: Vec<(usize, u32)> = String::from_utf8_lossy(&listing)
        .lines()
        .filter_map(|l| {
            let (num, uidl) = l.trim().split_once(' ')?;
            Some((num.parse().ok()?, pop3_uid(uidl.trim())))
        })
        .filter(|(_, uid)| !known.contains(uid))
        .collect();
    let start = pending.len().saturating_sub(MAX_NEW_PER_CHECK);
    pending.drain(..start);

    let mut out = Vec::new();
    for (num, uid) in pending {
        if p.cmd(&format!("RETR {}", num)).await.is_err() {
            continue;
        }
        let raw = p.multiline(MAX_MESSAGE_BYTES).await?;
        if let Some(mail) = parse_raw(uid, &raw) {
            out.push(mail);
        }
    }
    let _ = p.cmd("QUIT").await;
    Ok(out)
}

// ─── Parseo comun ───

pub(super) fn parse_raw(uid: u32, raw: &[u8]) -> Option<FetchedMail> {
    use mailparse::MailHeaderMap;
    let parsed = mailparse::parse_mail(raw).ok()?;
    let h = &parsed.headers;
    let body = extract_text_body(&parsed);
    let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(FetchedMail {
        uid,
        message_id: h.get_first_value("Message-ID").map(|s| s.trim().to_string()),
        from: h.get_first_value("From").unwrap_or_else(|| "desconocido".to_string()),
        subject: crate::textutil::ellipsize(&h.get_first_value("Subject").unwrap_or_default(), 300),
        date: h.get_first_value("Date").unwrap_or_default(),
        body_text: crate::textutil::take_chars(&body, BODY_MAX_CHARS),
        ics: super::ics::events_from_mail(&parsed),
    })
}

/// Extrae el texto plano de un email parseado
pub(super) fn extract_text_body(parsed: &mailparse::ParsedMail) -> String {
    if !parsed.subparts.is_empty() {
        for mime in ["text/plain", "text/html"] {
            if let Some(part) = parsed.subparts.iter().find(|p| p.ctype.mimetype.eq_ignore_ascii_case(mime)) {
                let body = part.get_body().unwrap_or_default();
                return if mime == "text/html" { strip_html_tags(&body) } else { body };
            }
        }
        for part in &parsed.subparts {
            let text = extract_text_body(part);
            if !text.is_empty() {
                return text;
            }
        }
        return String::new();
    }

    let ct = parsed.ctype.mimetype.to_lowercase();
    if ct == "text/html" {
        strip_html_tags(&parsed.get_body().unwrap_or_default())
    } else if ct.starts_with("text/") {
        parsed.get_body().unwrap_or_default()
    } else {
        String::new()
    }
}

/// Quita etiquetas HTML (y el contenido de <style>/<script>) de forma basica
pub(super) fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    let mut skip_until: Option<&str> = None;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let t = tag.trim().to_ascii_lowercase();
                if let Some(end) = skip_until {
                    if t.starts_with(end) {
                        skip_until = None;
                    }
                } else if t.starts_with("style") {
                    skip_until = Some("/style");
                } else if t.starts_with("script") {
                    skip_until = Some("/script");
                } else if t.starts_with("br") || t.starts_with("/p") || t.starts_with("/div") || t.starts_with("/tr") {
                    result.push('\n');
                }
            }
            _ if in_tag => tag.push(ch),
            _ if skip_until.is_none() => result.push(ch),
            _ => {}
        }
    }
    let result = result
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    result
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_correo_con_tildes_largas() {
        let cuerpo = "ñ".repeat(10_000);
        let raw = format!(
            "From: =?UTF-8?Q?Mar=C3=ADa?= <maria@uni.cl>\r\nSubject: =?UTF-8?B?UmV1bmnDs24=?=\r\nMessage-ID: <abc@uni.cl>\r\nDate: Wed, 18 Nov 2026 10:00:00 -0300\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}\r\n",
            cuerpo
        );
        let m = parse_raw(7, raw.as_bytes()).unwrap();
        assert_eq!(m.from, "María <maria@uni.cl>");
        assert_eq!(m.subject, "Reunión");
        assert_eq!(m.message_id.as_deref(), Some("<abc@uni.cl>"));
        assert_eq!(m.body_text.chars().count(), BODY_MAX_CHARS);
    }

    #[test]
    fn html_sin_estilos() {
        let t = strip_html_tags("<html><style>p{color:red}</style><p>Hola&nbsp;mundo</p><script>x()</script><br>chao &amp; adios</html>");
        assert_eq!(t, "Hola mundo\nchao & adios");
    }

    #[test]
    fn uid_pop3_estable() {
        assert_eq!(pop3_uid("abc"), 96354);
    }
}

/// Red real: TLS (rustls + raices webpki) y saludo IMAP, sin login.
/// `cargo test -- --ignored saludo_imap_real`
#[cfg(test)]
mod red {
    #[tokio::test]
    #[ignore]
    async fn saludo_imap_real() {
        let tls = super::tls_connect("imap.gmail.com", 993).await.expect("TLS");
        let mut client = async_imap::Client::new(tls);
        let greeting = client.read_response().await.expect("saludo").expect("io");
        assert!(format!("{:?}", greeting.parsed()).contains("Ok"), "{:?}", greeting.parsed());
    }
}
