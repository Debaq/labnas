use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::models::notifications::UserRole;
use crate::state::AppState;

pub async fn permission_check(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();

    // Public routes - no auth needed
    if matches!(
        path.as_str(),
        "/api/health" | "/api/auth/login" | "/api/auth/register"
    ) || !path.starts_with("/api/")
    {
        return next.run(request).await;
    }

    // Extract token
    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let Some(token) = token else {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    };

    let mut sessions = state.sessions.lock().await;
    let session = sessions.get(&token).cloned();

    let Some(session) = session else {
        drop(sessions);
        return (StatusCode::UNAUTHORIZED, "Sesion invalida").into_response();
    };

    // Verificar expiración de sesión (24 horas)
    if session.created_at.elapsed() > std::time::Duration::from_secs(24 * 60 * 60) {
        sessions.remove(&token);
        drop(sessions);
        return (StatusCode::UNAUTHORIZED, "Sesion expirada").into_response();
    }
    drop(sessions);

    // Pendiente: only self-service
    if session.role == UserRole::Pendiente {
        if matches!(path.as_str(), "/api/auth/me" | "/api/auth/logout") {
            return next.run(request).await;
        }
        return (StatusCode::FORBIDDEN, "Tu cuenta esta pendiente de aprobacion").into_response();
    }

    let is_admin = session.role == UserRole::Admin;

    // Check route permissions
    let allowed = match (&method, path.as_str()) {
        // Self-service always ok
        (_, "/api/auth/me") => true,
        (_, "/api/auth/logout") => true,
        (&Method::POST, "/api/auth/link-code") => true,

        // Admin only: bot config
        (&Method::POST, p) if p.starts_with("/api/notifications/telegram/token") => is_admin,
        (&Method::DELETE, p) if p.starts_with("/api/notifications/telegram/token") => is_admin,
        (&Method::POST, "/api/notifications/schedule") => is_admin,
        (&Method::POST, p) if p.contains("/role") => is_admin,
        (&Method::POST, p) if p.contains("/link") => is_admin,
        (&Method::DELETE, p) if p.starts_with("/api/notifications/telegram/chat/") => is_admin,
        (&Method::POST, "/api/notifications/telegram/test") => is_admin,

        // Admin only: user management
        (&Method::GET, "/api/auth/users") => is_admin,
        (&Method::POST, p) if p.starts_with("/api/auth/users/") => is_admin,
        (&Method::DELETE, p) if p.starts_with("/api/auth/users/") => is_admin,

        // Admin only: shutdown
        (&Method::POST, "/api/system/shutdown") => is_admin,

        // Admin only: autostart
        (&Method::POST, p) if p.starts_with("/api/system/autostart") => is_admin,

        // Admin only: network device labels
        (&Method::POST, p) if p.starts_with("/api/network/device/") => is_admin,
        (&Method::DELETE, p) if p.starts_with("/api/network/device/") => is_admin,

        // Admin only: printer enable/disable
        (&Method::POST, p) if p.contains("/enable") || p.contains("/disable") => is_admin,

        // File write: need archivos_escritura
        (&Method::POST, "/api/files/upload") => is_admin || session.permissions.archivos_escritura,
        (&Method::DELETE, p) if p.starts_with("/api/files") => is_admin || session.permissions.archivos_escritura,
        (&Method::POST, "/api/files/directory") => is_admin || session.permissions.archivos_escritura,

        // Printing: need impresion
        (&Method::POST, "/api/printing/print") => is_admin || session.permissions.impresion,
        (&Method::POST, "/api/printing/print-file") => is_admin || session.permissions.impresion,

        // Terminal: need terminal
        (&Method::GET, "/api/terminal") => is_admin || session.permissions.terminal,

        // 3D printer management: admin or operator
        (&Method::POST, p) if p.starts_with("/api/printers3d") => {
            is_admin || session.role == UserRole::Operador
        }
        (&Method::DELETE, p) if p.starts_with("/api/printers3d") => {
            is_admin || session.role == UserRole::Operador
        }

        // Everything else (GET reads, tasks, events, projects): any authenticated user
        _ => true,
    };

    if !allowed {
        return (StatusCode::FORBIDDEN, "Sin permisos para esta accion").into_response();
    }

    next.run(request).await
}
