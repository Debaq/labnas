use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};

use crate::config::save_config;
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

// --- Has Users (publico, sin auth) ---

pub async fn has_users(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let config = state.config.lock().await;
    let has = !config.web_users.is_empty();
    Json(serde_json::json!({ "has_users": has }))
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

    let mut config = state.config.lock().await;

    if config.web_users.iter().any(|u| u.username == username) {
        return Err((StatusCode::CONFLICT, "Usuario ya existe".to_string()));
    }

    let password_hash = bcrypt::hash(&req.password, 8)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error hasheando contrasena".to_string()))?;

    // First user = admin, rest = observador (auto-approved with print)
    let is_first = config.web_users.is_empty();
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

    config.web_users.push(WebUser {
        username: username.clone(),
        password_hash,
        role: role.clone(),
        permissions: permissions.clone(),
        linked_telegram: None,
    });

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    drop(config);

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

    let config = state.config.lock().await;
    let user = config
        .web_users
        .iter()
        .find(|u| u.username == username);

    let Some(user) = user else {
        drop(config);
        // Rate limiting: frenar fuerza bruta (mismo delay que password incorrecto)
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        return Err((StatusCode::UNAUTHORIZED, "Usuario o contrasena incorrectos".to_string()));
    };

    let valid = bcrypt::verify(&req.password, &user.password_hash)
        .unwrap_or(false);

    if !valid {
        drop(config);
        // Rate limiting: frenar fuerza bruta
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        return Err((StatusCode::UNAUTHORIZED, "Usuario o contrasena incorrectos".to_string()));
    }

    let role = user.role.clone();
    let permissions = user.permissions.clone();
    drop(config);

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

    // Get linked telegram from config
    let config = state.config.lock().await;
    let linked = config.web_users.iter()
        .find(|u| u.username == session.username)
        .and_then(|u| u.linked_telegram);

    Ok(Json(MeResponse {
        username: session.username.clone(),
        role: session.role.clone(),
        permissions: session.permissions.clone(),
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

    let mut config = state.config.lock().await;
    let user = config.web_users.iter_mut()
        .find(|u| u.username == username)
        .ok_or((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()))?;

    let valid = bcrypt::verify(&req.current_password, &user.password_hash).unwrap_or(false);
    if !valid {
        return Err((StatusCode::UNAUTHORIZED, "Contrasena actual incorrecta".to_string()));
    }

    user.password_hash = bcrypt::hash(&req.new_password, 8)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error hasheando".to_string()))?;

    save_config(&config).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::OK)
}

// --- List users (admin) ---

pub async fn list_users(
    State(state): State<AppState>,
) -> Json<Vec<MeResponse>> {
    let config = state.config.lock().await;
    let users: Vec<MeResponse> = config
        .web_users
        .iter()
        .map(|u| MeResponse {
            username: u.username.clone(),
            role: u.role.clone(),
            permissions: u.permissions.clone(),
            linked_telegram: u.linked_telegram,
        })
        .collect();
    Json(users)
}

/// Lista solo los nombres de usuario (accesible para cualquier usuario autenticado)
pub async fn list_usernames(
    State(state): State<AppState>,
) -> Json<Vec<String>> {
    let config = state.config.lock().await;
    let names: Vec<String> = config
        .web_users
        .iter()
        .map(|u| u.username.clone())
        .collect();
    Json(names)
}

// --- Set user role (admin) ---

pub async fn set_user_role(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Json(req): Json<SetWebUserRoleRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;

    let user = config
        .web_users
        .iter_mut()
        .find(|u| u.username == username)
        .ok_or((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()))?;

    user.role = req.role.clone();
    if let Some(perms) = req.permissions {
        user.permissions = perms;
    }
    let new_role = user.role.clone();
    let new_perms = user.permissions.clone();

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    drop(config);

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
    let mut config = state.config.lock().await;
    let before = config.web_users.len();
    config.web_users.retain(|u| u.username != username);

    if config.web_users.len() == before {
        return Err((StatusCode::NOT_FOUND, "Usuario no encontrado".to_string()));
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    drop(config);

    // Remove sessions
    let mut sessions = state.sessions.lock().await;
    sessions.retain(|_, s| s.username != username);

    Ok(StatusCode::NO_CONTENT)
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
    let mut config = state.config.lock().await;

    // Verify web user exists
    let web_user = config.web_users.iter()
        .find(|u| u.username == req.web_username)
        .ok_or((StatusCode::NOT_FOUND, "Usuario web no encontrado".to_string()))?;
    let role = web_user.role.clone();
    let perms = web_user.permissions.clone();

    // Link telegram chat
    let chat = config.notifications.telegram_chats.iter_mut()
        .find(|c| c.chat_id == chat_id)
        .ok_or((StatusCode::NOT_FOUND, "Chat no encontrado".to_string()))?;

    chat.linked_web_user = Some(req.web_username.clone());
    chat.role = role;
    chat.permissions = perms;

    // Link web user back
    if let Some(wu) = config.web_users.iter_mut().find(|u| u.username == req.web_username) {
        wu.linked_telegram = Some(chat_id);
    }

    save_config(&config).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::OK)
}
