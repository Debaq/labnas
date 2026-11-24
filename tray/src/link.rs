//! Vincular con la cuenta: se abre `/vincular` de LabNAS en el navegador (donde la
//! persona ya tiene su sesion, con 2FA o passkey si corresponde) y la web devuelve el
//! token a `http://127.0.0.1:<puerto>/callback`, donde esta app espera. Mismo esquema
//! que el login de apps nativas con OAuth (RFC 8252).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// Tiempo para que la persona autorice en el navegador
const WAIT: Duration = Duration::from_secs(300);

pub struct Linked {
    pub token: String,
    pub user: Option<String>,
}

fn random_state() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("getrandom");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn pct_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn pct_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|p| {
        let (k, v) = p.split_once('=')?;
        (k == key).then(|| pct_decode(v))
    })
}

fn page(title: &str, text: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>LabNAS</title><style>\
body{{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;\
background:#282a36;color:#f8f8f2;font-family:system-ui,sans-serif}}main{{max-width:520px;padding:24px;text-align:center}}\
h1{{font-size:1.3em;color:#bd93f9}}</style></head><body><main><h1>{title}</h1><p>{text}</p></main></body></html>"
    )
}

fn respond(mut stream: &TcpStream, status: &str, html: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
}

enum Callback {
    Token(Linked),
    Cancelled,
    Ignored,
}

/// Atiende un pedido al puerto local. Solo cuenta el que trae el `state` correcto.
fn handle(stream: &TcpStream, state: &str) -> Callback {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    if BufReader::new(stream).read_line(&mut line).is_err() {
        return Callback::Ignored;
    }
    // GET /callback?state=...&token=... HTTP/1.1
    let target = line.split_whitespace().nth(1).unwrap_or("");
    let Some(query) = target.strip_prefix("/callback?") else {
        respond(stream, "404 Not Found", &page("LabNAS", "Nada por aquí."));
        return Callback::Ignored;
    };
    if query_param(query, "state").as_deref() != Some(state) {
        respond(stream, "400 Bad Request", &page("Enlace vencido", "Vuelve a elegir «Vincular con mi cuenta» en el icono de la bandeja."));
        return Callback::Ignored;
    }
    if query_param(query, "error").is_some() {
        respond(stream, "200 OK", &page("Vinculación cancelada", "Puedes cerrar esta pestaña."));
        return Callback::Cancelled;
    }
    match query_param(query, "token").filter(|t| t.starts_with("lnd_") && t.len() < 200) {
        Some(token) => {
            respond(stream, "200 OK", &page("¡Listo!", "El control de música quedó vinculado. Ya puedes cerrar esta pestaña."));
            let user = query_param(query, "user").filter(|u| !u.is_empty());
            Callback::Token(Linked { token, user })
        }
        None => {
            respond(stream, "400 Bad Request", &page("Algo falló", "No llegó el token. Intenta vincular de nuevo."));
            Callback::Ignored
        }
    }
}

/// Abre el navegador y espera la respuesta (bloquea: llamar desde un hilo aparte).
pub fn link(base: &str) -> Result<Linked, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("no se pudo abrir un puerto local: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let state = random_state();
    let name = gethostname::gethostname().to_string_lossy().into_owned();
    let url = format!("{base}/vincular?port={port}&state={state}&name={}", pct_encode(&name));
    open::that_detached(&url).map_err(|e| format!("no se pudo abrir el navegador: {e}"))?;

    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                match handle(&stream, &state) {
                    Callback::Token(l) => return Ok(l),
                    Callback::Cancelled => return Err("vinculación cancelada".into()),
                    Callback::Ignored => {}
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("se acabó el tiempo para autorizar en el navegador".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codificacion() {
        assert_eq!(pct_encode("PC de Ana"), "PC%20de%20Ana");
        assert_eq!(pct_decode("PC%20de%20Ana"), "PC de Ana");
        assert_eq!(pct_decode("a+b%2"), "a b%2");
        assert_eq!(query_param("state=ab&token=lnd_1", "token").as_deref(), Some("lnd_1"));
        assert_eq!(query_param("state=ab", "token"), None);
    }

    #[test]
    fn callback_con_state_correcto() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(b"GET /callback?state=s1&token=lnd_abc&user=ana HTTP/1.1\r\n\r\n").unwrap();
            let mut out = String::new();
            let _ = std::io::Read::read_to_string(&mut s, &mut out);
            out
        });
        let (stream, _) = listener.accept().unwrap();
        let Callback::Token(l) = handle(&stream, "s1") else { panic!("esperaba token") };
        drop(stream);
        assert_eq!((l.token.as_str(), l.user.as_deref()), ("lnd_abc", Some("ana")));
        assert!(client.join().unwrap().starts_with("HTTP/1.1 200"));
    }
}
