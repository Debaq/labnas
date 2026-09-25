//! Doble factor (TOTP) por usuario: activar con QR, codigos de recuperacion,
//! contrasena de aplicacion para WebDAV y reinicio por un admin.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::auth::{bcrypt_hash, bcrypt_verify, login_failed, LOGIN_LOCK_SECS, MAX_LOGIN_FAILURES};
use crate::state::{AppState, SessionInfo};
use crate::totp;

type ApiResult<T> = Result<T, (StatusCode, String)>;

const ISSUER: &str = "LabNAS";
const RECOVERY_CODES: usize = 10;
/// Tiempo para escanear el QR y confirmar con un codigo
const PENDING_TTL: Duration = Duration::from_secs(10 * 60);

/// Secretos generados que aun no se confirman (usuario -> secreto)
static PENDING: LazyLock<Mutex<HashMap<String, (String, Instant)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

// ─── Verificacion (tambien la usa el login) ───

/// Acepta un codigo TOTP (no reutilizable) o uno de recuperacion (se consume)
pub fn verify_second_factor(conn: &Connection, username: &str, code: &str) -> Result<bool, String> {
    let row: Option<(Option<String>, i64)> = conn
        .query_row(
            "SELECT totp_secret, totp_last_step FROM web_users WHERE username = ?1",
            [username],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((Some(stored), last_step)) = row else {
        return Ok(false);
    };
    let secret = crate::secrets::decrypt(&stored)?;
    if let Some(step) = totp::verify(&secret, code, totp::current_step(), last_step) {
        conn.execute("UPDATE web_users SET totp_last_step = ?1 WHERE username = ?2", params![step, username])
            .map_err(|e| e.to_string())?;
        return Ok(true);
    }
    let used = conn
        .execute(
            "DELETE FROM totp_recovery WHERE username = ?1 AND code_hash = ?2",
            params![username, totp::hash_recovery(code)],
        )
        .map_err(|e| e.to_string())?;
    Ok(used == 1)
}

fn new_recovery_codes(conn: &Connection, username: &str) -> Result<Vec<String>, String> {
    conn.execute("DELETE FROM totp_recovery WHERE username = ?1", [username]).map_err(|e| e.to_string())?;
    let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| totp::new_recovery_code()).collect();
    for c in &codes {
        conn.execute(
            "INSERT OR IGNORE INTO totp_recovery (username, code_hash) VALUES (?1, ?2)",
            params![username, totp::hash_recovery(c)],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(codes)
}

/// Mismo bloqueo que el login: frena la fuerza bruta de codigos con una sesion robada
async fn check_locked(state: &AppState, username: &str) -> ApiResult<()> {
    let failures = state.login_failures.lock().await;
    match failures.get(username) {
        Some(f) if f.count >= MAX_LOGIN_FAILURES && f.last.elapsed().as_secs() < LOGIN_LOCK_SECS => {
            Err((StatusCode::TOO_MANY_REQUESTS, "Demasiados intentos fallidos. Espera unos minutos".to_string()))
        }
        _ => Ok(()),
    }
}

async fn require_password(state: &AppState, username: &str, password: &str) -> ApiResult<()> {
    let u = username.to_string();
    let hash: String = crate::db::db_op(&state.db, move |conn| {
        conn.query_row("SELECT password_hash FROM web_users WHERE username = ?1", [&u], |r| r.get(0))
            .map_err(|e| e.to_string())
    })
    .await?;
    if bcrypt_verify(password.to_string(), hash).await {
        Ok(())
    } else {
        login_failed(state, username).await;
        Err((StatusCode::UNAUTHORIZED, "Contrasena incorrecta".to_string()))
    }
}

async fn require_code(state: &AppState, username: &str, code: &str) -> ApiResult<()> {
    let (u, c) = (username.to_string(), code.trim().to_string());
    if crate::db::db_op(&state.db, move |conn| verify_second_factor(conn, &u, &c)).await? {
        Ok(())
    } else {
        login_failed(state, username).await;
        Err((StatusCode::UNAUTHORIZED, "Codigo de verificacion incorrecto".to_string()))
    }
}

// ─── Endpoints ───

/// GET /api/auth/2fa
pub async fn status(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> ApiResult<Json<Value>> {
    let u = session.username.clone();
    let (enabled, app_password, left) = crate::db::db_op(&state.db, move |conn| {
        conn.query_row(
            "SELECT totp_secret IS NOT NULL, app_password_hash IS NOT NULL,
                    (SELECT COUNT(*) FROM totp_recovery WHERE username = ?1)
             FROM web_users WHERE username = ?1",
            [&u],
            |r| Ok((r.get::<_, bool>(0)?, r.get::<_, bool>(1)?, r.get::<_, i64>(2)?)),
        )
        .map_err(|e| e.to_string())
    })
    .await?;
    Ok(Json(json!({ "enabled": enabled, "app_password": app_password, "recovery_left": left })))
}

#[derive(Deserialize)]
pub struct PasswordBody {
    pub password: String,
}

/// POST /api/auth/2fa/setup {password}: secreto nuevo + QR (se activa al confirmar)
pub async fn setup(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<PasswordBody>,
) -> ApiResult<Json<Value>> {
    let user = session.username.clone();
    check_locked(&state, &user).await?;
    require_password(&state, &user, &req.password).await?;
    let u = user.clone();
    let enabled: bool = crate::db::db_op(&state.db, move |conn| {
        conn.query_row("SELECT totp_secret IS NOT NULL FROM web_users WHERE username = ?1", [&u], |r| r.get(0))
            .map_err(|e| e.to_string())
    })
    .await?;
    if enabled {
        return Err((StatusCode::CONFLICT, "El doble factor ya esta activo".to_string()));
    }

    let secret = totp::new_secret();
    let uri = totp::otpauth_uri(ISSUER, &user, &secret);
    let qr = totp::qr_svg(&uri).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    if let Ok(mut p) = PENDING.lock() {
        p.retain(|_, (_, at)| at.elapsed() < PENDING_TTL);
        p.insert(user, (secret.clone(), Instant::now()));
    }
    Ok(Json(json!({ "secret": secret, "uri": uri, "qr_svg": qr })))
}

#[derive(Deserialize)]
pub struct CodeBody {
    pub code: String,
}

/// POST /api/auth/2fa/enable {code}: confirma el secreto pendiente; devuelve los codigos de recuperacion
pub async fn enable(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<CodeBody>,
) -> ApiResult<Json<Value>> {
    let user = session.username.clone();
    check_locked(&state, &user).await?;
    let pending = PENDING
        .lock()
        .ok()
        .and_then(|p| p.get(&user).filter(|(_, at)| at.elapsed() < PENDING_TTL).map(|(s, _)| s.clone()))
        .ok_or((StatusCode::BAD_REQUEST, "No hay una activacion en curso (o expiro): vuelve a empezar".to_string()))?;
    let Some(step) = totp::verify(&pending, &req.code, totp::current_step(), 0) else {
        login_failed(&state, &user).await;
        return Err((StatusCode::UNAUTHORIZED, "Codigo incorrecto: revisa la hora del telefono".to_string()));
    };

    let encrypted = crate::secrets::encrypt(&pending).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let u = user.clone();
    let codes = crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE web_users SET totp_secret = ?1, totp_last_step = ?2 WHERE username = ?3",
            params![encrypted, step, &u],
        )
        .map_err(|e| e.to_string())?;
        new_recovery_codes(conn, &u)
    })
    .await?;
    if let Ok(mut p) = PENDING.lock() {
        p.remove(&user);
    }
    // WebDAV deja de aceptar la contrasena normal
    crate::handlers::webdav::forget_credentials(&user);
    state.log_activity("Doble factor", "activado", &user).await;
    Ok(Json(json!({ "recovery_codes": codes })))
}

#[derive(Deserialize)]
pub struct DisableBody {
    pub password: String,
    pub code: String,
}

fn clear_2fa(conn: &Connection, username: &str) -> Result<usize, String> {
    conn.execute("DELETE FROM totp_recovery WHERE username = ?1", [username]).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE web_users SET totp_secret = NULL, totp_last_step = 0, app_password_hash = NULL WHERE username = ?1",
        [username],
    )
    .map_err(|e| e.to_string())
}

/// POST /api/auth/2fa/disable {password, code}
pub async fn disable(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<DisableBody>,
) -> ApiResult<StatusCode> {
    let user = session.username.clone();
    check_locked(&state, &user).await?;
    require_password(&state, &user, &req.password).await?;
    require_code(&state, &user, &req.code).await?;
    let u = user.clone();
    crate::db::db_op(&state.db, move |conn| clear_2fa(conn, &u)).await?;
    crate::handlers::webdav::forget_credentials(&user);
    state.log_activity("Doble factor", "desactivado", &user).await;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/auth/2fa/recovery {code}: reemplaza los codigos de recuperacion
pub async fn regenerate_recovery(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<CodeBody>,
) -> ApiResult<Json<Value>> {
    let user = session.username.clone();
    check_locked(&state, &user).await?;
    require_code(&state, &user, &req.code).await?;
    let u = user.clone();
    let codes = crate::db::db_op(&state.db, move |conn| new_recovery_codes(conn, &u)).await?;
    state.log_activity("Doble factor", "codigos de recuperacion regenerados", &user).await;
    Ok(Json(json!({ "recovery_codes": codes })))
}

/// POST /api/auth/2fa/app-password {code}: contrasena para WebDAV (que no admite
/// segundo factor). Reemplaza la anterior; se muestra una sola vez.
pub async fn app_password(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<CodeBody>,
) -> ApiResult<Json<Value>> {
    let user = session.username.clone();
    check_locked(&state, &user).await?;
    require_code(&state, &user, &req.code).await?;
    let raw = totp::base32_encode(&totp::random_bytes::<10>()).to_lowercase();
    let password = format!("{}-{}-{}-{}", &raw[0..4], &raw[4..8], &raw[8..12], &raw[12..16]);
    let hash = bcrypt_hash(password.clone()).await?;
    let u = user.clone();
    crate::db::db_op(&state.db, move |conn| {
        conn.execute("UPDATE web_users SET app_password_hash = ?1 WHERE username = ?2", params![hash, &u])
            .map_err(|e| e.to_string())
    })
    .await?;
    crate::handlers::webdav::forget_credentials(&user);
    state.log_activity("Doble factor", "contrasena de aplicacion generada", &user).await;
    Ok(Json(json!({ "password": password })))
}

/// DELETE /api/auth/users/{username}/2fa (admin): para quien perdio el telefono y los codigos
pub async fn admin_reset(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(username): Path<String>,
) -> ApiResult<StatusCode> {
    let u = username.clone();
    let n = crate::db::db_op(&state.db, move |conn| clear_2fa(conn, &u)).await?;
    if n == 0 {
        return Err((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()));
    }
    crate::handlers::webdav::forget_credentials(&username);
    state
        .log_activity("Doble factor", &format!("desactivado por admin para {}", username), &session.username)
        .await;
    Ok(StatusCode::NO_CONTENT)
}
