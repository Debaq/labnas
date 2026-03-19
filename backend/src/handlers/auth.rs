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
        .find(|u| u.username == username)
        .ok_or((StatusCode::UNAUTHORIZED, "Usuario o contrasena incorrectos".to_string()))?;

    let valid = bcrypt::verify(&req.password, &user.password_hash)
        .unwrap_or(false);

    if !valid {
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

    Ok(Json(MeResponse {
        username: session.username.clone(),
        role: session.role.clone(),
        permissions: session.permissions.clone(),
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
        })
        .collect();
    Json(users)
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
