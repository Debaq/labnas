//! WebDAV en /dav/ para montar el NAS como unidad (Windows, macOS, Linux).
//!
//! - Cada carpeta accesible que el usuario puede leer es una coleccion: /dav/<nombre>/
//! - Autenticacion Basic con el usuario y contraseña de la web (cache de 5 min para
//!   no correr bcrypt en cada request; bloqueo por intentos fallidos compartido)
//! - Mismos permisos por carpeta que la web; DELETE mueve a la papelera
//! - Desactivado por defecto (setting `webdav_enabled`)

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::models::notifications::{UserPermissions, UserRole};
use crate::state::{AppState, SessionInfo};
use crate::storage::{Op, Storage};

pub const PREFIX: &str = "/dav";
const SETTING: &str = "webdav_enabled";
const CRED_TTL: Duration = Duration::from_secs(5 * 60);

/// sha256(usuario \0 contraseña) -> (sesion, validada en)
static CREDENTIALS: LazyLock<Mutex<HashMap<String, (SessionInfo, Instant)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn plain(status: StatusCode, msg: &str) -> Response {
    (status, msg.to_string()).into_response()
}

fn unauthorized() -> Response {
    let mut r = plain(StatusCode::UNAUTHORIZED, "Autenticacion requerida");
    r.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Basic realm=\"LabNAS\", charset=\"UTF-8\""));
    r
}

// ─── Autenticacion ───

fn basic_credentials(req: &Request) -> Option<(String, String)> {
    let v = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    let b64 = v.strip_prefix("Basic ").or_else(|| v.strip_prefix("basic "))?;
    let decoded = String::from_utf8(B64.decode(b64.trim()).ok()?).ok()?;
    let (user, pass) = decoded.split_once(':')?;
    Some((user.trim().to_lowercase(), pass.to_string()))
}

/// Error como respuesta HTTP (en caja: `Response` es grande para un `Result`)
type DavError = Box<Response>;

async fn authenticate(state: &AppState, user: &str, pass: &str) -> Result<SessionInfo, DavError> {
    let key: String = Sha256::digest(format!("{}\0{}", user, pass)).iter().map(|b| format!("{:02x}", b)).collect();
    if let Ok(cache) = CREDENTIALS.lock() {
        if let Some((s, at)) = cache.get(&key) {
            if at.elapsed() < CRED_TTL {
                return Ok(s.clone());
            }
        }
    }

    // mismo bloqueo que el login web
    {
        let failures = state.login_failures.lock().await;
        if let Some(f) = failures.get(user) {
            if f.count >= 5 && f.last.elapsed() < Duration::from_secs(300) {
                return Err(Box::new(plain(StatusCode::TOO_MANY_REQUESTS, "Demasiados intentos fallidos")));
            }
        }
    }

    let u = user.to_string();
    let row = crate::db::db_op(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT password_hash, role, perm_terminal, perm_impresion, perm_archivos_escritura FROM web_users WHERE username = ?1",
            [&u],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, bool>(2)?, r.get::<_, bool>(3)?, r.get::<_, bool>(4)?)),
        )
        .optional()
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|(s, e)| Box::new(plain(s, &e)))?;

    let ok = match &row {
        Some((hash, ..)) => {
            let (p, h) = (pass.to_string(), hash.clone());
            tokio::task::spawn_blocking(move || bcrypt::verify(&p, &h).unwrap_or(false)).await.unwrap_or(false)
        }
        None => false,
    };
    let Some((_, role, t, i, a)) = row.filter(|_| ok) else {
        let mut failures = state.login_failures.lock().await;
        let f = failures.entry(user.to_string()).or_insert(crate::state::LoginFailures { count: 0, last: Instant::now() });
        f.count += 1;
        f.last = Instant::now();
        return Err(Box::new(unauthorized()));
    };
    state.login_failures.lock().await.remove(user);

    let session = SessionInfo {
        username: user.to_string(),
        role: crate::handlers::auth::role_from_str(&role),
        permissions: UserPermissions { terminal: t, impresion: i, archivos_escritura: a },
        created_at: crate::state::now_unix(),
    };
    if let Ok(mut cache) = CREDENTIALS.lock() {
        cache.retain(|_, (_, at)| at.elapsed() < CRED_TTL);
        cache.insert(key, (session.clone(), Instant::now()));
    }
    Ok(session)
}

// ─── Montajes ───

/// (nombre en /dav/, raiz) de las carpetas que el usuario puede leer
fn mounts(st: &Storage, s: &SessionInfo) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for root in st.visible_roots(s) {
        let base = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "raiz".to_string());
        let mut name = base.clone();
        let mut n = 2;
        while out.iter().any(|(m, _)| *m == name) {
            name = format!("{}-{}", base, n);
            n += 1;
        }
        out.push((name, root));
    }
    out
}

fn method_op(m: &Method) -> Op {
    match m.as_str() {
        "GET" | "HEAD" | "OPTIONS" | "PROPFIND" => Op::Read,
        _ => Op::Write,
    }
}

/// Ruta de una URL /dav/<nombre>/<resto> ya decodificada: (nombre, resto)
fn split_dav_path(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix(PREFIX)?;
    let rest = rest.trim_start_matches('/');
    let decoded = urlencoding::decode(rest).ok()?.to_string();
    let (name, tail) = decoded.split_once('/').unwrap_or((decoded.as_str(), ""));
    Some((name.to_string(), tail.to_string()))
}

/// Destino real dentro de la raiz (sin `..`, sin escapar por symlinks, sin ~/.labnas)
fn target_in_root(root: &Path, tail: &str) -> Result<PathBuf, DavError> {
    let rel = Path::new(tail);
    if rel.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        return Err(Box::new(plain(StatusCode::FORBIDDEN, "Ruta invalida")));
    }
    let target = crate::storage::canonicalize_lenient(&root.join(rel)).map_err(|(s, e)| Box::new(plain(s, &e)))?;
    let data_dir = std::fs::canonicalize(crate::db::data_dir()).unwrap_or_else(|_| crate::db::data_dir());
    if !target.starts_with(root) || target.starts_with(&data_dir) {
        return Err(Box::new(plain(StatusCode::FORBIDDEN, "Ruta protegida")));
    }
    Ok(target)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// PROPFIND / OPTIONS de /dav/ (coleccion virtual con las carpetas del usuario)
fn top_level(method: &Method, depth0: bool, mounts: &[(String, PathBuf)]) -> Response {
    let mut resp = match method.as_str() {
        "OPTIONS" => StatusCode::OK.into_response(),
        "PROPFIND" => {
            let entry = |href: &str, name: &str| {
                format!(
                    "<D:response><D:href>{}</D:href><D:propstat><D:prop><D:displayname>{}</D:displayname>\
                     <D:resourcetype><D:collection/></D:resourcetype></D:prop><D:status>HTTP/1.1 200 OK</D:status>\
                     </D:propstat></D:response>",
                    xml_escape(href),
                    xml_escape(name)
                )
            };
            let mut body = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?><D:multistatus xmlns:D=\"DAV:\">");
            body.push_str(&entry(&format!("{}/", PREFIX), "LabNAS"));
            if !depth0 {
                for (name, _) in mounts {
                    body.push_str(&entry(&format!("{}/{}/", PREFIX, urlencoding::encode(name)), name));
                }
            }
            body.push_str("</D:multistatus>");
            let mut r = Response::new(Body::from(body));
            *r.status_mut() = StatusCode::MULTI_STATUS;
            r.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/xml; charset=utf-8"));
            r
        }
        _ => plain(StatusCode::METHOD_NOT_ALLOWED, "Solo lectura"),
    };
    let h = resp.headers_mut();
    h.insert("DAV", HeaderValue::from_static("1, 2"));
    h.insert(header::ALLOW, HeaderValue::from_static("OPTIONS, PROPFIND"));
    resp
}

async fn enabled(state: &AppState) -> bool {
    let on = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_setting_bool(conn, SETTING))).await.unwrap_or(false);
    on && state.enabled_modules.lock().await.contains("files")
}

/// Todas las rutas /dav, /dav/, /dav/{*path}
pub async fn handle(State(state): State<AppState>, req: Request) -> Response {
    if !enabled(&state).await {
        return plain(StatusCode::NOT_FOUND, "WebDAV desactivado");
    }
    let Some((user, pass)) = basic_credentials(&req) else { return unauthorized() };
    let session = match authenticate(&state, &user, &pass).await {
        Ok(s) => s,
        Err(r) => return *r,
    };
    if session.role == UserRole::Pendiente {
        return plain(StatusCode::FORBIDDEN, "Cuenta pendiente de aprobacion");
    }

    let st = match Storage::load(&state.db).await {
        Ok(s) => s,
        Err((s, e)) => return plain(s, &e),
    };
    let mounts = mounts(&st, &session);
    let Some((name, tail)) = split_dav_path(req.uri().path()) else { return plain(StatusCode::NOT_FOUND, "No encontrado") };

    if name.is_empty() {
        let depth0 = req.headers().get("depth").and_then(|v| v.to_str().ok()) == Some("0");
        return top_level(req.method(), depth0, &mounts);
    }
    let Some((_, root)) = mounts.iter().find(|(m, _)| *m == name) else {
        return plain(StatusCode::NOT_FOUND, "Carpeta no encontrada");
    };
    let target = match target_in_root(root, &tail) {
        Ok(t) => t,
        Err(r) => return *r,
    };
    let op = method_op(req.method());
    if !st.can(&session, &target, op) {
        return plain(StatusCode::FORBIDDEN, "Sin permiso en esta carpeta");
    }

    // MOVE/COPY: el destino debe estar en la misma carpeta y ser escribible
    if matches!(req.method().as_str(), "MOVE" | "COPY") {
        let dest = req
            .headers()
            .get("destination")
            .and_then(|v| v.to_str().ok())
            .map(|d| d.split_once("://").map(|(_, r)| r.find('/').map(|i| &r[i..]).unwrap_or("/")).unwrap_or(d).to_string());
        let Some((dname, dtail)) = dest.as_deref().and_then(split_dav_path) else {
            return plain(StatusCode::BAD_REQUEST, "Destination invalido");
        };
        if dname != name {
            return plain(StatusCode::BAD_GATEWAY, "Solo se puede mover o copiar dentro de la misma carpeta");
        }
        match target_in_root(root, &dtail) {
            Ok(d) if st.can(&session, &d, Op::Write) => {}
            Ok(_) => return plain(StatusCode::FORBIDDEN, "Sin permiso en el destino"),
            Err(r) => return *r,
        }
    }

    // DELETE: a la papelera, como en la web
    if req.method() == Method::DELETE {
        if st.is_root(&target) {
            return plain(StatusCode::FORBIDDEN, "No se puede borrar una carpeta raiz");
        }
        if !target.exists() {
            return plain(StatusCode::NOT_FOUND, "No encontrado");
        }
        return match crate::handlers::trash::move_to_trash(&state, &st.paths(), &target, &session.username).await {
            Ok(_) => {
                state.log_activity("A la papelera (WebDAV)", &target.display().to_string(), &session.username).await;
                StatusCode::NO_CONTENT.into_response()
            }
            Err((s, e)) => plain(s, &e),
        };
    }

    if matches!(req.method().as_str(), "PUT" | "MKCOL" | "MOVE" | "COPY") {
        state
            .log_activity(&format!("WebDAV {}", req.method()), &target.display().to_string(), &session.username)
            .await;
    }

    let dav = dav_server::DavHandler::builder()
        .filesystem(dav_server::localfs::LocalFs::new(root, false, false, false))
        .locksystem(dav_server::fakels::FakeLs::new())
        .strip_prefix(format!("{}/{}", PREFIX, urlencoding::encode(&name)))
        .hide_symlinks(true)
        .build_handler();
    dav.handle(req).await.into_response()
}

// ─── Ajuste (admin) ───

#[derive(serde::Serialize, serde::Deserialize)]
pub struct WebdavSettings {
    pub enabled: bool,
}

/// GET /api/files/webdav
pub async fn get_settings(State(state): State<AppState>) -> Result<Json<WebdavSettings>, (StatusCode, String)> {
    let enabled = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_setting_bool(conn, SETTING))).await?;
    Ok(Json(WebdavSettings { enabled }))
}

/// PUT /api/files/webdav (admin)
pub async fn set_settings(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<WebdavSettings>,
) -> Result<Json<WebdavSettings>, (StatusCode, String)> {
    let v = if req.enabled { "true" } else { "false" };
    crate::db::db_op(&state.db, move |conn| crate::db::set_setting(conn, SETTING, v)).await?;
    if let Ok(mut c) = CREDENTIALS.lock() {
        c.clear();
    }
    state.log_activity("WebDAV", if req.enabled { "activado" } else { "desactivado" }, &session.username).await;
    Ok(Json(req))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rutas_dav() {
        assert_eq!(split_dav_path("/dav"), Some((String::new(), String::new())));
        assert_eq!(split_dav_path("/dav/"), Some((String::new(), String::new())));
        assert_eq!(split_dav_path("/dav/nick/Docs/a%20b.txt"), Some(("nick".into(), "Docs/a b.txt".into())));
        assert_eq!(split_dav_path("/otra"), None);
    }

    #[test]
    fn no_escapa_de_la_raiz() {
        let root = std::env::temp_dir().join(format!("labnas-dav-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        assert!(target_in_root(&root, "a/b.txt").is_ok());
        assert!(target_in_root(&root, "../etc/passwd").is_err());
        std::os::unix::fs::symlink("/etc", root.join("link")).unwrap();
        assert!(target_in_root(&root, "link/passwd").is_err());
    }
}
