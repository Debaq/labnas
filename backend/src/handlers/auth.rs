use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rusqlite::{params, OptionalExtension};

use crate::models::auth::*;
use crate::models::notifications::{UserPermissions, UserRole};
use crate::state::{now_unix, AppState, LoginFailures, SessionInfo, SESSION_TTL_SECS};

/// Intentos fallidos antes de bloquear el login de un usuario
const MAX_LOGIN_FAILURES: u32 = 5;
/// Duracion del bloqueo tras superar el maximo de intentos
const LOGIN_LOCK_SECS: u64 = 5 * 60;

// --- Sesiones persistentes ---

/// Crea una sesion (memoria + tabla `sessions`) y devuelve el token.
pub async fn create_session(
    state: &AppState,
    username: &str,
    role: UserRole,
    permissions: UserPermissions,
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
        permissions,
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
        "SELECT s.token, s.username, s.created_at, u.role, u.perm_terminal, u.perm_impresion, u.perm_archivos_escritura
         FROM sessions s JOIN web_users u ON u.username = s.username",
    ) else {
        return map;
    };
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            SessionInfo {
                username: row.get(1)?,
                created_at: row.get(2)?,
                role: role_from_str(&row.get::<_, String>(3)?),
                permissions: UserPermissions {
                    terminal: row.get(4)?,
                    impresion: row.get(5)?,
                    archivos_escritura: row.get(6)?,
                },
            },
        ))
    });
    if let Ok(rows) = rows {
        for (token, session) in rows.flatten() {
            map.insert(token, session);
        }
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

async fn bcrypt_verify(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || bcrypt::verify(&password, &hash).unwrap_or(false))
        .await
        .unwrap_or(false)
}

async fn bcrypt_hash(password: String) -> Result<String, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || bcrypt::hash(&password, 10))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error hasheando contrasena".to_string()))
}

fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub fn role_from_str(s: &str) -> UserRole {
    match s {
        "admin" => UserRole::Admin,
        "operador" => UserRole::Operador,
        "observador" => UserRole::Observador,
        _ => UserRole::Pendiente,
    }
}

fn role_to_str(role: &UserRole) -> &'static str {
    match role {
        UserRole::Admin => "admin",
        UserRole::Operador => "operador",
        UserRole::Observador => "observador",
        UserRole::Pendiente => "pendiente",
    }
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
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, String)> {
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
    let (role, permissions) = crate::db::db_op_status(&state.db, move |conn| {
        // IMMEDIATE: dos registros simultaneos no pueden crear dos admins
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let conn = &*tx;

        // Check if user already exists
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM web_users WHERE username = ?1",
            params![&username_clone],
            |row| row.get::<_, i64>(0),
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        > 0;

        if exists {
            return Err((StatusCode::CONFLICT, "Usuario ya existe".to_string()));
        }

        // Primer usuario = admin; el resto queda pendiente hasta que un admin lo apruebe
        let user_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM web_users", [], |row| row.get(0),
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let is_first = user_count == 0;
        let (role, permissions) = if is_first {
            (UserRole::Admin, UserPermissions {
                terminal: true,
                impresion: true,
                archivos_escritura: true,
            })
        } else {
            (UserRole::Pendiente, UserPermissions::default())
        };

        conn.execute(
            "INSERT INTO web_users (username, password_hash, role, perm_terminal, perm_impresion, perm_archivos_escritura)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &username_clone, &password_hash, role_to_str(&role),
                permissions.terminal, permissions.impresion, permissions.archivos_escritura,
            ],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        tx.commit().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok((role, permissions))
    }).await?;

    let token = create_session(&state, &username, role.clone(), permissions.clone()).await?;

    let detail = if role == UserRole::Pendiente { format!("{} (pendiente de aprobacion)", username) } else { username.clone() };
    state.log_activity("Registro", &detail, &username).await;

    if role == UserRole::Pendiente {
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

    Ok(Json(AuthResponse {
        token,
        username,
        role,
        permissions,
        enabled_modules: modules,
    }))
}

// --- Login ---

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, String)> {
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
            "SELECT password_hash, role, perm_terminal, perm_impresion, perm_archivos_escritura
             FROM web_users WHERE username = ?1",
            params![&username_clone],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, bool>(4)?,
                ))
            },
        ).optional().map_err(|e| e.to_string())
    }).await?;

    // Usuario inexistente: igual se verifica contra un hash para no revelar si existe
    let (password_hash, role_str, perm_terminal, perm_impresion, perm_archivos, exists) = match user_opt {
        Some((h, r, t, i, a)) => (h, r, t, i, a, true),
        None => (dummy_hash(), String::new(), false, false, false, false),
    };

    let valid = bcrypt_verify(req.password.clone(), password_hash).await && exists;

    if !valid {
        {
            let mut failures = state.login_failures.lock().await;
            let entry = failures.entry(username.clone()).or_insert(LoginFailures {
                count: 0,
                last: std::time::Instant::now(),
            });
            entry.count += 1;
            entry.last = std::time::Instant::now();
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        return Err((StatusCode::UNAUTHORIZED, "Usuario o contrasena incorrectos".to_string()));
    }
    state.login_failures.lock().await.remove(&username);

    let role = role_from_str(&role_str);
    let permissions = UserPermissions {
        terminal: perm_terminal,
        impresion: perm_impresion,
        archivos_escritura: perm_archivos,
    };

    let token = create_session(&state, &username, role.clone(), permissions.clone()).await?;

    let modules = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_modules(conn))).await
        .unwrap_or_default();

    Ok(Json(AuthResponse {
        token,
        username,
        role,
        permissions,
        enabled_modules: modules,
    }))
}

// --- Me ---

pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, StatusCode> {
    let token = extract_token(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&token).ok_or(StatusCode::UNAUTHORIZED)?;
    let username = session.username.clone();
    let role = session.role.clone();
    let permissions = session.permissions.clone();
    drop(sessions);

    // Get linked telegram from DB
    let username_clone = username.clone();
    let linked = crate::db::db_op(&state.db, move |conn| {
        conn.query_row(
            "SELECT linked_telegram FROM web_users WHERE username = ?1",
            params![&username_clone],
            |row| row.get::<_, Option<i64>>(0),
        ).optional().map_err(|e| e.to_string())
    }).await.ok().flatten().flatten();

    let modules = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_modules(conn))).await
        .unwrap_or_default();

    Ok(Json(MeResponse {
        username,
        role,
        permissions,
        linked_telegram: linked,
        enabled_modules: modules,
    }))
}

// --- Logout ---

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> StatusCode {
    if let Some(token) = extract_token(&headers) {
        remove_session(&state, &token).await;
    }
    StatusCode::OK
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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
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

    Ok(StatusCode::OK)
}

// --- List users (admin) ---

pub async fn list_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<MeResponse>>, (StatusCode, String)> {
    let users = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT username, role, perm_terminal, perm_impresion, perm_archivos_escritura, linked_telegram
             FROM web_users"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([], |row| {
            Ok(MeResponse {
                username: row.get(0)?,
                role: role_from_str(&row.get::<_, String>(1)?),
                permissions: UserPermissions {
                    terminal: row.get(2)?,
                    impresion: row.get(3)?,
                    archivos_escritura: row.get(4)?,
                },
                linked_telegram: row.get(5)?,
                enabled_modules: vec![],
            })
        }).map_err(|e| e.to_string())?;

        let mut users = Vec::new();
        for row in rows {
            users.push(row.map_err(|e| e.to_string())?);
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
    Path(username): Path<String>,
    Json(req): Json<SetWebUserRoleRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let role = req.role.clone();
    let permissions = req.permissions.clone().unwrap_or_else(|| {
        crate::handlers::notifications::default_permissions_for_role(&role)
    });

    let role_str = role_to_str(&role).to_string();
    let new_role = role.clone();
    let new_perms = permissions.clone();
    let username_clone = username.clone();

    let linked_tg = crate::db::db_op_status(&state.db, move |conn| {
        // Update web user
        let rows = conn.execute(
            "UPDATE web_users SET role = ?1, perm_terminal = ?2, perm_impresion = ?3, perm_archivos_escritura = ?4
             WHERE username = ?5",
            params![&role_str, permissions.terminal, permissions.impresion, permissions.archivos_escritura, &username_clone],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if rows == 0 {
            return Err((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()));
        }

        // Get linked telegram
        let linked: Option<i64> = conn.query_row(
            "SELECT linked_telegram FROM web_users WHERE username = ?1",
            params![&username_clone],
            |row| row.get(0),
        ).optional()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .flatten();

        // Sync role to linked telegram chat
        if let Some(tg_id) = linked {
            conn.execute(
                "UPDATE telegram_chats SET role = ?1, perm_terminal = ?2, perm_impresion = ?3, perm_archivos_escritura = ?4
                 WHERE chat_id = ?5",
                params![&role_str, new_perms.terminal, new_perms.impresion, new_perms.archivos_escritura, tg_id],
            ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }

        Ok(linked)
    }).await?;

    let _ = linked_tg; // used inside closure

    {
        let mut sessions = state.sessions.lock().await;
        for session in sessions.values_mut() {
            if session.username == username {
                session.role = new_role.clone();
                session.permissions = new_perms.clone();
            }
        }
    }
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
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
    let (role, permissions) = crate::db::db_op_status(&state.db, move |conn| {
        // Check if new username already exists
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM web_users WHERE username = ?1",
            params![&new_clone],
            |row| row.get::<_, i64>(0),
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        > 0;

        if exists {
            return Err((StatusCode::CONFLICT, "Ese nombre de usuario ya esta en uso".to_string()));
        }

        // Get current user data
        let (password_hash, role_str, perm_t, perm_i, perm_a, linked_tg): (String, String, bool, bool, bool, Option<i64>) =
            conn.query_row(
                "SELECT password_hash, role, perm_terminal, perm_impresion, perm_archivos_escritura, linked_telegram
                 FROM web_users WHERE username = ?1",
                params![&old_clone],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            ).optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()))?;

        // Insert new row with new username
        conn.execute(
            "INSERT INTO web_users (username, password_hash, role, perm_terminal, perm_impresion, perm_archivos_escritura, linked_telegram)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![&new_clone, &password_hash, &role_str, perm_t, perm_i, perm_a, linked_tg],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Mover sesiones a la fila nueva antes de borrar la vieja (FK ON DELETE CASCADE)
        conn.execute(
            "UPDATE sessions SET username = ?1 WHERE username = ?2",
            params![&new_clone, &old_clone],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Delete old row
        conn.execute(
            "DELETE FROM web_users WHERE username = ?1",
            params![&old_clone],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Update linked_web_user in telegram_chats
        conn.execute(
            "UPDATE telegram_chats SET linked_web_user = ?1 WHERE linked_web_user = ?2",
            params![&new_clone, &old_clone],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let role = role_from_str(&role_str);
        let permissions = UserPermissions {
            terminal: perm_t,
            impresion: perm_i,
            archivos_escritura: perm_a,
        };

        Ok((role, permissions))
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
        permissions,
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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Usuario web no encontrado".to_string()))?;

        // Check chat exists
        let chat_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM telegram_chats WHERE chat_id = ?1",
            params![chat_id],
            |row| row.get::<_, i64>(0),
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        > 0;

        if !chat_exists {
            return Err((StatusCode::NOT_FOUND, "Chat no encontrado".to_string()));
        }

        // Link telegram chat
        conn.execute(
            "UPDATE telegram_chats SET linked_web_user = ?1, role = ?2, perm_terminal = ?3, perm_impresion = ?4, perm_archivos_escritura = ?5
             WHERE chat_id = ?6",
            params![&web_username, &role_str, perm_t, perm_i, perm_a, chat_id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Link web user back
        conn.execute(
            "UPDATE web_users SET linked_telegram = ?1 WHERE username = ?2",
            params![chat_id, &web_username],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok(())
    }).await?;

    Ok(StatusCode::OK)
}
