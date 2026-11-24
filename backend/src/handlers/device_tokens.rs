//! Tokens de dispositivo: credencial de larga duracion para apps de escritorio
//! (`labnas-tray`, el control de musica en la bandeja del sistema).
//!
//! - Se crean desde la web con la sesion normal (`POST /api/auth/device-tokens`); el
//!   token en claro se entrega una sola vez y en la DB queda solo su hash.
//! - Valen hasta que el usuario los revoca (Ajustes > General > Dispositivos vinculados)
//!   o se borra la cuenta (ON DELETE CASCADE).
//! - Alcance minimo: solo las rutas de `DEVICE_SCOPE`. Con uno de estos tokens no se
//!   puede crear otro token, ni leer archivos, ni nada de administracion.
//! - Rol y permisos se leen de `web_users` en cada pedido: si el usuario queda pendiente
//!   o se le cambia el rol, el token lo sigue al instante.

use axum::{
    extract::{Path, State},
    http::{Method, StatusCode},
    Extension, Json,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::db_op;
use crate::error::ApiError;
use crate::handlers::auth::role_from_str;
use crate::models::notifications::{UserPermissions, UserRole};
use crate::state::{now_unix, AppState, SessionInfo};

/// Prefijo que distingue un token de dispositivo de un token de sesion
pub const PREFIX: &str = "lnd_";
/// Tokens por usuario (evita que un script llene la tabla)
const MAX_PER_USER: i64 = 20;
/// `last_used` se actualiza como mucho cada tanto (la bandeja consulta cada pocos segundos)
const LAST_USED_EVERY_SECS: i64 = 300;

/// Rutas que acepta un token de dispositivo: controlar el reproductor compartido
const DEVICE_SCOPE: &[(&str, &str)] = &[
    ("GET", "/api/music/current"),
    ("POST", "/api/music/pause"),
    ("POST", "/api/music/next"),
    ("POST", "/api/music/previous"),
    ("POST", "/api/music/volume"),
    // la app comprueba que el token sigue vivo y a nombre de quien esta, y se desvincula
    ("GET", "/api/auth/device-tokens/me"),
    ("DELETE", "/api/auth/device-tokens/me"),
];

pub fn is_device_token(token: &str) -> bool {
    token.starts_with(PREFIX)
}

pub fn scope_allows(method: &Method, path: &str) -> bool {
    DEVICE_SCOPE.iter().any(|(m, p)| *m == method.as_str() && *p == path)
}

fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{:02x}", b)).collect()
}

fn new_token() -> String {
    let mut buf = [0u8; 24];
    getrandom::fill(&mut buf).expect("getrandom");
    let hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
    format!("{PREFIX}{hex}")
}

fn valid_format(token: &str) -> bool {
    token
        .strip_prefix(PREFIX)
        .is_some_and(|h| h.len() == 48 && h.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Sesion equivalente al token (None si no existe o el formato no calza). La sesion
/// lleva `created_at` = ahora: estos tokens no vencen por tiempo.
pub async fn session_for(state: &AppState, token: &str) -> Option<SessionInfo> {
    if !valid_format(token) {
        return None;
    }
    let hash = token_hash(token);
    let now = chrono::Utc::now();
    let stale = (now - chrono::Duration::seconds(LAST_USED_EVERY_SECS)).to_rfc3339();
    let now_s = now.to_rfc3339();
    db_op(&state.db, move |conn| {
        let row = conn.query_row(
            "SELECT u.username, u.role, u.perm_terminal, u.perm_impresion, u.perm_archivos_escritura
             FROM device_tokens t JOIN web_users u ON u.username = t.username WHERE t.token_hash = ?1",
            params![hash],
            |r| {
                Ok(SessionInfo {
                    username: r.get(0)?,
                    role: role_from_str(&r.get::<_, String>(1)?),
                    permissions: UserPermissions {
                        terminal: r.get(2)?,
                        impresion: r.get(3)?,
                        archivos_escritura: r.get(4)?,
                    },
                    created_at: now_unix(),
                })
            },
        );
        let Ok(session) = row else { return Ok(None) };
        let _ = conn.execute(
            "UPDATE device_tokens SET last_used = ?1 WHERE token_hash = ?2 AND (last_used IS NULL OR last_used < ?3)",
            params![now_s, hash, stale],
        );
        Ok(Some(session))
    })
    .await
    .ok()
    .flatten()
}

#[derive(Deserialize)]
pub struct CreateRequest {
    name: String,
}

#[derive(Serialize)]
pub struct Created {
    id: String,
    name: String,
    /// Solo se entrega aqui; despues no hay forma de volver a verlo
    token: String,
}

#[derive(Serialize)]
pub struct DeviceTokenView {
    id: String,
    name: String,
    created_at: String,
    last_used: Option<String>,
}

/// Nombre que muestra la lista: sin saltos de linea ni controles, 60 caracteres
fn clean_name(raw: &str) -> String {
    let s: String = raw.chars().filter(|c| !c.is_control()).collect();
    let s = crate::textutil::take_chars(s.trim(), 60).trim().to_string();
    if s.is_empty() { "Dispositivo".to_string() } else { s }
}

/// POST /api/auth/device-tokens
pub async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<CreateRequest>,
) -> Result<Json<Created>, ApiError> {
    if session.role == UserRole::Pendiente {
        return Err((StatusCode::FORBIDDEN, "Tu cuenta esta pendiente de aprobacion".into()));
    }
    let name = clean_name(&req.name);
    let token = new_token();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let (u, n, i, h) = (session.username.clone(), name.clone(), id.clone(), token_hash(&token));
    db_op(&state.db, move |conn| {
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM device_tokens WHERE username = ?1", params![u], |r| r.get(0))
            .map_err(|e| format!("DB: {e}"))?;
        if count >= MAX_PER_USER {
            return Err(format!("Ya tienes {MAX_PER_USER} dispositivos vinculados: quita alguno primero"));
        }
        conn.execute(
            "INSERT INTO device_tokens (id, username, name, token_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![i, u, n, h, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| format!("DB: {e}"))?;
        Ok(())
    })
    .await
    .map_err(|(code, msg)| if msg.starts_with("Ya tienes") { (StatusCode::CONFLICT, msg) } else { (code, msg) })?;
    state.log_activity("Dispositivo vinculado", &name, &session.username).await;
    Ok(Json(Created { id, name, token }))
}

/// GET /api/auth/device-tokens
pub async fn list(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<Vec<DeviceTokenView>>, ApiError> {
    let u = session.username.clone();
    db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, created_at, last_used FROM device_tokens WHERE username = ?1 ORDER BY created_at")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&u], |r| {
                Ok(DeviceTokenView { id: r.get(0)?, name: r.get(1)?, created_at: r.get(2)?, last_used: r.get(3)? })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    })
    .await
    .map(Json)
}

/// GET /api/auth/device-tokens/me: a nombre de quien esta el token (para la app)
pub async fn me(Extension(session): Extension<SessionInfo>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "username": session.username }))
}

/// DELETE /api/auth/device-tokens/me: la app se desvincula con su propio token
pub async fn revoke_self(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|t| is_device_token(t))
        .ok_or((StatusCode::BAD_REQUEST, "Solo con un token de dispositivo".to_string()))?;
    let (u, h) = (session.username.clone(), token_hash(token));
    let name: Option<String> = db_op(&state.db, move |conn| {
        let name = conn
            .query_row("SELECT name FROM device_tokens WHERE token_hash = ?1 AND username = ?2", params![h, u], |r| r.get(0))
            .ok();
        conn.execute("DELETE FROM device_tokens WHERE token_hash = ?1 AND username = ?2", params![h, u])
            .map_err(|e| e.to_string())?;
        Ok(name)
    })
    .await?;
    if let Some(name) = name {
        state.log_activity("Dispositivo desvinculado", &name, &session.username).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/auth/device-tokens/{id}
pub async fn delete(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let (u, i) = (session.username.clone(), id.clone());
    let name: Option<String> = db_op(&state.db, move |conn| {
        let name = conn
            .query_row("SELECT name FROM device_tokens WHERE id = ?1 AND username = ?2", params![i, u], |r| r.get(0))
            .ok();
        conn.execute("DELETE FROM device_tokens WHERE id = ?1 AND username = ?2", params![i, u])
            .map_err(|e| e.to_string())?;
        Ok(name)
    })
    .await?;
    let Some(name) = name else {
        return Err((StatusCode::NOT_FOUND, "Dispositivo no encontrado".into()));
    };
    state.log_activity("Dispositivo desvinculado", &name, &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato_y_alcance() {
        let t = new_token();
        assert!(is_device_token(&t) && valid_format(&t));
        assert!(!valid_format("lnd_xyz"));
        assert!(!valid_format(&t.replacen(PREFIX, "abc_", 1)));
        assert!(scope_allows(&Method::POST, "/api/music/next"));
        assert!(scope_allows(&Method::GET, "/api/music/current"));
        assert!(!scope_allows(&Method::POST, "/api/music/play"));
        assert!(!scope_allows(&Method::GET, "/api/music/next"));
        assert!(!scope_allows(&Method::POST, "/api/auth/device-tokens"));
        assert!(!scope_allows(&Method::GET, "/api/files"));
    }

    #[test]
    fn nombre_limpio() {
        assert_eq!(clean_name("  PC de Ana\n "), "PC de Ana");
        assert_eq!(clean_name(""), "Dispositivo");
        assert_eq!(clean_name(&"x".repeat(100)).chars().count(), 60);
    }
}
