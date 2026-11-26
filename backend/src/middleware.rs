use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::state::{AppState, SessionInfo};

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

/// Rutas sin autenticacion (mas las que declara cada modulo en `crate::modules`)
fn is_public(path: &str) -> bool {
    matches!(
        path,
        "/api/health"
            | "/api/auth/login"
            | "/api/auth/register"
            | "/api/auth/has-users"
            | "/api/auth/passkey/login/options"
            | "/api/auth/passkey/login"
            | "/api/system/branding"
            // certificado publico del servidor, para instalarlo como confiable
            | "/api/tls/cert.pem"
    ) || crate::modules::is_public(path)
        // fichas publicas por token (handlers::access; el modulo del registro tiene que estar encendido)
        || path.starts_with("/api/public/")
        // pantalla del laboratorio: solo lectura, por token (handlers::display)
        || path.starts_with("/api/pantalla/")
}

/// Rutas que no se atienden por el puente PHP (devuelve el motivo)
pub fn bridge_denied(method: &Method, path: &str) -> Option<&'static str> {
    if WS_PATHS.contains(&path) || path == "/api/live/ticket" {
        return Some("No disponible por el puente: requiere conexion directa (LAN o Tailscale)");
    }
    if path == "/api/auth/register" {
        return Some("El registro de cuentas no esta disponible por el puente");
    }
    // Camaras: solo en la red del laboratorio (el aviso de grabacion si se ve)
    if path.starts_with("/api/cameras") && path != "/api/cameras/recording-now" {
        return Some("Las camaras solo estan disponibles en la red del laboratorio (LAN o Tailscale)");
    }
    if !is_public(path) && required_access(method, path) == Access::Admin {
        return Some("La administracion no esta disponible por el puente: usa la LAN o Tailscale");
    }
    None
}

/// Lo que exige una ruta. Ver, Usar y Gestionar son niveles en el modulo de la ruta
/// (ver `crate::modules` y roles.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Cualquier usuario aprobado, sin importar el modulo
    Any,
    Ver,
    Usar,
    Gestionar,
    Admin,
}

/// Permisos por ruta. **Denegar por defecto**: toda ruta no listada es solo admin,
/// asi un endpoint nuevo nunca queda abierto por olvido.
pub fn required_access(method: &Method, path: &str) -> Access {
    use Access::*;
    let seg: Vec<&str> = path.trim_start_matches("/api/").split('/').collect();
    let m = method.as_str();
    let read = m == "GET";

    match (m, seg.as_slice()) {
        // --- Auth (autoservicio) ---
        (_, ["auth", "me"]) | (_, ["auth", "logout"]) => Any,
        ("POST", ["auth", "password"]) | ("POST", ["auth", "rename"]) | ("POST", ["auth", "link-code"]) => Any,
        ("GET", ["auth", "usernames"]) => Any,
        ("GET", ["auth", "2fa"]) | ("POST", ["auth", "2fa", _]) => Any,
        ("POST", ["auth", "passkey", "register", ..]) | ("GET", ["auth", "passkeys"]) | ("DELETE", ["auth", "passkeys", _]) => Any,
        // Preferencias de la interfaz (piel)
        ("GET", ["auth", "preferences"]) | ("PUT", ["auth", "preferences"]) => Any,
        // Tokens de dispositivo propios (labnas-tray)
        ("GET", ["auth", "device-tokens"]) | ("POST", ["auth", "device-tokens"]) => Any,
        ("GET", ["auth", "device-tokens", "me"]) | ("DELETE", ["auth", "device-tokens", _]) => Any,

        // --- Modulos ---
        ("GET", ["modules"]) => Any,

        // --- Niveles de acceso: cada handler revisa el modulo del registro (gestionar, o
        // haberlo creado) o la autorizacion vigente de la persona (handlers::access) ---
        ("GET", ["access"]) | ("PUT", ["access"]) | ("GET", ["access", "levels"]) => Any,
        ("POST", ["access", "grants"]) | ("DELETE", ["access", "grants", _]) => Any,
        ("GET", ["shared"]) | ("GET", ["shared", "view"]) | ("GET", ["shared", "download"]) => Any,

        // --- Eventos en tiempo real (cada evento se filtra por el nivel en su modulo) ---
        ("POST", ["live", "ticket"]) | ("GET", ["live"]) | ("GET", ["live", "poll"]) | ("GET", ["live", "notices"]) => Any,

        // --- Archivos: lectura y escritura las decide storage::Storage por carpeta;
        // "con permiso de escritura" = nivel Usar en Archivos ---
        ("GET", ["files"]) | ("GET", ["files", "download"]) | ("GET", ["files", "quickaccess"]) => Ver,
        ("GET", ["files", "roots"]) | ("POST", ["files", "preview-token"]) => Ver,
        ("POST", ["files", "upload"]) | ("POST", ["files", "directory"]) | ("DELETE", ["files"]) => Ver,
        (_, ["files", "upload", "chunked", ..]) => Ver,
        ("POST", ["download-url"]) => Ver,
        ("GET", ["trash"]) | ("POST", ["trash", _, "restore"]) | ("DELETE", ["trash", _]) => Ver,
        // Un enlace publico saca el archivo del laboratorio: hace falta Usar
        ("GET", ["shares"]) => Ver,
        ("POST", ["shares"]) | ("DELETE", ["shares", _]) => Usar,

        // --- Sistema (lectura) ---
        ("GET", ["storage"]) | ("GET", ["system", "disks"]) | ("GET", ["system", "info"]) => Any,
        ("GET", ["system", "update", "check"]) => Any,
        ("GET", ["system", "mdns"]) | ("GET", ["system", "services"]) => Any,

        // --- Red ---
        ("GET", ["network", "hosts"]) => Ver,
        ("POST", ["network", "scan"]) => Usar,
        ("POST", ["network", "wake", _]) => Gestionar,

        // --- Musica (reproductor compartido) ---
        ("POST", ["music", "lastfm-key"]) | ("POST", ["music", "mpv-args"]) => Admin,
        (_, ["music", ..]) => if read { Ver } else { Usar },

        // --- Terminal ---
        ("GET", ["terminal"]) => Usar,

        // --- Impresoras 3D ---
        ("GET", ["printers3d", ..]) => Ver,
        // Cola compartida: cualquiera pide; el handler limita editar/borrar a lo propio
        ("POST", ["printers3d", "queue"]) | ("PUT", ["printers3d", "queue", _]) | ("DELETE", ["printers3d", "queue", _]) => Usar,
        (_, ["printers3d", ..]) => Gestionar,

        // --- Impresion CUPS ---
        ("GET", ["printing", "user-costs"]) => Admin,
        ("POST", ["printing", "printers", _, "costs"]) | ("POST", ["printing", "printers", _, "stats", "reset"]) => Admin,
        ("POST", ["printing", "printers", _, "enable"]) | ("POST", ["printing", "printers", _, "disable"]) => Admin,
        ("POST", ["printing", "printers", _, "visibility"]) => Admin,
        // Escaner: mismo nivel que imprimir (tambien para ver las paginas)
        (_, ["printing", "scan", ..]) => Usar,
        ("GET", ["printing", ..]) => Ver,
        ("POST", ["printing", "printers", _, "wake"]) => Usar,
        ("POST", ["printing", "print"]) | ("POST", ["printing", "print-file"]) => Usar,
        ("DELETE", ["printing", "jobs", _]) => Usar,
        (_, ["printing", "duplex", ..]) => Usar,
        // Token propio de la impresora de red (IPP)
        ("POST", ["printing", "ipp", "token"]) | ("DELETE", ["printing", "ipp", "token"]) => Usar,

        // --- Tareas, proyectos, calendario (los handlers validan autoria; Gestionar edita lo ajeno) ---
        (_, ["tasks", ..]) | (_, ["projects", ..]) | (_, ["events", ..]) => if read { Ver } else { Usar },

        // --- Notas, portafolio, inventario ---
        (_, ["notes", ..]) | (_, ["portfolio", ..]) | (_, ["inventory", ..]) => if read { Ver } else { Usar },

        // --- Correo (cuenta propia de cada usuario) ---
        ("PUT", ["email", "oauth", "microsoft", "config"]) => Admin,
        (_, ["email", ..]) => if read { Ver } else { Usar },

        // --- Reportes ---
        ("GET", ["reports", "config"]) | ("GET", ["reports", "mine"]) | ("POST", ["reports"]) => Any,

        // --- Camaras: ver y grabar segun el nivel; configurar y borrar, admin ---
        // El aviso de grabacion lo ve todo el mundo, tenga o no acceso a las camaras
        ("GET", ["cameras", "recording-now"]) => Any,
        // Deteccion con IA: el handler permite solo al admin y a las personas autorizadas
        ("GET", ["cameras", "detectors"]) | ("GET", ["cameras", "detections"]) => Any,
        // Descargas: el handler exige Ver en Camaras, o estar autorizado si la hizo la IA
        ("GET", ["cameras", "recordings", _, "download"]) => Any,
        ("GET", ["cameras", "detect"]) => Admin,
        ("GET", ["cameras", ..]) => Ver,
        ("POST", ["cameras", _, "record" | "stop" | "photo"]) => Usar,

        // --- Sensores ---
        ("GET", ["sensors", ..]) => Ver,

        // --- Maquinas: ver, reservar y reportar fallas (el handler limita cancelar a lo
        // propio y bloquear horarios a quien gestiona); Gestionar edita, aprueba y repara ---
        ("GET", ["machines", ..]) => Ver,
        ("POST", ["machines", _, "bookings"]) | ("POST", ["machines", _, "faults"]) => Usar,
        ("POST", ["machines", "bookings", _, "cancel"]) => Usar,
        // Bitacora de mantencion: el staff o el responsable de la maquina (lo revisa el handler)
        ("POST", ["machines", _, "maintenance", "log"]) => Usar,
        // Induccion: pedirla (y retirar la propia solicitud, el handler lo limita)
        ("POST", ["machines", _, "certifications", "request"]) => Usar,
        ("DELETE", ["machines", _, "certifications", _]) => Usar,
        ("DELETE", ["machines", _]) => Admin,
        (_, ["machines", ..]) => Gestionar,

        // --- Plazos: ver y crear; el handler limita editar, cumplir y borrar a quien lo creo
        // o a quien gestiona ---
        (_, ["deadlines", ..]) => if read { Ver } else { Usar },

        // Todo lo demas: admin (usuarios, roles, modulos, telegram, apagar, actualizar,
        // branding, mdns, servicios, limite de subida, raices de almacenamiento...)
        _ => Admin,
    }
}

fn allowed(access: Access, path: &str, s: &SessionInfo) -> bool {
    use crate::roles::Level;
    let level = match access {
        Access::Any => return true,
        Access::Admin => return s.is_admin(),
        Access::Ver => Level::Ver,
        Access::Usar => Level::Usar,
        Access::Gestionar => Level::Gestionar,
    };
    // Un nivel sin modulo no deberia existir (test `niveles_siempre_con_modulo`): solo admin
    match crate::modules::for_path(path) {
        Some(m) => s.can(m, level),
        None => s.is_admin(),
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

    if !path.starts_with("/api/") {
        return next.run(request).await;
    }

    // Modulo desactivado => la ruta no existe, tambien las publicas (enlaces, sensores WiFi)
    if let Some(module_id) = crate::modules::for_path(&path) {
        let enabled = state.enabled_modules.lock().await.contains(module_id);
        if !enabled {
            return (StatusCode::NOT_FOUND, "Modulo no disponible").into_response();
        }
    }

    if is_public(&path) {
        return next.run(request).await;
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

    // Token de dispositivo (labnas-tray): solo por header y solo en su alcance
    if crate::handlers::device_tokens::is_device_token(&token) {
        if from_cookie || !crate::handlers::device_tokens::scope_allows(&method, &path) {
            return (StatusCode::FORBIDDEN, "Este token de dispositivo no sirve para esta accion").into_response();
        }
        let Some(session) = crate::handlers::device_tokens::session_for(&state, &token).await else {
            return (StatusCode::UNAUTHORIZED, "Dispositivo no vinculado o revocado").into_response();
        };
        if session.role.is_pending() {
            return (StatusCode::FORBIDDEN, "Tu cuenta esta pendiente de aprobacion").into_response();
        }
        if !allowed(required_access(&method, &path), &path, &session) {
            return (StatusCode::FORBIDDEN, "Sin permisos para esta accion").into_response();
        }
        request.extensions_mut().insert(session);
        return next.run(request).await;
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
    if session.role.is_pending() {
        if matches!(path.as_str(), "/api/auth/me" | "/api/auth/logout" | "/api/live/ticket" | "/api/live") {
            request.extensions_mut().insert(session);
            return next.run(request).await;
        }
        return (StatusCode::FORBIDDEN, "Tu cuenta esta pendiente de aprobacion").into_response();
    }

    if !allowed(required_access(&method, &path), &path, &session) {
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
        assert_eq!(acc(Method::GET, "/api/ledger"), Admin);
        assert_eq!(acc(Method::POST, "/api/ledger/notes"), Admin);
        assert_eq!(acc(Method::POST, "/api/ledger/3/annul"), Admin);
        assert_eq!(acc(Method::POST, "/api/sensors/devices/abc/token"), Admin);
        assert_eq!(acc(Method::PUT, "/api/sensors/security"), Admin);
        assert_eq!(acc(Method::PUT, "/api/files/webdav"), Admin);
        assert_eq!(acc(Method::POST, "/api/network/wake/aa:bb:cc:dd:ee:ff"), Gestionar);
        assert_eq!(acc(Method::GET, "/api/system/smart"), Admin);
        assert_eq!(acc(Method::POST, "/api/snapshots/now"), Admin);
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
        assert!(!d(Method::GET, "/api/live/poll"), "long-poll de eventos si pasa");
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
        // el middleware exige ver Archivos; storage::Storage decide por carpeta
        assert_eq!(acc(Method::POST, "/api/download-url"), Ver);
        assert_eq!(acc(Method::POST, "/api/files/upload"), Ver);
        assert_eq!(acc(Method::DELETE, "/api/files"), Ver);
        assert_eq!(acc(Method::GET, "/api/files/download"), Ver);
        assert_eq!(acc(Method::GET, "/api/trash"), Ver);
        assert_eq!(acc(Method::DELETE, "/api/trash"), Admin);
        assert_eq!(acc(Method::POST, "/api/shares"), Usar);
    }

    #[test]
    fn impresion_y_3d() {
        assert_eq!(acc(Method::POST, "/api/printing/print-file"), Usar);
        assert_eq!(acc(Method::POST, "/api/printing/duplex/prepare"), Usar);
        assert_eq!(acc(Method::POST, "/api/printing/printers/hp/enable"), Admin);
        assert_eq!(acc(Method::POST, "/api/printing/printers/hp/visibility"), Admin);
        assert_eq!(acc(Method::GET, "/api/printing/ipp"), Ver);
        assert_eq!(acc(Method::POST, "/api/printing/ipp/token"), Usar);
        assert_eq!(acc(Method::DELETE, "/api/printing/ipp/token"), Usar);
        assert_eq!(acc(Method::GET, "/api/printing/scan/devices"), Usar);
        assert_eq!(acc(Method::GET, "/api/printing/scan/sessions/abc/pages/0"), Usar);
        assert_eq!(acc(Method::POST, "/api/printing/scan/pages"), Usar);
    }

    #[test]
    fn camaras() {
        assert_eq!(acc(Method::GET, "/api/cameras/recording-now"), Any);
        assert_eq!(acc(Method::GET, "/api/cameras/detections"), Any);
        assert_eq!(acc(Method::PUT, "/api/cameras/detectors/watchers"), Admin);
        assert_eq!(acc(Method::POST, "/api/cameras/detectors"), Admin);
        assert_eq!(acc(Method::POST, "/api/cameras/detectors/test"), Admin);
        assert_eq!(acc(Method::PUT, "/api/cameras/detectors/x"), Admin);
        assert_eq!(acc(Method::POST, "/api/cameras/ai/model"), Admin);
        assert_eq!(acc(Method::GET, "/api/cameras"), Ver);
        assert_eq!(acc(Method::GET, "/api/cameras/abc/stream"), Ver);
        assert_eq!(acc(Method::POST, "/api/cameras/abc/record"), Usar);
        assert_eq!(acc(Method::POST, "/api/cameras/abc/photo"), Usar);
        assert_eq!(acc(Method::GET, "/api/cameras/detect"), Admin);
        assert_eq!(acc(Method::POST, "/api/cameras"), Admin);
        assert_eq!(acc(Method::PUT, "/api/cameras/settings"), Admin);
        assert_eq!(acc(Method::DELETE, "/api/cameras/recordings/x"), Admin);
        assert!(bridge_denied(&Method::GET, "/api/cameras/abc/stream").is_some());
        assert!(bridge_denied(&Method::GET, "/api/cameras/recording-now").is_none());
        assert_eq!(acc(Method::PUT, "/api/printers3d/abc"), Gestionar);
        assert_eq!(acc(Method::GET, "/api/printers3d/abc/status"), Ver);
        assert_eq!(acc(Method::POST, "/api/printers3d/queue"), Usar);
        assert_eq!(acc(Method::POST, "/api/printers3d/queue/reorder"), Gestionar);
    }

    #[test]
    fn autoservicio() {
        assert_eq!(acc(Method::GET, "/api/auth/me"), Any);
        assert_eq!(acc(Method::POST, "/api/auth/password"), Any);
        assert_eq!(acc(Method::POST, "/api/auth/passkey/register/options"), Any);
        assert_eq!(acc(Method::DELETE, "/api/auth/passkeys/abc"), Any);
        assert!(is_public("/api/auth/passkey/login"));
        assert_eq!(acc(Method::POST, "/api/music/play"), Usar);
        assert_eq!(acc(Method::GET, "/api/music/status"), Ver);
        assert_eq!(acc(Method::POST, "/api/auth/device-tokens"), Any);
        assert_eq!(acc(Method::DELETE, "/api/auth/device-tokens/abc"), Any);
        assert_eq!(acc(Method::GET, "/api/auth/device-tokens/me"), Any);
        assert_eq!(acc(Method::PUT, "/api/auth/preferences"), Any);
        assert_eq!(acc(Method::GET, "/api/email/proposals"), Ver);
        assert_eq!(acc(Method::POST, "/api/email/oauth/microsoft/start"), Usar);
        assert_eq!(acc(Method::GET, "/api/email/oauth/microsoft/config"), Ver);
        assert_eq!(acc(Method::POST, "/api/email/proposals/abc/approve"), Usar);
        assert_eq!(acc(Method::GET, "/api/terminal"), Usar);
        assert_eq!(acc(Method::POST, "/api/live/ticket"), Any);
        assert_eq!(acc(Method::GET, "/api/live"), Any);
    }

    #[test]
    fn roles_y_usuarios_son_admin() {
        assert_eq!(acc(Method::GET, "/api/roles"), Admin);
        assert_eq!(acc(Method::POST, "/api/roles"), Admin);
        assert_eq!(acc(Method::PUT, "/api/roles/docente"), Admin);
        assert_eq!(acc(Method::DELETE, "/api/roles/docente"), Admin);
        assert_eq!(acc(Method::POST, "/api/auth/users/pepe/role"), Admin);
    }

    /// Toda ruta que pide Ver/Usar/Gestionar cae en un modulo (si no, `allowed` la deja
    /// solo al admin y la funcion quedaria rota sin que nadie lo note)
    #[test]
    fn niveles_siempre_con_modulo() {
        let mut checked = 0;
        // Rutas declaradas en main.rs: `.route("/api/x/{id}", get(..).post(..))`
        let src = include_str!("main.rs");
        let mut routes = Vec::new();
        for line in src.lines().map(str::trim).filter(|l| l.starts_with(".route(\"/api/")) {
            let path = line.split('"').nth(1).unwrap_or_default();
            let path: String = path.split('/').map(|p| if p.starts_with('{') { "x" } else { p }).collect::<Vec<_>>().join("/");
            for verb in ["get", "post", "put", "delete", "patch"] {
                if line.contains(&format!("{verb}(")) {
                    routes.push((verb.to_uppercase(), path.clone()));
                }
            }
        }
        assert!(routes.len() > 200, "no se leyeron las rutas de main.rs");
        for (method, path) in routes {
            let Ok(m) = method.parse::<Method>() else { continue };
            if matches!(required_access(&m, &path), Access::Ver | Access::Usar | Access::Gestionar) {
                assert!(crate::modules::for_path(&path).is_some(), "{} {} pide nivel pero no tiene modulo", method, path);
                checked += 1;
            }
        }
        assert!(checked > 20);
    }

    fn sess(role: &str, levels: &[(&str, crate::roles::Level)]) -> SessionInfo {
        let grants = crate::roles::Grants {
            admin: role == "admin",
            levels: levels.iter().map(|(m, l)| (m.to_string(), *l)).collect(),
        };
        SessionInfo::new("ana".into(), crate::models::notifications::UserRole(role.into()), grants, None)
    }

    #[test]
    fn niveles_por_modulo() {
        use crate::roles::Level;
        let est = sess("estudiante", &[("machines", Level::Usar), ("files", Level::Ver)]);
        assert!(allowed(acc(Method::GET, "/api/machines"), "/api/machines", &est));
        assert!(allowed(acc(Method::POST, "/api/machines/x/bookings"), "/api/machines/x/bookings", &est));
        assert!(!allowed(acc(Method::PUT, "/api/machines/x"), "/api/machines/x", &est));
        assert!(!allowed(acc(Method::GET, "/api/terminal"), "/api/terminal", &est));
        assert!(!allowed(acc(Method::GET, "/api/notes"), "/api/notes", &est), "sin fila = sin acceso");
        assert!(allowed(acc(Method::GET, "/api/files"), "/api/files", &est));
        assert!(!allowed(acc(Method::POST, "/api/shares"), "/api/shares", &est));
        assert!(allowed(acc(Method::GET, "/api/auth/me"), "/api/auth/me", &est));
        assert!(!allowed(acc(Method::GET, "/api/auth/users"), "/api/auth/users", &est));
        let admin = sess("admin", &[]);
        assert!(allowed(acc(Method::PUT, "/api/machines/x"), "/api/machines/x", &admin));
        assert!(allowed(acc(Method::GET, "/api/auth/users"), "/api/auth/users", &admin));
    }
}
