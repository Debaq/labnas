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
    ("files",      &["/api/files", "/api/shares", "/api/share/", "/api/download-url"]),
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
    ) || path.starts_with("/api/share/")
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
    PermFileWrite,
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

        // --- Modulos ---
        ("GET", ["modules"]) => User,

        // --- Archivos ---
        ("GET", ["files"]) | ("GET", ["files", "download"]) | ("GET", ["files", "quickaccess"]) => User,
        ("GET", ["files", "roots"]) => User,
        ("POST", ["files", "upload"]) | ("POST", ["files", "directory"]) | ("DELETE", ["files"]) => PermFileWrite,
        ("POST", ["download-url"]) => PermFileWrite,
        ("GET", ["shares"]) | ("POST", ["shares"]) | ("DELETE", ["shares", _]) => User,

        // --- Sistema (lectura) ---
        ("GET", ["storage"]) | ("GET", ["system", "disks"]) | ("GET", ["system", "info"]) => User,
        ("GET", ["system", "update", "check"]) => User,
        ("GET", ["system", "mdns"]) | ("GET", ["system", "services"]) => User,

        // --- Red ---
        ("GET", ["network", "hosts"]) | ("POST", ["network", "scan"]) => User,

        // --- Musica (reproductor compartido) ---
        ("POST", ["music", "lastfm-key"]) | ("POST", ["music", "mpv-args"]) => Admin,
        (_, ["music", ..]) => User,

        // --- Terminal ---
        ("GET", ["terminal"]) => PermTerminal,

        // --- Impresoras 3D ---
        ("GET", ["printers3d", ..]) => User,
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
        ("POST", ["email", "groq-key"]) => Admin,
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
        Access::PermFileWrite => is_admin || s.permissions.archivos_escritura,
    }
}

pub async fn permission_check(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();

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

    // Token por header; por query solo para el WebSocket de la terminal
    // (los navegadores no permiten headers en WebSocket)
    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| {
            if path != "/api/terminal" {
                return None;
            }
            request.uri().query().and_then(|q| {
                q.split('&')
                    .find_map(|p| p.strip_prefix("token="))
                    .map(|t| t.to_string())
            })
        });

    let Some(token) = token else {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    };

    let session = state.sessions.lock().await.get(&token).cloned();
    let Some(session) = session else {
        return (StatusCode::UNAUTHORIZED, "Sesion invalida").into_response();
    };

    if session.is_expired() {
        crate::handlers::auth::remove_session(&state, &token).await;
        return (StatusCode::UNAUTHORIZED, "Sesion expirada").into_response();
    }

    // Pendiente: solo puede ver su propia cuenta y cerrar sesion
    if session.role == UserRole::Pendiente {
        if matches!(path.as_str(), "/api/auth/me" | "/api/auth/logout") {
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
        // ruta inventada: denegada por defecto
        assert_eq!(acc(Method::GET, "/api/nueva/ruta"), Admin);
    }

    #[test]
    fn escritura_de_archivos_requiere_permiso() {
        assert_eq!(acc(Method::POST, "/api/download-url"), PermFileWrite);
        assert_eq!(acc(Method::POST, "/api/files/upload"), PermFileWrite);
        assert_eq!(acc(Method::DELETE, "/api/files"), PermFileWrite);
        assert_eq!(acc(Method::GET, "/api/files/download"), User);
    }

    #[test]
    fn impresion_y_3d() {
        assert_eq!(acc(Method::POST, "/api/printing/print-file"), PermPrint);
        assert_eq!(acc(Method::POST, "/api/printing/duplex/prepare"), PermPrint);
        assert_eq!(acc(Method::POST, "/api/printing/printers/hp/enable"), Admin);
        assert_eq!(acc(Method::PUT, "/api/printers3d/abc"), Operator);
        assert_eq!(acc(Method::GET, "/api/printers3d/abc/status"), User);
    }

    #[test]
    fn autoservicio() {
        assert_eq!(acc(Method::GET, "/api/auth/me"), User);
        assert_eq!(acc(Method::POST, "/api/auth/password"), User);
        assert_eq!(acc(Method::POST, "/api/music/play"), User);
        assert_eq!(acc(Method::GET, "/api/terminal"), PermTerminal);
    }
}
