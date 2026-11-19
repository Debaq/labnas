//! Impresora de red IPP: Windows (o macOS/Linux) agrega una impresora de LabNAS
//! por URL y usa su driver generico (IPP Everywhere / Mopria), sin instalar nada.
//!
//! Cada usuario tiene un token propio en la URL (`/ipp/<token>/<impresora>`):
//! asi el trabajo se cuenta a su nombre, respeta el permiso de impresion y se
//! puede revocar, sin pedir contrasena en Windows. El servidor escucha en un
//! puerto aparte (HTTP plano en la LAN) mientras el modulo de impresion este
//! encendido; el documento llega como PDF, PWG raster o JPEG y sale por `lp`.

use super::*;
use super::ipp_codec::{self as codec, kw, mime, name, op, status, tag, text, uri, ParseError, Value, Writer};

use axum::body::Body;
use axum::extract::{ConnectInfo, DefaultBodyLimit};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::StreamExt;
use serde::Serialize;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::Instant;
use tokio::io::AsyncWriteExt;

pub const IPP_PORT_DEFAULT: u16 = 8631;
/// Cabecera IPP maxima (los atributos; el documento va aparte)
const MAX_HEADER: usize = 256 * 1024;
/// Documento maximo (una pagina PWG a 600 ppp en color puede pesar decenas de MB)
const MAX_DOCUMENT: u64 = 1024 * 1024 * 1024;

pub fn ipp_port() -> u16 {
    std::env::var("LABNAS_IPP_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(IPP_PORT_DEFAULT)
}

static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);
static LISTEN_ERROR: StdMutex<Option<String>> = StdMutex::new(None);

fn set_listen_error(e: Option<String>) {
    if let Ok(mut g) = LISTEN_ERROR.lock() {
        *g = e;
    }
}

fn uptime() -> i32 {
    STARTED.elapsed().as_secs().min(i32::MAX as u64) as i32 + 1
}

// ═══════════════════════════════════════
// Servidor
// ═══════════════════════════════════════

pub fn ipp_router(state: AppState) -> Router {
    Router::new()
        .route("/ipp/{token}/{printer}", get(ipp_info).post(ipp_handle))
        // Algunos clientes mandan las operaciones de un trabajo a la URI del trabajo
        .route("/ipp/{token}/{printer}/{job}", get(ipp_info).post(ipp_handle_job_uri))
        // El limite se aplica mientras se recibe el documento
        .layer(DefaultBodyLimit::disable())
        .with_state(state)
}

/// Tarea de fondo del modulo de impresion: abre el puerto IPP (y lo cierra al apagar el modulo)
pub async fn ipp_server_loop(state: AppState) {
    LazyLock::force(&STARTED);
    let port = ipp_port();
    let app = ipp_router(state).into_make_service_with_connect_info::<SocketAddr>();
    loop {
        match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
            Ok(listener) => {
                set_listen_error(None);
                println!("[IPP] Impresoras de red en el puerto {}", port);
                if let Err(e) = axum::serve(listener, app.clone()).await {
                    set_listen_error(Some(format!("El servidor IPP se detuvo: {}", e)));
                }
            }
            Err(e) => {
                let msg = format!("No se pudo abrir el puerto {}: {}", port, e);
                eprintln!("[IPP] {}", msg);
                set_listen_error(Some(msg));
            }
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

async fn ipp_info() -> &'static str {
    "LabNAS: impresora de red IPP. Agreguela en Windows con esta misma direccion \
     (Configuracion > Impresoras > Agregar > La impresora que deseo no esta en la lista > \
     Seleccionar una impresora compartida por nombre).\n"
}

fn http_error(code: StatusCode, msg: &str) -> Response {
    (code, msg.to_string()).into_response()
}

fn ipp_response(body: Vec<u8>) -> Response {
    ([(axum::http::header::CONTENT_TYPE, "application/ipp")], body).into_response()
}

/// Respuesta de solo estado (con mensaje legible)
fn ipp_status(req: &codec::Request, code: u16, msg: &str) -> Response {
    let mut w = Writer::new(resp_version(req), code, req.request_id);
    if !msg.is_empty() {
        w.attr("status-message", &[text(msg)]);
    }
    ipp_response(w.finish())
}

fn resp_version(req: &codec::Request) -> (u8, u8) {
    match req.version.0 {
        1 | 2 => req.version,
        _ => (2, 0),
    }
}

struct Ctx {
    state: AppState,
    username: String,
    printer: String,
    printer_uri: String,
    client_ip: String,
}

async fn ipp_handle_job_uri(
    state: State<AppState>,
    conn: ConnectInfo<SocketAddr>,
    Path((token, printer, _job)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    ipp_handle(state, conn, Path((token, printer)), headers, body).await
}

async fn ipp_handle(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path((token, printer)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    // 1) Cabecera IPP (el resto del cuerpo es el documento)
    let mut stream = body.into_data_stream();
    let mut buf: Vec<u8> = Vec::new();
    let (req, offset) = loop {
        match codec::parse_request(&buf) {
            Ok(r) => break r,
            Err(ParseError::Bad(e)) => return http_error(StatusCode::BAD_REQUEST, &format!("IPP invalido: {}", e)),
            Err(ParseError::Incomplete) => {
                if buf.len() > MAX_HEADER {
                    return http_error(StatusCode::PAYLOAD_TOO_LARGE, "Cabecera IPP demasiado grande");
                }
                match stream.next().await {
                    Some(Ok(chunk)) => buf.extend_from_slice(&chunk),
                    _ => return http_error(StatusCode::BAD_REQUEST, "Peticion IPP incompleta"),
                }
            }
        }
    };
    let first = buf.split_off(offset);

    // 2) Modulo, token e impresora. Token o impresora desconocidos: 404 sin detalles
    if !state.enabled_modules.lock().await.contains("printing") {
        return http_error(StatusCode::NOT_FOUND, "No encontrado");
    }
    let Some((username, allowed)) = user_for_token(&state, &token).await else {
        return http_error(StatusCode::NOT_FOUND, "No encontrado");
    };
    // Ocultas por el admin: invisibles por IPP para todos (incluido el admin)
    if validate_printer_name(&printer).is_err()
        || !printer_exists(&printer).await
        || is_printer_hidden(&state, &printer).await
    {
        return http_error(StatusCode::NOT_FOUND, "No encontrado");
    }
    if !matches!(req.version.0, 1 | 2) {
        return ipp_status(&req, status::VERSION_NOT_SUPPORTED, "Version de IPP no soportada");
    }
    if !allowed {
        return ipp_status(&req, status::FORBIDDEN, "Tu usuario de LabNAS no tiene permiso de impresion");
    }

    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("labnas");
    let printer_uri = req
        .op_str("printer-uri")
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("ipp://{}/ipp/{}/{}", host, token, printer));
    let ctx = Ctx { state, username, printer, printer_uri, client_ip: addr.ip().to_string() };

    match req.op {
        op::GET_PRINTER_ATTRIBUTES => get_printer_attributes(&ctx, &req).await,
        op::VALIDATE_JOB => format_error(&req).unwrap_or_else(|| ipp_status(&req, status::OK, "")),
        op::PRINT_JOB => print_job(&ctx, &req, first, &mut stream).await,
        op::CREATE_JOB => create_job(&ctx, &req),
        op::SEND_DOCUMENT => send_document(&ctx, &req, first, &mut stream).await,
        op::CANCEL_JOB => cancel_ipp_job(&ctx, &req).await,
        op::GET_JOB_ATTRIBUTES => get_job_attributes(&ctx, &req).await,
        op::GET_JOBS => get_jobs(&ctx, &req),
        _ => ipp_status(&req, status::OPERATION_NOT_SUPPORTED, "Operacion no soportada"),
    }
}

// ═══════════════════════════════════════
// Tokens por usuario
// ═══════════════════════════════════════

fn token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{:02x}", b)).collect()
}

fn new_token() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("getrandom");
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Usuario dueno del token y si hoy puede imprimir (el permiso se revisa en cada trabajo)
async fn user_for_token(state: &AppState, token: &str) -> Option<(String, bool)> {
    if token.len() != 32 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hash = token_hash(token);
    db_op(&state.db, move |conn| {
        let row = conn.query_row(
            "SELECT u.username, u.role, u.perm_impresion FROM ipp_tokens t
             JOIN web_users u ON u.username = t.username WHERE t.token_hash = ?1",
            params![hash],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, bool>(2)?)),
        );
        Ok(row.ok())
    })
    .await
    .ok()
    .flatten()
    .map(|(user, role, perm)| {
        let allowed = role == "admin" || (perm && role != "pendiente");
        (user, allowed)
    })
}

#[derive(Serialize)]
pub struct IppInfo {
    port: u16,
    /// Token propio (la web arma las URL por impresora); None si no lo ha generado
    token: Option<String>,
    can_print: bool,
    listen_error: Option<String>,
}

async fn ipp_info_for(state: &AppState, session: &crate::state::SessionInfo) -> Result<IppInfo, (StatusCode, String)> {
    let u = session.username.clone();
    let stored: Option<String> = db_op(&state.db, move |conn| {
        Ok(conn
            .query_row("SELECT token_enc FROM ipp_tokens WHERE username = ?1", params![u], |r| r.get(0))
            .ok())
    })
    .await?;
    let token = stored.and_then(|s| crate::secrets::decrypt(&s).ok());
    Ok(IppInfo {
        port: ipp_port(),
        token,
        can_print: session.role == crate::models::notifications::UserRole::Admin || session.permissions.impresion,
        listen_error: LISTEN_ERROR.lock().ok().and_then(|g| g.clone()),
    })
}

pub async fn get_ipp_info(
    State(state): State<AppState>,
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
) -> Result<Json<IppInfo>, (StatusCode, String)> {
    Ok(Json(ipp_info_for(&state, &session).await?))
}

/// Crea o reemplaza el token propio (la URL anterior deja de funcionar)
pub async fn regenerate_ipp_token(
    State(state): State<AppState>,
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
) -> Result<Json<IppInfo>, (StatusCode, String)> {
    let token = new_token();
    let enc = crate::secrets::encrypt(&token).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let (u, hash) = (session.username.clone(), token_hash(&token));
    db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO ipp_tokens (username, token_hash, token_enc, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(username) DO UPDATE SET token_hash = excluded.token_hash,
                token_enc = excluded.token_enc, created_at = excluded.created_at",
            params![u, hash, enc, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| format!("DB: {}", e))?;
        Ok(())
    })
    .await?;
    state.log_activity("Impresion", "Genero su direccion de impresora de red (IPP)", &session.username).await;
    Ok(Json(ipp_info_for(&state, &session).await?))
}

pub async fn revoke_ipp_token(
    State(state): State<AppState>,
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
) -> Result<StatusCode, (StatusCode, String)> {
    let u = session.username.clone();
    db_op(&state.db, move |conn| {
        conn.execute("DELETE FROM ipp_tokens WHERE username = ?1", params![u]).map_err(|e| format!("DB: {}", e))?;
        Ok(())
    })
    .await?;
    state.log_activity("Impresion", "Revoco su direccion de impresora de red (IPP)", &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

// ═══════════════════════════════════════
// Capacidades de la impresora (desde CUPS)
// ═══════════════════════════════════════

#[cfg(test)]
pub static TEST_PRINTERS: StdMutex<Vec<String>> = StdMutex::new(Vec::new());

async fn printer_exists(printer: &str) -> bool {
    #[cfg(test)]
    if TEST_PRINTERS.lock().unwrap().iter().any(|p| p == printer) {
        return true;
    }
    match Command::new("lpstat").arg("-e").env("LANG", "C").output().await {
        Ok(o) => String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == printer),
        Err(_) => false,
    }
}

/// Tamano de papel: nombre PWG, nombres CUPS equivalentes y medidas en centesimas de mm
struct Media {
    pwg: &'static str,
    cups: &'static [&'static str],
    x: i32,
    y: i32,
}

const MEDIA: &[Media] = &[
    Media { pwg: "na_letter_8.5x11in", cups: &["Letter"], x: 21590, y: 27940 },
    Media { pwg: "na_legal_8.5x14in", cups: &["Legal"], x: 21590, y: 35560 },
    // Oficio chileno (216 x 330 mm)
    Media { pwg: "na_foolscap_8.5x13in", cups: &["FLSA", "Folio", "Oficio", "FanFoldGermanLegal"], x: 21590, y: 33020 },
    Media { pwg: "iso_a4_210x297mm", cups: &["A4"], x: 21000, y: 29700 },
    Media { pwg: "iso_a5_148x210mm", cups: &["A5"], x: 14800, y: 21000 },
    Media { pwg: "iso_a6_105x148mm", cups: &["A6"], x: 10500, y: 14800 },
    Media { pwg: "na_executive_7.25x10.5in", cups: &["Executive"], x: 18415, y: 26670 },
    Media { pwg: "iso_dl_110x220mm", cups: &["EnvDL", "DL"], x: 11000, y: 22000 },
    Media { pwg: "na_number-10_4.125x9.5in", cups: &["Env10", "ENV10"], x: 10477, y: 24130 },
    Media { pwg: "na_index-4x6_4x6in", cups: &["4x6", "4X6FULL"], x: 10160, y: 15240 },
    Media { pwg: "jpn_hagaki_100x148mm", cups: &["Postcard"], x: 10000, y: 14800 },
];
/// Margen que se anuncia (centesimas de mm)
const MARGIN: i32 = 300;

#[derive(Clone)]
struct Caps {
    color: bool,
    duplex: bool,
    media: Vec<usize>,
    default_media: usize,
    make_model: String,
    info: String,
    location: String,
}

static CAPS_CACHE: LazyLock<StdMutex<HashMap<String, (Instant, Caps)>>> = LazyLock::new(Default::default);

/// Valor de `clave=valor` en la salida de `lpoptions -p` (comillas simples opcionales)
fn lpoption_value(text: &str, key: &str) -> Option<String> {
    let needle = format!("{}=", key);
    let start = text
        .match_indices(&needle)
        .find(|(i, _)| *i == 0 || text[..*i].ends_with(char::is_whitespace))?
        .0
        + needle.len();
    let mut chars = text[start..].chars();
    let mut out = String::new();
    if text[start..].starts_with('\'') {
        chars.next();
        for c in chars.by_ref() {
            if c == '\'' {
                break;
            }
            out.push(c);
        }
        return Some(out);
    }
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            c if c.is_whitespace() => break,
            c => out.push(c),
        }
    }
    Some(out)
}

/// Capacidades a partir de `lpoptions -l` (opciones del PPD) y `lpoptions` (atributos)
fn parse_caps(options_l: &str, attrs: &str) -> Caps {
    let values_of = |key: &str| -> Option<(Vec<String>, Option<String>)> {
        let line = options_l.lines().find(|l| l.split(['/', ':']).next() == Some(key))?;
        let (_, vals) = line.split_once(':')?;
        let mut def = None;
        let list = vals
            .split_whitespace()
            .map(|v| match v.strip_prefix('*') {
                Some(d) => {
                    def = Some(d.to_string());
                    d.to_string()
                }
                None => v.to_string(),
            })
            .collect();
        Some((list, def))
    };

    let is_gray = |v: &str| {
        let v = v.to_lowercase();
        v.contains("gray") || v.contains("grey") || v.contains("mono") || v == "black" || v == "k"
    };
    let color = match values_of("ColorModel") {
        Some((vals, _)) if !vals.is_empty() => !vals.iter().all(|v| is_gray(v)),
        _ => true,
    };
    let duplex = values_of("Duplex").is_some_and(|(vals, _)| vals.iter().any(|v| v.starts_with("Duplex")));

    let find_media = |cups: &str| MEDIA.iter().position(|m| m.cups.iter().any(|c| c.eq_ignore_ascii_case(cups)));
    let (mut media, mut default_media) = (Vec::new(), None);
    if let Some((vals, def)) = values_of("PageSize") {
        for v in &vals {
            if let Some(i) = find_media(v) {
                if !media.contains(&i) {
                    media.push(i);
                }
            }
        }
        default_media = def.as_deref().and_then(find_media);
    }
    if media.is_empty() {
        media = vec![0, 1, 3]; // carta, oficio (legal) y A4
    }
    let default_media = default_media.filter(|d| media.contains(d)).unwrap_or(media[0]);

    Caps {
        color,
        duplex,
        media,
        default_media,
        make_model: lpoption_value(attrs, "printer-make-and-model").unwrap_or_default(),
        info: lpoption_value(attrs, "printer-info").unwrap_or_default(),
        location: lpoption_value(attrs, "printer-location").unwrap_or_default(),
    }
}

async fn printer_caps(printer: &str) -> Caps {
    if let Some((at, caps)) = CAPS_CACHE.lock().ok().and_then(|c| c.get(printer).cloned()) {
        if at.elapsed() < Duration::from_secs(60) {
            return caps;
        }
    }
    let run = |args: Vec<&str>| {
        let mut cmd = Command::new("lpoptions");
        cmd.args(args).env("LANG", "C");
        async move {
            match tokio::time::timeout(Duration::from_secs(5), cmd.output()).await {
                Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout).to_string(),
                _ => String::new(),
            }
        }
    };
    let (opts, attrs) = tokio::join!(run(vec!["-p", printer, "-l"]), run(vec!["-p", printer]));
    let caps = parse_caps(&opts, &attrs);
    if let Ok(mut c) = CAPS_CACHE.lock() {
        c.insert(printer.to_string(), (Instant::now(), caps.clone()));
    }
    caps
}

fn media_col(m: &Media) -> Value {
    Value::Collection(vec![
        (
            "media-size".into(),
            vec![Value::Collection(vec![
                ("x-dimension".into(), vec![Value::Int(m.x)]),
                ("y-dimension".into(), vec![Value::Int(m.y)]),
            ])],
        ),
        ("media-top-margin".into(), vec![Value::Int(MARGIN)]),
        ("media-bottom-margin".into(), vec![Value::Int(MARGIN)]),
        ("media-left-margin".into(), vec![Value::Int(MARGIN)]),
        ("media-right-margin".into(), vec![Value::Int(MARGIN)]),
        ("media-source".into(), vec![kw("auto")]),
        ("media-type".into(), vec![kw("stationery")]),
    ])
}

/// UUID estable por impresora (Windows lo usa para reconocerla)
fn printer_uuid(printer: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(format!("labnas-ipp:{}", printer).as_bytes());
    let x: String = h.iter().take(16).map(|b| format!("{:02x}", b)).collect();
    format!("urn:uuid:{}-{}-4{}-a{}-{}", &x[0..8], &x[8..12], &x[13..16], &x[17..20], &x[20..32])
}

const DOCUMENT_FORMATS: &[&str] =
    &["application/pdf", "image/pwg-raster", "image/jpeg", "application/postscript", "application/octet-stream"];
const OPERATIONS: &[u16] = &[
    op::PRINT_JOB,
    op::VALIDATE_JOB,
    op::CREATE_JOB,
    op::SEND_DOCUMENT,
    op::CANCEL_JOB,
    op::GET_JOB_ATTRIBUTES,
    op::GET_JOBS,
    op::GET_PRINTER_ATTRIBUTES,
];

fn more_info_uri(printer_uri: &str) -> String {
    match printer_uri.split_once("://") {
        Some((scheme, rest)) if scheme == "ipp" || scheme == "http" => format!("http://{}", rest),
        Some((_, rest)) => format!("https://{}", rest),
        None => printer_uri.to_string(),
    }
}

fn printer_attributes(ctx: &Ctx, caps: &Caps) -> Vec<(&'static str, Vec<Value>)> {
    let media: Vec<&Media> = caps.media.iter().map(|&i| &MEDIA[i]).collect();
    let def = &MEDIA[caps.default_media];
    let lang = |s: &str| Value::Str(tag::LANGUAGE, s.to_string());
    let charset = Value::Str(tag::CHARSET, "utf-8".into());
    let mut sides = vec![kw("one-sided")];
    if caps.duplex {
        sides.extend([kw("two-sided-long-edge"), kw("two-sided-short-edge")]);
    }
    let color_modes = if caps.color { vec![kw("auto"), kw("color"), kw("monochrome")] } else { vec![kw("monochrome")] };
    let raster_types = if caps.color { vec![kw("sgray_8"), kw("srgb_8")] } else { vec![kw("sgray_8")] };
    let queued = JOBS.lock().map(|j| j.map.values().filter(|x| x.printer == ctx.printer && x.state < 7).count()).unwrap_or(0);
    let info = if caps.info.is_empty() { format!("{} (LabNAS)", ctx.printer.replace('_', " ")) } else { caps.info.clone() };
    let model = if caps.make_model.is_empty() { ctx.printer.replace('_', " ") } else { caps.make_model.clone() };
    let res = |d: i32| Value::Resolution(d, d, 3);

    vec![
        ("printer-uri-supported", vec![uri(&ctx.printer_uri)]),
        ("uri-security-supported", vec![kw("none")]),
        ("uri-authentication-supported", vec![kw("none")]),
        ("printer-name", vec![name(&ctx.printer)]),
        ("printer-info", vec![text(&info)]),
        ("printer-location", vec![text(&caps.location)]),
        ("printer-make-and-model", vec![text(&model)]),
        ("printer-device-id", vec![text(&format!("MFG:LabNAS;MDL:{};CMD:PDF,PWGRaster,JPEG,POSTSCRIPT;", ctx.printer))]),
        ("printer-uuid", vec![uri(&printer_uuid(&ctx.printer))]),
        // La misma direccion por HTTP responde con instrucciones
        ("printer-more-info", vec![uri(&more_info_uri(&ctx.printer_uri))]),
        ("printer-state", vec![Value::Enum(3)]),
        ("printer-state-reasons", vec![kw("none")]),
        ("printer-state-message", vec![text("")]),
        ("printer-is-accepting-jobs", vec![Value::Bool(true)]),
        ("queued-job-count", vec![Value::Int(queued as i32)]),
        ("printer-up-time", vec![Value::Int(uptime())]),
        ("printer-kind", vec![kw("document")]),
        ("ipp-versions-supported", vec![kw("1.1"), kw("2.0")]),
        ("ipp-features-supported", vec![kw("ipp-everywhere")]),
        // Windows elige su driver generico (IPP Class Driver) con estos atributos
        ("mopria-certified", vec![text("1.3")]),
        ("operations-supported", OPERATIONS.iter().map(|&o| Value::Enum(o as i32)).collect()),
        ("multiple-document-jobs-supported", vec![Value::Bool(false)]),
        ("multiple-operation-time-out", vec![Value::Int(120)]),
        ("charset-configured", vec![charset.clone()]),
        ("charset-supported", vec![charset]),
        ("natural-language-configured", vec![lang("es")]),
        ("generated-natural-language-supported", vec![lang("es"), lang("en")]),
        ("document-format-default", vec![mime("application/octet-stream")]),
        ("document-format-supported", DOCUMENT_FORMATS.iter().map(|f| mime(f)).collect()),
        ("pdl-override-supported", vec![kw("attempted")]),
        ("compression-supported", vec![kw("none")]),
        ("job-ids-supported", vec![Value::Bool(true)]),
        ("which-jobs-supported", vec![kw("completed"), kw("not-completed")]),
        (
            "job-creation-attributes-supported",
            ["copies", "media", "media-col", "sides", "print-color-mode", "print-quality", "orientation-requested"]
                .iter()
                .map(|s| kw(s))
                .collect(),
        ),
        ("copies-default", vec![Value::Int(1)]),
        ("copies-supported", vec![Value::Range(1, 100)]),
        ("media-default", vec![kw(def.pwg)]),
        ("media-ready", vec![kw(def.pwg)]),
        ("media-supported", media.iter().map(|m| kw(m.pwg)).collect()),
        ("media-col-default", vec![media_col(def)]),
        ("media-col-ready", vec![media_col(def)]),
        ("media-col-database", media.iter().map(|m| media_col(m)).collect()),
        (
            "media-col-supported",
            ["media-size", "media-top-margin", "media-bottom-margin", "media-left-margin", "media-right-margin", "media-source", "media-type"]
                .iter()
                .map(|s| kw(s))
                .collect(),
        ),
        (
            "media-size-supported",
            media
                .iter()
                .map(|m| {
                    Value::Collection(vec![
                        ("x-dimension".into(), vec![Value::Int(m.x)]),
                        ("y-dimension".into(), vec![Value::Int(m.y)]),
                    ])
                })
                .collect(),
        ),
        ("media-top-margin-supported", vec![Value::Int(MARGIN)]),
        ("media-bottom-margin-supported", vec![Value::Int(MARGIN)]),
        ("media-left-margin-supported", vec![Value::Int(MARGIN)]),
        ("media-right-margin-supported", vec![Value::Int(MARGIN)]),
        ("media-source-supported", vec![kw("auto")]),
        ("media-type-supported", vec![kw("stationery")]),
        ("sides-default", vec![kw("one-sided")]),
        ("sides-supported", sides),
        ("color-supported", vec![Value::Bool(caps.color)]),
        ("print-color-mode-default", vec![kw(if caps.color { "auto" } else { "monochrome" })]),
        ("print-color-mode-supported", color_modes),
        ("print-quality-default", vec![Value::Enum(4)]),
        ("print-quality-supported", vec![Value::Enum(3), Value::Enum(4), Value::Enum(5)]),
        ("printer-resolution-default", vec![res(300)]),
        ("printer-resolution-supported", vec![res(300), res(600)]),
        ("pwg-raster-document-resolution-supported", vec![res(300), res(600)]),
        ("pwg-raster-document-type-supported", raster_types),
        ("pwg-raster-document-sheet-back", vec![kw("normal")]),
        ("orientation-requested-default", vec![Value::Enum(3)]),
        ("orientation-requested-supported", vec![Value::Enum(3), Value::Enum(4), Value::Enum(5), Value::Enum(6)]),
        ("finishings-default", vec![Value::Enum(3)]),
        ("finishings-supported", vec![Value::Enum(3)]),
        ("page-ranges-supported", vec![Value::Bool(false)]),
    ]
}

async fn get_printer_attributes(ctx: &Ctx, req: &codec::Request) -> Response {
    let caps = printer_caps(&ctx.printer).await;
    let requested: Vec<&str> = req
        .op_attr("requested-attributes")
        .map(|a| a.values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let everything =
        requested.is_empty() || requested.iter().any(|r| matches!(*r, "all" | "printer-description" | "job-template"));

    let mut w = Writer::new(resp_version(req), status::OK, req.request_id);
    w.group(tag::PRINTER);
    for (attr, values) in printer_attributes(ctx, &caps) {
        // media-col-database es grande: solo si se pide por nombre (RFC 8011)
        let listed = requested.contains(&attr);
        if listed || (everything && attr != "media-col-database") {
            w.attr(attr, &values);
        }
    }
    ipp_response(w.finish())
}

// ═══════════════════════════════════════
// Trabajos
// ═══════════════════════════════════════

const PENDING: i32 = 3;
const PROCESSING: i32 = 5;
const CANCELED: i32 = 7;
const ABORTED: i32 = 8;
const COMPLETED: i32 = 9;

struct IppJob {
    username: String,
    printer: String,
    name: String,
    state: i32,
    message: String,
    created: i32,
    completed: Option<i32>,
    cups_id: Option<String>,
    /// Opciones de Create-Job, para cuando llegue el documento
    options: JobOptions,
    format: Option<String>,
    born: Instant,
}

struct Jobs {
    next: i32,
    map: BTreeMap<i32, IppJob>,
}

static JOBS: LazyLock<StdMutex<Jobs>> = LazyLock::new(|| {
    // Ids distintos entre reinicios (Windows recuerda los trabajos por id)
    let start = (chrono::Utc::now().timestamp() % 1_000_000) as i32 * 10 + 1;
    StdMutex::new(Jobs { next: start, map: BTreeMap::new() })
});

impl Jobs {
    /// Olvida trabajos viejos; un Create-Job sin documento caduca a los 10 minutos
    fn prune(&mut self) {
        let now = uptime();
        for j in self.map.values_mut() {
            if j.state == PENDING && j.born.elapsed() > Duration::from_secs(600) {
                j.state = ABORTED;
                j.message = "No llego el documento".into();
                j.completed = Some(now);
            }
        }
        self.map.retain(|_, j| j.state < CANCELED || j.born.elapsed() < Duration::from_secs(3600));
        while self.map.len() > 1000 {
            let Some(first) = self.map.keys().next().copied() else { break };
            self.map.remove(&first);
        }
    }
}

#[derive(Clone, Default)]
struct JobOptions {
    copies: Option<String>,
    lp: HashMap<String, String>,
}

/// Opciones del trabajo que entiende `lp`
fn job_options(req: &codec::Request) -> JobOptions {
    let mut o = JobOptions::default();
    if let Some(n) = req.job_value("copies").and_then(Value::as_int) {
        if (1..=100).contains(&n) {
            o.copies = Some(n.to_string());
        }
    }
    if let Some(s) = req.job_value("sides").and_then(Value::as_str) {
        if matches!(s, "one-sided" | "two-sided-long-edge" | "two-sided-short-edge") {
            o.lp.insert("sides".into(), s.into());
        }
    }
    let media = req
        .job_value("media")
        .and_then(Value::as_str)
        .and_then(|m| MEDIA.iter().find(|x| x.pwg == m))
        .or_else(|| media_from_col(req.job_value("media-col")?));
    if let Some(m) = media {
        o.lp.insert("media".into(), m.pwg.into());
    }
    if let Some(s) = req.job_value("print-color-mode").and_then(Value::as_str) {
        if matches!(s, "monochrome" | "color") {
            o.lp.insert("print-color-mode".into(), s.into());
        }
    }
    if let Some(q) = req.job_value("print-quality").and_then(Value::as_int) {
        if (3..=5).contains(&q) {
            o.lp.insert("print-quality".into(), q.to_string());
        }
    }
    o
}

/// Tamano pedido como coleccion (media-col > media-size), con 1 mm de tolerancia
fn media_from_col(v: &Value) -> Option<&'static Media> {
    let Value::Collection(members) = v else { return None };
    let size = members.iter().find(|(n, _)| n == "media-size")?.1.first()?;
    let Value::Collection(dims) = size else { return None };
    let dim = |key: &str| dims.iter().find(|(n, _)| n == key)?.1.first()?.as_int();
    let (x, y) = (dim("x-dimension")?, dim("y-dimension")?);
    MEDIA.iter().find(|m| (m.x - x).abs() <= 100 && (m.y - y).abs() <= 100)
}

/// Respuesta de error si el formato declarado no se acepta
fn format_error(req: &codec::Request) -> Option<Response> {
    let f = req.op_str("document-format").filter(|f| !DOCUMENT_FORMATS.contains(f))?;
    Some(ipp_status(req, status::FORMAT_NOT_SUPPORTED, &format!("Formato no soportado: {}", f)))
}

/// Extension del documento segun su contenido (o el formato declarado)
fn detect_format(declared: Option<&str>, head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"%PDF") {
        return Some("pdf");
    }
    if head.starts_with(b"RaS2") {
        return Some("pwg");
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if head.starts_with(b"%!") {
        return Some("ps");
    }
    match declared {
        Some("application/pdf") => Some("pdf"),
        Some("image/pwg-raster") => Some("pwg"),
        Some("image/jpeg") => Some("jpg"),
        Some("application/postscript") => Some("ps"),
        _ => None,
    }
}

/// Titulo legible y seguro para el archivo (CUPS lo muestra en la cola)
fn safe_title(job_name: Option<&str>) -> String {
    let t: String = job_name
        .unwrap_or("")
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') { c } else { '_' })
        .take(60)
        .collect();
    let t = t.trim().trim_matches('.').to_string();
    if t.is_empty() { "documento".into() } else { t }
}

fn job_uri(ctx: &Ctx, id: i32) -> String {
    format!("{}/{}", ctx.printer_uri.trim_end_matches('/'), id)
}

fn write_job_attrs(w: &mut Writer, ctx: &Ctx, id: i32, j: &IppJob) {
    let reason = match j.state {
        PENDING => "none",
        PROCESSING => "job-printing",
        CANCELED => "job-canceled-by-user",
        ABORTED => "aborted-by-system",
        _ => "job-completed-successfully",
    };
    w.attr("job-id", &[Value::Int(id)]);
    w.attr("job-uri", &[uri(&job_uri(ctx, id))]);
    w.attr("job-printer-uri", &[uri(&ctx.printer_uri)]);
    w.attr("job-name", &[name(&j.name)]);
    w.attr("job-originating-user-name", &[name(&j.username)]);
    w.attr("job-state", &[Value::Enum(j.state)]);
    w.attr("job-state-reasons", &[kw(reason)]);
    if !j.message.is_empty() {
        w.attr("job-state-message", &[text(&j.message)]);
    }
    w.attr("job-printer-up-time", &[Value::Int(uptime())]);
    w.attr("time-at-creation", &[Value::Int(j.created)]);
    match j.completed {
        Some(t) => w.attr("time-at-completed", &[Value::Int(t)]),
        None => w.attr("time-at-completed", &[Value::Other(tag::NO_VALUE, Vec::new())]),
    }
}

fn job_response(ctx: &Ctx, req: &codec::Request, code: u16, msg: &str, id: i32) -> Response {
    let mut w = Writer::new(resp_version(req), code, req.request_id);
    if !msg.is_empty() {
        w.attr("status-message", &[text(msg)]);
    }
    if let Ok(jobs) = JOBS.lock() {
        if let Some(j) = jobs.map.get(&id) {
            w.group(tag::JOB);
            write_job_attrs(&mut w, ctx, id, j);
        }
    }
    ipp_response(w.finish())
}

fn register_job(ctx: &Ctx, req: &codec::Request, state: i32) -> i32 {
    let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    jobs.prune();
    let id = jobs.next;
    jobs.next += 1;
    jobs.map.insert(
        id,
        IppJob {
            username: ctx.username.clone(),
            printer: ctx.printer.clone(),
            name: safe_title(req.op_str("job-name")),
            state,
            message: String::new(),
            created: uptime(),
            completed: None,
            cups_id: None,
            options: job_options(req),
            format: req.op_str("document-format").map(str::to_string),
            born: Instant::now(),
        },
    );
    id
}

fn finish_job(id: i32, state: i32, message: String, cups_id: Option<String>) {
    if let Ok(mut jobs) = JOBS.lock() {
        if let Some(j) = jobs.map.get_mut(&id) {
            j.state = state;
            j.message = message;
            j.cups_id = cups_id;
            if state >= CANCELED {
                j.completed = Some(uptime());
            }
        }
    }
}

type DataStream = axum::body::BodyDataStream;

/// Guarda el documento (lo ya leido mas el resto del cuerpo) con limite de tamano
async fn save_document(first: Vec<u8>, stream: &mut DataStream, path: &std::path::Path) -> Result<u64, (u16, String)> {
    let io_err = |e: std::io::Error| (status::INTERNAL_ERROR, format!("No se pudo guardar el documento: {}", e));
    let mut file = tokio::fs::File::create(path).await.map_err(io_err)?;
    let mut total = first.len() as u64;
    file.write_all(&first).await.map_err(io_err)?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| (status::BAD_REQUEST, format!("Documento incompleto: {}", e)))?;
        total += chunk.len() as u64;
        if total > MAX_DOCUMENT {
            return Err((status::TOO_LARGE, "Documento demasiado grande".into()));
        }
        file.write_all(&chunk).await.map_err(io_err)?;
    }
    file.flush().await.map_err(io_err)?;
    Ok(total)
}

/// "request id is HP-123 (1 file(s))" -> "HP-123" (la frase cambia con el idioma del sistema)
fn parse_cups_id(printer: &str, lp_stdout: &str) -> Option<String> {
    let prefix = format!("{}-", printer);
    lp_stdout
        .split_whitespace()
        .find(|w| w.strip_prefix(&prefix).is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())))
        .map(str::to_string)
}

/// Recibe el documento del trabajo `id` y lo manda a CUPS
async fn receive_and_print(ctx: &Ctx, req: &codec::Request, id: i32, first: Vec<u8>, stream: &mut DataStream) -> Response {
    let (options, declared, title) = {
        let jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
        match jobs.map.get(&id) {
            Some(j) => (j.options.clone(), j.format.clone(), j.name.clone()),
            None => return ipp_status(req, status::NOT_FOUND, "Trabajo no encontrado"),
        }
    };
    let dir = std::env::temp_dir().join(format!("labnas-ipp-{}", uuid::Uuid::new_v4()));
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        finish_job(id, ABORTED, e.to_string(), None);
        return job_response(ctx, req, status::INTERNAL_ERROR, "No se pudo crear el temporal", id);
    }
    let result = print_from_dir(ctx, &dir, &options, declared.as_deref(), &title, first, stream).await;
    let _ = tokio::fs::remove_dir_all(&dir).await;

    match result {
        Ok(cups_id) => {
            finish_job(id, PROCESSING, String::new(), cups_id);
            job_response(ctx, req, status::OK, "", id)
        }
        Err((code, msg)) => {
            finish_job(id, ABORTED, msg.clone(), None);
            job_response(ctx, req, code, &msg, id)
        }
    }
}

async fn print_from_dir(
    ctx: &Ctx,
    dir: &std::path::Path,
    options: &JobOptions,
    declared: Option<&str>,
    title: &str,
    first: Vec<u8>,
    stream: &mut DataStream,
) -> Result<Option<String>, (u16, String)> {
    let raw = dir.join("documento.bin");
    let size = save_document(first, stream, &raw).await?;
    if size == 0 {
        return Err((status::BAD_REQUEST, "Documento vacio".into()));
    }
    let mut head = [0u8; 8];
    let n = {
        use tokio::io::AsyncReadExt;
        let mut f = tokio::fs::File::open(&raw).await.map_err(|e| (status::INTERNAL_ERROR, e.to_string()))?;
        f.read(&mut head).await.unwrap_or(0)
    };
    let Some(ext) = detect_format(declared, &head[..n]) else {
        return Err((status::FORMAT_NOT_SUPPORTED, "Formato de documento no soportado".into()));
    };
    let file = dir.join(format!("{}.{}", title, ext));
    tokio::fs::rename(&raw, &file).await.map_err(|e| (status::INTERNAL_ERROR, e.to_string()))?;
    let file_str = file.to_string_lossy().to_string();

    let (_, out) = run_lp_command(&ctx.printer, &file_str, options.copies.clone(), None, &options.lp)
        .await
        .map_err(|(_, e)| (status::INTERNAL_ERROR, e))?;
    let details = format!("{}.{} en {} (impresora de red, desde {})", title, ext, ctx.printer, ctx.client_ip);
    ctx.state.log_activity("Impresion", &details, &ctx.username).await;
    track_print_stats(&ctx.state, &ctx.printer, &file_str, &options.copies, &None, &options.lp, &ctx.username).await;
    Ok(parse_cups_id(&ctx.printer, &out))
}

async fn print_job(ctx: &Ctx, req: &codec::Request, first: Vec<u8>, stream: &mut DataStream) -> Response {
    if let Some(r) = format_error(req) {
        return r;
    }
    let id = register_job(ctx, req, PROCESSING);
    receive_and_print(ctx, req, id, first, stream).await
}

fn create_job(ctx: &Ctx, req: &codec::Request) -> Response {
    if let Some(r) = format_error(req) {
        return r;
    }
    let id = register_job(ctx, req, PENDING);
    job_response(ctx, req, status::OK, "", id)
}

fn request_job_id(req: &codec::Request) -> Option<i32> {
    req.op_int("job-id")
        .or_else(|| req.op_str("job-uri")?.rsplit('/').next()?.parse().ok())
}

/// El trabajo existe y es de este usuario en esta impresora
fn own_job(ctx: &Ctx, id: i32) -> Option<i32> {
    let jobs = JOBS.lock().ok()?;
    let j = jobs.map.get(&id)?;
    (j.username == ctx.username && j.printer == ctx.printer).then_some(j.state)
}

async fn send_document(ctx: &Ctx, req: &codec::Request, first: Vec<u8>, stream: &mut DataStream) -> Response {
    let Some(id) = request_job_id(req) else {
        return ipp_status(req, status::BAD_REQUEST, "Falta job-id");
    };
    match own_job(ctx, id) {
        None => return ipp_status(req, status::NOT_FOUND, "Trabajo no encontrado"),
        Some(PENDING) => {}
        Some(_) => return ipp_status(req, status::NOT_POSSIBLE, "El trabajo ya tiene su documento"),
    }
    if let Some(f) = req.op_str("document-format") {
        if !DOCUMENT_FORMATS.contains(&f) {
            return ipp_status(req, status::FORMAT_NOT_SUPPORTED, &format!("Formato no soportado: {}", f));
        }
        if let Ok(mut jobs) = JOBS.lock() {
            if let Some(j) = jobs.map.get_mut(&id) {
                j.format = Some(f.to_string());
            }
        }
    }
    if let Some(n) = req.op_str("document-name").filter(|n| !n.is_empty()) {
        if let Ok(mut jobs) = JOBS.lock() {
            if let Some(j) = jobs.map.get_mut(&id) {
                if j.name == "documento" {
                    j.name = safe_title(Some(n));
                }
            }
        }
    }
    let last = req.op_attr("last-document").and_then(|a| a.values.first()).and_then(Value::as_bool).unwrap_or(true);
    if first.is_empty() && !last {
        // Nada que imprimir todavia: queda esperando el siguiente Send-Document
        return job_response(ctx, req, status::OK, "", id);
    }
    if let Ok(mut jobs) = JOBS.lock() {
        if let Some(j) = jobs.map.get_mut(&id) {
            j.state = PROCESSING;
        }
    }
    receive_and_print(ctx, req, id, first, stream).await
}

/// ¿Sigue el trabajo en la cola de CUPS?
async fn cups_job_pending(printer: &str, cups_id: &str) -> bool {
    match Command::new("lpstat").args(["-W", "not-completed", "-o", printer]).env("LANG", "C").output().await {
        Ok(o) => String::from_utf8_lossy(&o.stdout).lines().any(|l| l.split_whitespace().next() == Some(cups_id)),
        Err(_) => false,
    }
}

/// Un trabajo entregado a CUPS pasa a completado cuando sale de su cola
async fn refresh_state(id: i32) {
    let pending = JOBS.lock().ok().and_then(|j| {
        let job = j.map.get(&id)?;
        (job.state == PROCESSING).then(|| (job.printer.clone(), job.cups_id.clone()))
    });
    let Some((printer, cups_id)) = pending else { return };
    let still = match cups_id {
        Some(cid) => cups_job_pending(&printer, &cid).await,
        None => false,
    };
    if !still {
        finish_job(id, COMPLETED, String::new(), None);
    }
}

async fn get_job_attributes(ctx: &Ctx, req: &codec::Request) -> Response {
    let Some(id) = request_job_id(req) else {
        return ipp_status(req, status::BAD_REQUEST, "Falta job-id");
    };
    if own_job(ctx, id).is_none() {
        return ipp_status(req, status::NOT_FOUND, "Trabajo no encontrado");
    }
    refresh_state(id).await;
    job_response(ctx, req, status::OK, "", id)
}

fn get_jobs(ctx: &Ctx, req: &codec::Request) -> Response {
    let completed = req.op_str("which-jobs") == Some("completed");
    let limit = req.op_int("limit").filter(|n| *n > 0).unwrap_or(100) as usize;
    let mut w = Writer::new(resp_version(req), status::OK, req.request_id);
    if let Ok(mut jobs) = JOBS.lock() {
        jobs.prune();
        let mine = jobs
            .map
            .iter()
            .filter(|(_, j)| j.username == ctx.username && j.printer == ctx.printer)
            .filter(|(_, j)| (j.state >= CANCELED) == completed)
            .rev()
            .take(limit);
        for (id, j) in mine {
            w.group(tag::JOB);
            write_job_attrs(&mut w, ctx, *id, j);
        }
    }
    ipp_response(w.finish())
}

async fn cancel_ipp_job(ctx: &Ctx, req: &codec::Request) -> Response {
    let Some(id) = request_job_id(req) else {
        return ipp_status(req, status::BAD_REQUEST, "Falta job-id");
    };
    let Some(state) = own_job(ctx, id) else {
        return ipp_status(req, status::NOT_FOUND, "Trabajo no encontrado");
    };
    let cups_id = JOBS.lock().ok().and_then(|j| j.map.get(&id).and_then(|x| x.cups_id.clone()));
    match (state, cups_id) {
        (PENDING, _) => {}
        (PROCESSING, Some(cid)) if cups_job_pending(&ctx.printer, &cid).await => {
            let ok = Command::new("cancel").arg(&cid).output().await.map(|o| o.status.success()).unwrap_or(false);
            if !ok {
                return ipp_status(req, status::NOT_POSSIBLE, "CUPS no pudo cancelar el trabajo");
            }
        }
        _ => return ipp_status(req, status::NOT_POSSIBLE, "El trabajo ya termino"),
    }
    finish_job(id, CANCELED, String::new(), None);
    ctx.state.log_activity("Impresion", &format!("Cancelo un trabajo de red en {}", ctx.printer), &ctx.username).await;
    ipp_status(req, status::OK, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacidades_desde_lpoptions() {
        let hp = "PageSize/Media Size: A6 A5 *Letter A4 FLSA Legal EnvDL Custom.WIDTHxHEIGHT\nColorModel/Output Mode: *Gray\n";
        let attrs = "copies=1 printer-make-and-model='HP LaserJet Professional P1102w' printer-info=HP\\ Laser printer-location=''";
        let c = parse_caps(hp, attrs);
        assert!(!c.color);
        assert!(!c.duplex);
        assert_eq!(MEDIA[c.default_media].pwg, "na_letter_8.5x11in");
        let names: Vec<_> = c.media.iter().map(|&i| MEDIA[i].pwg).collect();
        assert!(names.contains(&"na_foolscap_8.5x13in") && names.contains(&"iso_a4_210x297mm"));
        assert_eq!(c.make_model, "HP LaserJet Professional P1102w");
        assert_eq!(c.info, "HP Laser");

        let epson = "Ink/Grayscale: *COLOR MONO\nPageSize/Media Size: *A4 Letter\nDuplex/2-Sided: *None DuplexNoTumble DuplexTumble\n";
        let c = parse_caps(epson, "");
        assert!(c.color && c.duplex);
        assert_eq!(MEDIA[c.default_media].pwg, "iso_a4_210x297mm");
    }

    #[test]
    fn formato_por_contenido() {
        assert_eq!(detect_format(None, b"%PDF-1.4"), Some("pdf"));
        assert_eq!(detect_format(Some("application/octet-stream"), b"RaS2PwgR"), Some("pwg"));
        assert_eq!(detect_format(Some("image/jpeg"), b"xxxx"), Some("jpg"));
        assert_eq!(detect_format(Some("application/octet-stream"), b"UNIRAST\0"), None);
    }

    #[test]
    fn titulo_seguro() {
        assert_eq!(safe_title(Some("../../etc/passwd")), "_.._etc_passwd");
        assert_eq!(safe_title(Some("Informe final.docx")), "Informe final.docx");
        assert_eq!(safe_title(None), "documento");
        assert_eq!(safe_title(Some("...")), "documento");
    }

    #[test]
    fn id_de_cups() {
        assert_eq!(parse_cups_id("HP-LaserJet", "request id is HP-LaserJet-12 (1 file(s))\n").as_deref(), Some("HP-LaserJet-12"));
        assert_eq!(parse_cups_id("HP", "la id solicitada es HP-7 (1 archivo(s))").as_deref(), Some("HP-7"));
        assert_eq!(parse_cups_id("HP", "nada HP-x"), None);
    }

    #[test]
    fn uuid_estable() {
        let a = printer_uuid("HP");
        assert_eq!(a, printer_uuid("HP"));
        assert_ne!(a, printer_uuid("Epson"));
        assert_eq!(a.len(), "urn:uuid:".len() + 36);
    }
}
