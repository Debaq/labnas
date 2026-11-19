use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::models::notifications::UserRole;
use crate::state::{AppState, SessionInfo};

/// Mapeo modulo -> prefijos de ruta API
const MODULE_ROUTE_PREFIXES: &[(&str, &[&str])] = &[
    ("files",      &["/api/files", "/api/shares", "/api/share/", "/api/download-url", "/api/trash", "/api/preview/"]),
    ("network",    &["/api/network"]),
    ("printing",   &["/api/printing"]),
    ("printers3d", &["/api/printers3d"]),
    ("tasks",      &["/api/tasks", "/api/projects", "/api/events"]),
    ("notes",      &["/api/notes"]),
    ("email",      &["/api/email"]),
    ("inventory",  &["/api/inventory"]),
    ("portfolio",  &["/api/portfolio"]),
    ("sensors",    &["/api/sensors"]),
    ("terminal",   &["/api/terminal"]),
    ("music",      &["/api/music"]),
];

fn resolve_module_for_path(path: &str) -> Option<&'static str> {
    for (module_id, prefixes) in MODULE_ROUTE_PREFIXES {
        for prefix in *prefixes {
            if path.starts_with(prefix) {
                return Some(module_id);
            }
        }
    }
    None
}

/// Cabecera que el frontend manda en todo pedido: con sesion por cookie es obligatoria
/// en metodos que modifican (defensa CSRF junto a SameSite=Strict)
pub const CSRF_HEADER: &str = "x-labnas-csrf";

/// Token de la cookie de sesion
pub fn cookie_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let prefix = format!("{}=", crate::handlers::auth::SESSION_COOKIE);
    headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .map(str::trim)
        .find_map(|c| c.strip_prefix(prefix.as_str()))
        .filter(|t| !t.is_empty() && t.len() <= 128 && t.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(String::from)
}

/// WebSockets: se autentican con `?ticket=` (ver events::create_ticket)
const WS_PATHS: &[&str] = &["/api/terminal", "/api/live"];

/// Rutas sin autenticacion
fn is_public(path: &str) -> bool {
    matches!(
        path,
        "/api/health"
            | "/api/auth/login"
            | "/api/auth/register"
            | "/api/auth/has-users"
            | "/api/system/branding"
            | "/api/sensors/data"
            // certificado publico del servidor, para instalarlo como confiable
            | "/api/tls/cert.pem"
    ) || path.starts_with("/api/share/")
        // validada por un token de un solo archivo (handlers::files::serve_preview)
        || path.starts_with("/api/preview/")
}

/// Rutas que no se atienden por el puente PHP (devuelve el motivo)
pub fn bridge_denied(method: &Method, path: &str) -> Option<&'static str> {
    if WS_PATHS.contains(&path) || path.starts_with("/api/live") {
        return Some("No disponible por el puente: requiere conexion directa (LAN o Tailscale)");
    }
    if path == "/api/auth/register" {
        return Some("El registro de cuentas no esta disponible por el puente");
    }
    if !is_public(path) && required_access(method, path) == Access::Admin {
        return Some("La administracion no esta disponible por el puente: usa la LAN o Tailscale");
    }
    None
}

/// Lo que exige una ruta
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Cualquier usuario aprobado
    User,
    /// Admin u operador
    Operator,
    Admin,
    PermTerminal,
    PermPrint,
}

/// Permisos por ruta. **Denegar por defecto**: toda ruta no listada es solo admin,
/// asi un endpoint nuevo nunca queda abierto por olvido.
pub fn required_access(method: &Method, path: &str) -> Access {
    use Access::*;
    let seg: Vec<&str> = path.trim_start_matches("/api/").split('/').collect();
    let m = method.as_str();

    match (m, seg.as_slice()) {
        // --- Auth (autoservicio) ---
        (_, ["auth", "me"]) | (_, ["auth", "logout"]) => User,
        ("POST", ["auth", "password"]) | ("POST", ["auth", "rename"]) | ("POST", ["auth", "link-code"]) => User,
        ("GET", ["auth", "usernames"]) => User,
        ("GET", ["auth", "2fa"]) | ("POST", ["auth", "2fa", _]) => User,

        // --- Modulos ---
        ("GET", ["modules"]) => User,

        // --- Eventos en tiempo real ---
        ("POST", ["live", "ticket"]) | ("GET", ["live"]) => User,

        // --- Archivos ---
        ("GET", ["files"]) | ("GET", ["files", "download"]) | ("GET", ["files", "quickaccess"]) => User,
        ("GET", ["files", "roots"]) | ("POST", ["files", "preview-token"]) => User,
        // Escritura: el handler verifica el permiso de la carpeta (storage::Storage).
        // Por defecto escriben los usuarios con permiso de escritura, como antes.
        ("POST", ["files", "upload"]) | ("POST", ["files", "directory"]) | ("DELETE", ["files"]) => User,
        ("POST", ["download-url"]) => User,
        ("GET", ["trash"]) | ("POST", ["trash", _, "restore"]) | ("DELETE", ["trash", _]) => User,
        ("GET", ["shares"]) | ("POST", ["shares"]) | ("DELETE", ["shares", _]) => User,

        // --- Sistema (lectura) ---
        ("GET", ["storage"]) | ("GET", ["system", "disks"]) | ("GET", ["system", "info"]) => User,
        ("GET", ["system", "update", "check"]) => User,
        ("GET", ["system", "mdns"]) | ("GET", ["system", "services"]) => User,

        // --- Red ---
        ("GET", ["network", "hosts"]) | ("POST", ["network", "scan"]) => User,
        ("POST", ["network", "wake", _]) => Operator,

        // --- Musica (reproductor compartido) ---
        ("POST", ["music", "lastfm-key"]) | ("POST", ["music", "mpv-args"]) => Admin,
        (_, ["music", ..]) => User,

        // --- Terminal ---
        ("GET", ["terminal"]) => PermTerminal,

        // --- Impresoras 3D ---
        ("GET", ["printers3d", ..]) => User,
        // Cola compartida: cualquiera pide; el handler limita editar/borrar a lo propio
        ("POST", ["printers3d", "queue"]) | ("PUT", ["printers3d", "queue", _]) | ("DELETE", ["printers3d", "queue", _]) => User,
        (_, ["printers3d", ..]) => Operator,

        // --- Impresion CUPS ---
        ("GET", ["printing", "user-costs"]) => Admin,
        ("POST", ["printing", "printers", _, "costs"]) | ("POST", ["printing", "printers", _, "stats", "reset"]) => Admin,
        ("POST", ["printing", "printers", _, "enable"]) | ("POST", ["printing", "printers", _, "disable"]) => Admin,
        ("GET", ["printing", ..]) => User,
        ("POST", ["printing", "printers", _, "wake"]) => PermPrint,
        ("POST", ["printing", "print"]) | ("POST", ["printing", "print-file"]) => PermPrint,
        ("DELETE", ["printing", "jobs", _]) => PermPrint,
        (_, ["printing", "duplex", ..]) => PermPrint,

        // --- Tareas, proyectos, calendario (los handlers validan autoria) ---
        (_, ["tasks", ..]) | (_, ["projects", ..]) | (_, ["events", ..]) => User,

        // --- Notas, portafolio, inventario ---
        (_, ["notes", ..]) | (_, ["portfolio", ..]) | (_, ["inventory", ..]) => User,

        // --- Correo (cuenta propia de cada usuario) ---
        ("PUT", ["email", "oauth", "microsoft", "config"]) => Admin,
        (_, ["email", ..]) => User,

        // --- Reportes ---
        ("GET", ["reports", "config"]) | ("GET", ["reports", "mine"]) | ("POST", ["reports"]) => User,

        // --- Sensores ---
        ("GET", ["sensors", ..]) => User,

        // Todo lo demas: admin (usuarios, modulos, telegram, apagar, actualizar, branding,
        // mdns, servicios, limite de subida, raices de almacenamiento...)
        _ => Admin,
    }
}

fn allowed(access: Access, s: &SessionInfo) -> bool {
    let is_admin = s.role == UserRole::Admin;
    match access {
        Access::User => true,
        Access::Operator => is_admin || s.role == UserRole::Operador,
        Access::Admin => is_admin,
        Access::PermTerminal => is_admin || s.permissions.terminal,
        Access::PermPrint => is_admin || s.permissions.impresion,
    }
}

pub async fn permission_check(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();

    // Por el puente PHP (internet): nada de terminal, WebSocket, registro de cuentas ni admin
    if request.extensions().get::<crate::bridge::BridgeOrigin>().is_some() {
        if let Some(reason) = bridge_denied(&method, &path) {
            return (StatusCode::FORBIDDEN, reason).into_response();
        }
    }

    if !path.starts_with("/api/") || is_public(&path) {
        return next.run(request).await;
    }

    // Modulo desactivado => la ruta no existe
    if let Some(module_id) = resolve_module_for_path(&path) {
        let enabled = state.enabled_modules.lock().await.contains(module_id);
        if !enabled {
            return (StatusCode::NOT_FOUND, "Modulo no disponible").into_response();
        }
    }

    // Token por header. Los WebSocket (el navegador no permite headers) usan un
    // ticket de un solo uso: el token de sesion nunca viaja en una URL.
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());
    let cookie = if bearer.is_none() { cookie_token(request.headers()) } else { None };
    let from_cookie = cookie.is_some();
    let token = bearer
        .or(cookie)
        .or_else(|| {
            if !WS_PATHS.contains(&path.as_str()) {
                return None;
            }
            request
                .uri()
                .query()
                .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("ticket=")))
                .and_then(crate::events::redeem_ticket)
        });

    let Some(token) = token else {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    };

    if from_cookie {
        // CSRF: la cookie viaja sola; un formulario de otro sitio no puede poner esta cabecera
        let safe = matches!(method, Method::GET | Method::HEAD | Method::OPTIONS);
        if !safe && !request.headers().contains_key(CSRF_HEADER) {
            return (StatusCode::FORBIDDEN, "Falta la cabecera anti-CSRF").into_response();
        }
        // Los handlers leen el token del header Authorization: se completa desde la cookie
        if let Ok(v) = axum::http::HeaderValue::from_str(&format!("Bearer {}", token)) {
            request.headers_mut().insert(axum::http::header::AUTHORIZATION, v);
        }
    }

    let session = state.sessions.lock().await.get(&token).cloned();
    let Some(session) = session else {
        return (StatusCode::UNAUTHORIZED, "Sesion invalida").into_response();
    };

    if session.is_expired() {
        crate::handlers::auth::remove_session(&state, &token).await;
        return (StatusCode::UNAUTHORIZED, "Sesion expirada").into_response();
    }

    request.extensions_mut().insert(crate::events::SessionToken(token.clone()));

    // Pendiente: solo puede ver su propia cuenta, cerrar sesion y escuchar sus
    // eventos (para enterarse al instante cuando lo aprueban)
    if session.role == UserRole::Pendiente {
        if matches!(path.as_str(), "/api/auth/me" | "/api/auth/logout" | "/api/live/ticket" | "/api/live") {
            request.extensions_mut().insert(session);
            return next.run(request).await;
        }
        return (StatusCode::FORBIDDEN, "Tu cuenta esta pendiente de aprobacion").into_response();
    }

    if !allowed(required_access(&method, &path), &session) {
        return (StatusCode::FORBIDDEN, "Sin permisos para esta accion").into_response();
    }

    // Los handlers pueden leer la sesion con `Extension<SessionInfo>`
    request.extensions_mut().insert(session);
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use Access::*;

    fn acc(m: Method, p: &str) -> Access {
        required_access(&m, p)
    }

    #[test]
    fn rutas_sensibles_son_admin() {
        assert_eq!(acc(Method::POST, "/api/system/upload-limit"), Admin);
        assert_eq!(acc(Method::POST, "/api/system/services"), Admin);
        assert_eq!(acc(Method::PUT, "/api/system/services/8080"), Admin);
        assert_eq!(acc(Method::DELETE, "/api/system/services/8080"), Admin);
        assert_eq!(acc(Method::POST, "/api/music/mpv-args"), Admin);
        assert_eq!(acc(Method::GET, "/api/auth/users"), Admin);
        assert_eq!(acc(Method::POST, "/api/auth/users/pepe/role"), Admin);
        assert_eq!(acc(Method::GET, "/api/notifications/telegram"), Admin);
        assert_eq!(acc(Method::PUT, "/api/files/roots"), Admin);
        assert_eq!(acc(Method::GET, "/api/system/autostart"), Admin);
        assert_eq!(acc(Method::GET, "/api/reports"), Admin);
        assert_eq!(acc(Method::GET, "/api/backups"), Admin);
        assert_eq!(acc(Method::POST, "/api/backups/abc/run"), Admin);
        assert_eq!(acc(Method::GET, "/api/audit"), Admin);
        assert_eq!(acc(Method::POST, "/api/sensors/devices/abc/token"), Admin);
        assert_eq!(acc(Method::PUT, "/api/sensors/security"), Admin);
        assert_eq!(acc(Method::PUT, "/api/files/webdav"), Admin);
        assert_eq!(acc(Method::POST, "/api/network/wake/aa:bb:cc:dd:ee:ff"), Operator);
        assert_eq!(acc(Method::GET, "/api/system/smart"), Admin);
        assert_eq!(acc(Method::PUT, "/api/system/tls"), Admin);
        assert_eq!(acc(Method::GET, "/api/ai/config"), Admin);
        assert_eq!(acc(Method::PUT, "/api/ai/config"), Admin);
        assert_eq!(acc(Method::POST, "/api/ai/test"), Admin);
        assert_eq!(acc(Method::PUT, "/api/email/oauth/microsoft/config"), Admin);
        // ruta inventada: denegada por defecto
        assert_eq!(acc(Method::GET, "/api/nueva/ruta"), Admin);
    }

    #[test]
    fn puente_niega_admin_y_terminal() {
        let d = |m: Method, p: &str| bridge_denied(&m, p).is_some();
        assert!(d(Method::GET, "/api/terminal"));
        assert!(d(Method::GET, "/api/live"));
        assert!(d(Method::POST, "/api/live/ticket"));
        assert!(d(Method::POST, "/api/auth/register"));
        assert!(d(Method::GET, "/api/auth/users"));
        assert!(d(Method::POST, "/api/system/update"));
        assert!(d(Method::GET, "/api/bridge"));
        assert!(d(Method::GET, "/api/nueva/ruta"), "lo no declarado es admin: tambien negado");
        assert!(!d(Method::GET, "/api/files"));
        assert!(!d(Method::POST, "/api/auth/login"));
        assert!(!d(Method::GET, "/api/health"));
        assert!(!d(Method::GET, "/api/email/inbox"));
        assert!(!d(Method::POST, "/api/email/proposals/x/approve"));
    }

    #[test]
    fn archivos_delegan_en_permisos_por_carpeta() {
        // el middleware deja pasar; storage::Storage decide por carpeta (tests de integracion)
        assert_eq!(acc(Method::POST, "/api/download-url"), User);
        assert_eq!(acc(Method::POST, "/api/files/upload"), User);
        assert_eq!(acc(Method::DELETE, "/api/files"), User);
        assert_eq!(acc(Method::GET, "/api/files/download"), User);
        assert_eq!(acc(Method::GET, "/api/trash"), User);
        assert_eq!(acc(Method::DELETE, "/api/trash"), Admin);
    }

    #[test]
    fn impresion_y_3d() {
        assert_eq!(acc(Method::POST, "/api/printing/print-file"), PermPrint);
        assert_eq!(acc(Method::POST, "/api/printing/duplex/prepare"), PermPrint);
        assert_eq!(acc(Method::POST, "/api/printing/printers/hp/enable"), Admin);
        assert_eq!(acc(Method::PUT, "/api/printers3d/abc"), Operator);
        assert_eq!(acc(Method::GET, "/api/printers3d/abc/status"), User);
        assert_eq!(acc(Method::POST, "/api/printers3d/queue"), User);
        assert_eq!(acc(Method::POST, "/api/printers3d/queue/reorder"), Operator);
    }

    #[test]
    fn autoservicio() {
        assert_eq!(acc(Method::GET, "/api/auth/me"), User);
        assert_eq!(acc(Method::POST, "/api/auth/password"), User);
        assert_eq!(acc(Method::POST, "/api/music/play"), User);
        assert_eq!(acc(Method::GET, "/api/email/proposals"), User);
        assert_eq!(acc(Method::POST, "/api/email/oauth/microsoft/start"), User);
        assert_eq!(acc(Method::GET, "/api/email/oauth/microsoft/config"), User);
        assert_eq!(acc(Method::POST, "/api/email/proposals/abc/approve"), User);
        assert_eq!(acc(Method::GET, "/api/terminal"), PermTerminal);
        assert_eq!(acc(Method::POST, "/api/live/ticket"), User);
        assert_eq!(acc(Method::GET, "/api/live"), User);
    }
}
