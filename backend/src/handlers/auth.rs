use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rusqlite::{params, OptionalExtension};

use crate::models::auth::*;
use crate::models::notifications::UserRole;
use crate::roles::Grants;
use crate::state::{now_unix, AppState, LoginFailures, SessionInfo, SESSION_TTL_SECS};
use crate::error::internal;

/// Intentos fallidos antes de bloquear el login de un usuario
pub(crate) const MAX_LOGIN_FAILURES: u32 = 5;
/// Duracion del bloqueo tras superar el maximo de intentos
pub(crate) const LOGIN_LOCK_SECS: u64 = 5 * 60;

// --- Sesiones persistentes ---

/// Crea una sesion (memoria + tabla `sessions`) y devuelve el token.
pub async fn create_session(
    state: &AppState,
    username: &str,
    role: UserRole,
    grants: Grants,
    account_expires: Option<i64>,
) -> Result<String, (StatusCode, String)> {
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let created_at = now_unix();

    let (t, u) = (token.clone(), username.to_string());
    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO sessions (token, username, created_at) VALUES (?1, ?2, ?3)",
            params![t, u, created_at],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await?;

    state.sessions.lock().await.insert(token.clone(), SessionInfo {
        username: username.to_string(),
        role,
        grants,
        account_expires,
        created_at,
    });
    Ok(token)
}

pub async fn remove_session(state: &AppState, token: &str) {
    state.sessions.lock().await.remove(token);
    let t = token.to_string();
    let _ = crate::db::db_op(&state.db, move |conn| {
        conn.execute("DELETE FROM sessions WHERE token = ?1", params![t])
            .map_err(|e| e.to_string())?;
        Ok(())
    }).await;
}

/// Carga las sesiones vigentes al arrancar (rol y permisos frescos desde web_users).
pub fn load_sessions(conn: &rusqlite::Connection) -> std::collections::HashMap<String, SessionInfo> {
    let cutoff = now_unix() - SESSION_TTL_SECS;
    let _ = conn.execute("DELETE FROM sessions WHERE created_at < ?1", params![cutoff]);

    let mut map = std::collections::HashMap::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT s.token, s.username, s.created_at FROM sessions s JOIN web_users u ON u.username = s.username",
    ) else {
        return map;
    };
    let rows: Vec<(String, String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    for (token, username, created_at) in rows {
        let Some((role, grants, account_expires)) = crate::roles::load_user(conn, &username) else { continue };
        map.insert(token, SessionInfo { username, role: UserRole(role), grants, account_expires, created_at });
    }
    map
}

/// Hash real (mismo costo) para verificar cuando el usuario no existe
fn dummy_hash() -> String {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DUMMY
        .get_or_init(|| bcrypt::hash("labnas-dummy-password", 10).unwrap_or_default())
        .clone()
}

pub(crate) async fn bcrypt_verify(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || bcrypt::verify(&password, &hash).unwrap_or(false))
        .await
        .unwrap_or(false)
}

pub(crate) async fn bcrypt_hash(password: String) -> Result<String, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || bcrypt::hash(&password, 10))
        .await
        .map_err(internal)?
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error hasheando contrasena".to_string()))
}

/// Cookie de sesion: HttpOnly (un XSS no puede leerla) y SameSite=Strict (no viaja
/// en pedidos desde otros sitios). `secure` cuando el navegador entro por HTTPS (puente).
pub const SESSION_COOKIE: &str = "labnas_session";

pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{}",
        SESSION_COOKIE,
        token,
        crate::state::SESSION_TTL_SECS,
        if secure { "; Secure" } else { "" }
    )
}

fn with_cookie(mut resp: Response, token: &str, secure: bool) -> Response {
    if let Ok(v) = axum::http::HeaderValue::from_str(&session_cookie(token, secure)) {
        resp.headers_mut().append(axum::http::header::SET_COOKIE, v);
    }
    resp
}

/// Aviso a las otras sesiones del usuario (toast y notificacion del visor): si no fue el, lo ve al instante
pub fn notify_internet_login(state: &AppState, username: &str, ip: &str) {
    crate::events::notify(
        state,
        crate::events::Audience::User(username.to_string()),
        None,
        crate::events::Level::Warning,
        "Nuevo inicio de sesión por internet",
        &format!("Se entró a tu cuenta por el puente desde {}. Si no fuiste tú, cambia la contraseña.", ip),
    );
}

pub(crate) const EXPIRED_MSG: &str = "Tu cuenta llego a su fecha de termino. Pidele al administrador que la extienda.";

/// Abre una sesion y responde como el login (token + cookie). La usan las passkeys.
pub async fn issue_login(
    state: &AppState,
    username: &str,
    secure: bool,
) -> Result<Response, (StatusCode, String)> {
    let u = username.to_string();
    let (role, grants, expires) = crate::db::db_op(&state.db, move |conn| Ok(crate::roles::load_user(conn, &u)))
        .await?
        .ok_or((StatusCode::UNAUTHORIZED, "Usuario no encontrado".to_string()))?;
    if crate::roles::is_expired(expires) {
        return Err((StatusCode::FORBIDDEN, EXPIRED_MSG.to_string()));
    }
    let role = UserRole(role);
    let token = create_session(state, username, role.clone(), grants, expires).await?;
    let modules = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_modules(conn))).await.unwrap_or_default();
    let cookie_token = token.clone();
    let resp = Json(AuthResponse { token, username: username.to_string(), role, enabled_modules: modules }).into_response();
    Ok(with_cookie(resp, &cookie_token, secure))
}

fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}


// --- Has Users (publico, sin auth) ---

pub async fn has_users(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let count: i64 = crate::db::db_op(&state.db, |conn| {
        conn.query_row("SELECT COUNT(*) FROM web_users", [], |row| row.get(0))
            .map_err(|e| e.to_string())
    }).await?;
    let has = count > 0;
    Ok(Json(serde_json::json!({ "has_users": has })))
}

// --- Register ---

pub async fn register(
    State(state): State<AppState>,
    bridge: Option<axum::Extension<crate::bridge::BridgeOrigin>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Response, (StatusCode, String)> {
    let username = req.username.trim().to_lowercase();
    if username.len() < 2 || username.len() > 32 {
        return Err((StatusCode::BAD_REQUEST, "Usuario debe tener entre 2 y 32 caracteres".to_string()));
    }
    if req.password.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, "Contrasena debe tener al menos 8 caracteres".to_string()));
    }
    if !username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        return Err((StatusCode::BAD_REQUEST, "Usuario solo puede contener letras, numeros, _ y .".to_string()));
    }

    let password_hash = bcrypt_hash(req.password.clone()).await?;

    let username_clone = username.clone();
    let (role, grants) = crate::db::db_op_status(&state.db, move |conn| {
        // IMMEDIATE: dos registros simultaneos no pueden crear dos admins
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
            .map_err(internal)?;
        let conn = &*tx;

        // Check if user already exists
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM web_users WHERE username = ?1",
            params![&username_clone],
            |row| row.get::<_, i64>(0),
        ).map_err(internal)?
        > 0;

        if exists {
            return Err((StatusCode::CONFLICT, "Usuario ya existe".to_string()));
        }

        // Primer usuario = admin; el resto queda pendiente hasta que un admin lo apruebe
        let user_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM web_users", [], |row| row.get(0),
        ).map_err(internal)?;

        let is_first = user_count == 0;
        let (role, grants) = if is_first { (UserRole::admin(), Grants::admin()) } else { (UserRole::pending(), Grants::default()) };

        conn.execute(
            "INSERT INTO web_users (username, password_hash, role) VALUES (?1, ?2, ?3)",
            params![&username_clone, &password_hash, role.as_str()],
        ).map_err(internal)?;

        tx.commit().map_err(internal)?;
        Ok((role, grants))
    }).await?;

    let token = create_session(&state, &username, role.clone(), grants, None).await?;

    let detail = if role.is_pending() { format!("{} (pendiente de aprobacion)", username) } else { username.clone() };
    state.log_activity("Registro", &detail, &username).await;

    if role.is_pending() {
        crate::events::notify(
            &state,
            crate::events::Audience::Admins,
            None,
            crate::events::Level::Info,
            "Usuario pendiente de aprobacion",
            &format!("{} creo una cuenta. Apruebala en Configuracion > Usuarios.", username),
        );
        let state_bg = state.clone();
        let name = username.clone();
        tokio::spawn(async move {
            crate::handlers::notifications::notify_admins(
                &state_bg,
                &format!("*Usuario pendiente*\n\n`{}` creo una cuenta en la web. Apruebala en Configuracion > Usuarios.", name),
            )
            .await;
        });
    }

    let modules = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_modules(conn))).await
        .unwrap_or_default();

    let cookie_token = token.clone();
    let resp = Json(AuthResponse {
        token,
        username,
        role,
        enabled_modules: modules,
    })
    .into_response();
    Ok(with_cookie(resp, &cookie_token, bridge.is_some()))
}

// --- Login ---

pub async fn login(
    State(state): State<AppState>,
    bridge: Option<axum::Extension<crate::bridge::BridgeOrigin>>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, (StatusCode, String)> {
    let username = req.username.trim().to_lowercase();

    // Bloqueo temporal tras demasiados intentos fallidos
    {
        let mut failures = state.login_failures.lock().await;
        if let Some(f) = failures.get(&username) {
            let elapsed = f.last.elapsed().as_secs();
            if f.count >= MAX_LOGIN_FAILURES && elapsed < LOGIN_LOCK_SECS {
                let mins = (LOGIN_LOCK_SECS - elapsed).div_ceil(60);
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    format!("Demasiados intentos fallidos. Espera {} min", mins),
                ));
            }
            if elapsed >= LOGIN_LOCK_SECS {
                failures.remove(&username);
            }
        }
    }

    let username_clone = username.clone();
    let user_opt = crate::db::db_op(&state.db, move |conn| {
        conn.query_row(
            "SELECT password_hash, role, expires_at, totp_secret IS NOT NULL
             FROM web_users WHERE username = ?1",
            params![&username_clone],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            },
        ).optional().map_err(|e| e.to_string())
    }).await?;

    // Usuario inexistente: igual se verifica contra un hash para no revelar si existe
    let (password_hash, expires, has_totp, exists) = match user_opt {
        Some((h, _, exp, totp)) => (h, exp, totp, true),
        None => (dummy_hash(), None, false, false),
    };

    let valid = bcrypt_verify(req.password.clone(), password_hash).await && exists;

    if !valid {
        login_failed(&state, &username).await;
        return Err((StatusCode::UNAUTHORIZED, "Usuario o contrasena incorrectos".to_string()));
    }

    // Doble factor: con la contrasena correcta se pide el codigo en un segundo paso
    if has_totp {
        let code = req.code.as_deref().map(str::trim).unwrap_or_default().to_string();
        if code.is_empty() {
            return Ok(Json(serde_json::json!({ "totp_required": true })).into_response());
        }
        let u = username.clone();
        let ok = crate::db::db_op(&state.db, move |conn| super::twofa::verify_second_factor(conn, &u, &code)).await?;
        if !ok {
            login_failed(&state, &username).await;
            return Err((StatusCode::UNAUTHORIZED, "Codigo de verificacion incorrecto".to_string()));
        }
    }
    state.login_failures.lock().await.remove(&username);

    // Cuenta con fecha de termino (p.ej. auditor): solo se avisa tras la contrasena correcta
    if crate::roles::is_expired(expires) {
        return Err((StatusCode::FORBIDDEN, EXPIRED_MSG.to_string()));
    }
    let u = username.clone();
    let (role_str, grants, _) = crate::db::db_op(&state.db, move |conn| Ok(crate::roles::load_user(conn, &u)))
        .await?
        .ok_or((StatusCode::UNAUTHORIZED, "Usuario o contrasena incorrectos".to_string()))?;
    let role = UserRole(role_str);

    let token = create_session(&state, &username, role.clone(), grants, expires).await?;

    let secure = bridge.is_some();
    // Inicio de sesion desde internet: queda en la auditoria con la IP real
    if let Some(axum::Extension(origin)) = bridge {
        state
            .log_activity("login_puente", &format!("Inicio de sesion por el puente PHP desde {}", origin.client_ip), &username)
            .await;
        notify_internet_login(&state, &username, &origin.client_ip);
    }

    let modules = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_modules(conn))).await
        .unwrap_or_default();

    let cookie_token = token.clone();
    let resp = Json(AuthResponse {
        token,
        username,
        role,
        enabled_modules: modules,
    })
    .into_response();
    Ok(with_cookie(resp, &cookie_token, secure))
}

/// Cuenta un intento fallido (bloquea tras MAX_LOGIN_FAILURES) y demora la respuesta
pub(crate) async fn login_failed(state: &AppState, username: &str) {
    {
        let mut failures = state.login_failures.lock().await;
        let entry = failures.entry(username.to_string()).or_insert(LoginFailures {
            count: 0,
            last: std::time::Instant::now(),
        });
        entry.count += 1;
        entry.last = std::time::Instant::now();
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}

// --- Me ---

/// Modulos para la interfaz: los que el rol no puede ver salen como apagados
pub async fn modules_for(state: &AppState, grants: &Grants) -> Vec<crate::db::ModuleInfo> {
    let mut modules = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_modules(conn))).await.unwrap_or_default();
    for m in modules.iter_mut() {
        m.enabled = m.enabled && grants.can(&m.id, crate::roles::Level::Ver);
    }
    modules
}

pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, StatusCode> {
    let token = extract_token(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&token).ok_or(StatusCode::UNAUTHORIZED)?;
    let username = session.username.clone();
    let role = session.role.clone();
    let grants = session.grants.clone();
    let expires_at = session.account_expires;
    drop(sessions);

    // Get linked telegram from DB
    let username_clone = username.clone();
    let role_id = role.as_str().to_string();
    let (linked, totp_enabled, role_name) = crate::db::db_op(&state.db, move |conn| {
        let row = conn.query_row(
            "SELECT linked_telegram, totp_secret IS NOT NULL FROM web_users WHERE username = ?1",
            params![&username_clone],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, bool>(1)?)),
        ).optional().map_err(|e| e.to_string())?;
        let name = crate::handlers::roles::role_name(conn, &role_id);
        Ok(row.map(|(l, t)| (l, t, name)))
    }).await.ok().flatten().unwrap_or((None, false, String::new()));

    let modules = modules_for(&state, &grants).await;

    Ok(Json(MeResponse {
        username,
        role,
        role_name,
        admin: grants.admin,
        levels: grants.all_levels(),
        expires_at,
        linked_telegram: linked,
        enabled_modules: modules,
        totp_enabled,
    }))
}

// --- Logout ---

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(token) = extract_token(&headers) {
        remove_session(&state, &token).await;
    }
    // Borra la cookie del navegador
    (
        [(axum::http::header::SET_COOKIE, format!("{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0", SESSION_COOKIE))],
        StatusCode::OK,
    )
}

// --- Change password ---

#[derive(Debug, serde::Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let token = extract_token(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&token)
        .ok_or((StatusCode::UNAUTHORIZED, "Sesion invalida".to_string()))?;
    let username = session.username.clone();
    drop(sessions);

    if req.new_password.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, "La nueva contrasena debe tener al menos 8 caracteres".to_string()));
    }

    // Get current hash
    let username_clone = username.clone();
    let current_hash = crate::db::db_op_status(&state.db, move |conn| {
        conn.query_row(
            "SELECT password_hash FROM web_users WHERE username = ?1",
            params![&username_clone],
            |row| row.get::<_, String>(0),
        ).optional()
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()))
    }).await?;

    if !bcrypt_verify(req.current_password.clone(), current_hash).await {
        return Err((StatusCode::UNAUTHORIZED, "Contrasena actual incorrecta".to_string()));
    }

    let new_hash = bcrypt_hash(req.new_password.clone()).await?;

    // Cambiar la contrasena cierra las demas sesiones del usuario
    let (username_clone2, token_clone) = (username.clone(), token.clone());
    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE web_users SET password_hash = ?1 WHERE username = ?2",
            params![&new_hash, &username_clone2],
        ).map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM sessions WHERE username = ?1 AND token != ?2",
            params![&username_clone2, &token_clone],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await?;
    state.sessions.lock().await.retain(|t, s| s.username != username || *t == token);
    crate::handlers::webdav::forget_credentials(&username);

    Ok(StatusCode::OK)
}

// --- List users (admin) ---

pub async fn list_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<UserRow>>, (StatusCode, String)> {
    let users = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT username, role, linked_telegram, totp_secret IS NOT NULL, expires_at FROM web_users ORDER BY username"
        ).map_err(|e| e.to_string())?;
        let mut users: Vec<UserRow> = stmt
            .query_map([], |row| {
                Ok(UserRow {
                    username: row.get(0)?,
                    role: UserRole(row.get(1)?),
                    linked_telegram: row.get(2)?,
                    totp_enabled: row.get(3)?,
                    expires_at: row.get(4)?,
                    overrides: Default::default(),
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        for u in users.iter_mut() {
            u.overrides = crate::roles::user_overrides(conn, &u.username);
        }
        Ok(users)
    }).await?;

    Ok(Json(users))
}

/// Lista solo los nombres de usuario (accesible para cualquier usuario autenticado)
pub async fn list_usernames(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let names = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare("SELECT username FROM web_users")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let mut names = Vec::new();
        for row in rows {
            names.push(row.map_err(|e| e.to_string())?);
        }
        Ok(names)
    }).await?;

    Ok(Json(names))
}

// --- Set user role (admin) ---

pub async fn set_user_role(
    State(state): State<AppState>,
    axum::Extension(me): axum::Extension<SessionInfo>,
    Path(username): Path<String>,
    Json(req): Json<SetWebUserRoleRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let role = req.role.clone();
    // Nunca quedarse sin administrador: no se quita el rol a uno mismo (otro admin
    // puede hacerlo) ni al ultimo admin
    if !role.is_admin() && username == me.username {
        return Err((StatusCode::BAD_REQUEST, "No puedes quitarte el rol de administrador; pidele a otro admin que lo haga".into()));
    }
    if role.is_admin() && req.expires_at.is_some() {
        return Err((StatusCode::BAD_REQUEST, "Un administrador no puede tener fecha de termino".into()));
    }
    let known = crate::modules::ids();
    if let Some(bad) = req.overrides.keys().find(|m| !known.contains(&m.as_str()) || *m == "dashboard") {
        return Err((StatusCode::BAD_REQUEST, format!("Modulo desconocido: {}", bad)));
    }

    let role_str = role.as_str().to_string();
    let username_clone = username.clone();
    let expires_at = req.expires_at;
    let overrides = req.overrides.clone();

    let demoting = !role.is_admin();
    crate::db::db_op_status(&state.db, move |conn| {
        if !crate::roles::role_exists(conn, &role_str) {
            return Err((StatusCode::BAD_REQUEST, "Ese rol no existe".to_string()));
        }
        if demoting {
            let (was_admin, admins): (bool, i64) = conn
                .query_row(
                    "SELECT (SELECT role FROM web_users WHERE username = ?1) = 'admin', (SELECT COUNT(*) FROM web_users WHERE role = 'admin')",
                    params![&username_clone],
                    |r| Ok((r.get::<_, Option<bool>>(0)?.unwrap_or(false), r.get(1)?)),
                )
                .map_err(internal)?;
            if was_admin && admins <= 1 {
                return Err((StatusCode::BAD_REQUEST, "Es el unico administrador: nombra a otro antes".to_string()));
            }
        }
        let tx = conn.unchecked_transaction().map_err(internal)?;
        let rows = tx.execute(
            "UPDATE web_users SET role = ?1, expires_at = ?2 WHERE username = ?3",
            params![&role_str, expires_at, &username_clone],
        ).map_err(internal)?;
        if rows == 0 {
            return Err((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()));
        }
        tx.execute("DELETE FROM user_module_overrides WHERE username = ?1", params![&username_clone]).map_err(internal)?;
        for (m, l) in &overrides {
            tx.execute(
                "INSERT INTO user_module_overrides (username, module_id, level) VALUES (?1, ?2, ?3)",
                params![&username_clone, m, *l as i64],
            ).map_err(internal)?;
        }
        // El chat de Telegram vinculado toma el mismo rol
        tx.execute(
            "UPDATE telegram_chats SET role = ?1 WHERE chat_id = (SELECT linked_telegram FROM web_users WHERE username = ?2)",
            params![&role_str, &username_clone],
        ).map_err(internal)?;
        tx.commit().map_err(internal)?;
        Ok(())
    }).await?;

    crate::roles::refresh_sessions(&state).await;
    state.log_activity("Rol de usuario", &format!("{} -> {}", username, role.as_str()), &me.username).await;
    // La UI del usuario recarga su rol/permisos al instante (p.ej. al ser aprobado)
    state.events.publish("auth.changed", (), crate::events::Audience::User(username.clone()), None);

    Ok(StatusCode::OK)
}

// --- Delete user (admin) ---

pub async fn delete_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let username_clone = username.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        let rows = conn.execute(
            "DELETE FROM web_users WHERE username = ?1",
            params![&username_clone],
        ).map_err(internal)?;

        if rows == 0 {
            return Err((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()));
        }
        Ok(())
    }).await?;

    // Remove sessions
    let mut sessions = state.sessions.lock().await;
    sessions.retain(|_, s| s.username != username);

    Ok(StatusCode::NO_CONTENT)
}

// --- Rename user (self) ---

pub async fn rename_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<crate::models::auth::RenameUserRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, String)> {
    let token = extract_token(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&token)
        .ok_or((StatusCode::UNAUTHORIZED, "Sesion invalida".to_string()))?;
    let old_username = session.username.clone();
    drop(sessions);

    let new_username = req.new_username.trim().to_lowercase();

    if new_username.len() < 2 || new_username.len() > 32 {
        return Err((StatusCode::BAD_REQUEST, "Usuario debe tener entre 2 y 32 caracteres".to_string()));
    }
    if !new_username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        return Err((StatusCode::BAD_REQUEST, "Usuario solo puede contener letras, numeros, _ y .".to_string()));
    }
    if new_username == old_username {
        return Err((StatusCode::BAD_REQUEST, "El nuevo nombre es igual al actual".to_string()));
    }

    let old_clone = old_username.clone();
    let new_clone = new_username.clone();
    let role = crate::db::db_op_status(&state.db, move |conn| {
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM web_users WHERE username = ?1",
            params![&new_clone],
            |row| row.get::<_, i64>(0),
        ).map_err(internal)?
        > 0;
        if exists {
            return Err((StatusCode::CONFLICT, "Ese nombre de usuario ya esta en uso".to_string()));
        }
        // Las tablas hijas (sesiones, passkeys, tokens, excepciones...) siguen por ON UPDATE CASCADE
        let tx = conn.unchecked_transaction().map_err(internal)?;
        let rows = tx.execute(
            "UPDATE web_users SET username = ?1 WHERE username = ?2",
            params![&new_clone, &old_clone],
        ).map_err(internal)?;
        if rows == 0 {
            return Err((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()));
        }
        tx.execute(
            "UPDATE telegram_chats SET linked_web_user = ?1 WHERE linked_web_user = ?2",
            params![&new_clone, &old_clone],
        ).map_err(internal)?;
        let role: String = tx
            .query_row("SELECT role FROM web_users WHERE username = ?1", params![&new_clone], |r| r.get(0))
            .map_err(internal)?;
        tx.commit().map_err(internal)?;
        Ok(UserRole(role))
    }).await?;

    // Actualizar todas las sesiones del usuario
    let mut sessions = state.sessions.lock().await;
    for session in sessions.values_mut() {
        if session.username == old_username {
            session.username = new_username.clone();
        }
    }

    state.log_activity("Renombrado", &format!("{} -> {}", old_username, new_username), &new_username).await;

    let modules = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_modules(conn))).await
        .unwrap_or_default();

    Ok(Json(AuthResponse {
        token,
        username: new_username,
        role,
        enabled_modules: modules,
    }))
}

// --- Generate link code (user requests from web) ---

pub async fn generate_link_code(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let token = extract_token(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&token)
        .ok_or((StatusCode::UNAUTHORIZED, "Sesion invalida".to_string()))?;
    let username = session.username.clone();
    drop(sessions);

    // Generate 8-char code
    let code: String = uuid::Uuid::new_v4().to_string()[..8].to_uppercase();

    let mut codes = state.link_codes.lock().await;
    // Remove old codes for this user
    codes.retain(|_, v| v.username != username);
    codes.insert(code.clone(), crate::state::LinkCode {
        username,
        created_at: std::time::Instant::now(),
    });

    Ok((StatusCode::OK, code))
}

// --- Admin links a telegram chat to a web user ---

#[derive(Debug, serde::Deserialize)]
pub struct LinkRequest {
    pub web_username: String,
}

pub async fn admin_link_chat(
    State(state): State<AppState>,
    Path(chat_id): Path<i64>,
    Json(req): Json<LinkRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let web_username = req.web_username.clone();

    crate::db::db_op_status(&state.db, move |conn| {
        // Verify web user exists and get their role/perms
        let (role_str, perm_t, perm_i, perm_a): (String, bool, bool, bool) = conn.query_row(
            "SELECT role, perm_terminal, perm_impresion, perm_archivos_escritura
             FROM web_users WHERE username = ?1",
            params![&web_username],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).optional()
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Usuario web no encontrado".to_string()))?;

        // Check chat exists
        let chat_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM telegram_chats WHERE chat_id = ?1",
            params![chat_id],
            |row| row.get::<_, i64>(0),
        ).map_err(internal)?
        > 0;

        if !chat_exists {
            return Err((StatusCode::NOT_FOUND, "Chat no encontrado".to_string()));
        }

        // Link telegram chat
        conn.execute(
            "UPDATE telegram_chats SET linked_web_user = ?1, role = ?2, perm_terminal = ?3, perm_impresion = ?4, perm_archivos_escritura = ?5
             WHERE chat_id = ?6",
            params![&web_username, &role_str, perm_t, perm_i, perm_a, chat_id],
        ).map_err(internal)?;

        // Link web user back
        conn.execute(
            "UPDATE web_users SET linked_telegram = ?1 WHERE username = ?2",
            params![chat_id, &web_username],
        ).map_err(internal)?;

        Ok(())
    }).await?;

    Ok(StatusCode::OK)
}
