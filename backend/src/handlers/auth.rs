use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rusqlite::{params, OptionalExtension};

use crate::models::auth::*;
use crate::models::notifications::{UserPermissions, UserRole};
use crate::state::{AppState, SessionInfo};

fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn role_from_str(s: &str) -> UserRole {
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
    if req.password.len() < 4 {
        return Err((StatusCode::BAD_REQUEST, "Contrasena debe tener al menos 4 caracteres".to_string()));
    }
    if !username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        return Err((StatusCode::BAD_REQUEST, "Usuario solo puede contener letras, numeros, _ y .".to_string()));
    }

    let password_hash = bcrypt::hash(&req.password, 8)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error hasheando contrasena".to_string()))?;

    let username_clone = username.clone();
    let (role, permissions) = crate::db::db_op_status(&state.db, move |conn| {
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

        // First user = admin, rest = observador
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
            (UserRole::Observador, UserPermissions {
                terminal: false,
                impresion: true,
                archivos_escritura: false,
            })
        };

        conn.execute(
            "INSERT INTO web_users (username, password_hash, role, perm_terminal, perm_impresion, perm_archivos_escritura)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &username_clone, &password_hash, role_to_str(&role),
                permissions.terminal, permissions.impresion, permissions.archivos_escritura,
            ],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok((role, permissions))
    }).await?;

    // Create session
    let token = uuid::Uuid::new_v4().to_string();
    let mut sessions = state.sessions.lock().await;
    sessions.insert(token.clone(), SessionInfo {
        username: username.clone(),
        role: role.clone(),
        permissions: permissions.clone(),
        created_at: std::time::Instant::now(),
    });

    state.log_activity("Registro", &username, &username).await;

    Ok(Json(AuthResponse {
        token,
        username,
        role,
        permissions,
    }))
}

// --- Login ---

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, String)> {
    let username = req.username.trim().to_lowercase();

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

    let Some((password_hash, role_str, perm_terminal, perm_impresion, perm_archivos)) = user_opt else {
        // Rate limiting: frenar fuerza bruta (mismo delay que password incorrecto)
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        return Err((StatusCode::UNAUTHORIZED, "Usuario o contrasena incorrectos".to_string()));
    };

    let valid = bcrypt::verify(&req.password, &password_hash)
        .unwrap_or(false);

    if !valid {
        // Rate limiting: frenar fuerza bruta
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        return Err((StatusCode::UNAUTHORIZED, "Usuario o contrasena incorrectos".to_string()));
    }

    let role = role_from_str(&role_str);
    let permissions = UserPermissions {
        terminal: perm_terminal,
        impresion: perm_impresion,
        archivos_escritura: perm_archivos,
    };

    let token = uuid::Uuid::new_v4().to_string();
    let mut sessions = state.sessions.lock().await;
    sessions.insert(token.clone(), SessionInfo {
        username: username.clone(),
        role: role.clone(),
        permissions: permissions.clone(),
        created_at: std::time::Instant::now(),
    });

    Ok(Json(AuthResponse {
        token,
        username,
        role,
        permissions,
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

    Ok(Json(MeResponse {
        username,
        role,
        permissions,
        linked_telegram: linked,
    }))
}

// --- Logout ---

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> StatusCode {
    if let Some(token) = extract_token(&headers) {
        let mut sessions = state.sessions.lock().await;
        sessions.remove(&token);
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

    if req.new_password.len() < 4 {
        return Err((StatusCode::BAD_REQUEST, "La nueva contrasena debe tener al menos 4 caracteres".to_string()));
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

    let valid = bcrypt::verify(&req.current_password, &current_hash).unwrap_or(false);
    if !valid {
        return Err((StatusCode::UNAUTHORIZED, "Contrasena actual incorrecta".to_string()));
    }

    let new_hash = bcrypt::hash(&req.new_password, 8)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error hasheando".to_string()))?;

    let username_clone2 = username.clone();
    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE web_users SET password_hash = ?1 WHERE username = ?2",
            params![&new_hash, &username_clone2],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await?;

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

    let mut sessions = state.sessions.lock().await;
    for session in sessions.values_mut() {
        if session.username == username {
            session.role = new_role.clone();
            session.permissions = new_perms.clone();
        }
    }

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

    Ok(Json(AuthResponse {
        token: token,
        username: new_username,
        role,
        permissions,
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
