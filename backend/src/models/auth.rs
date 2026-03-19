use serde::{Deserialize, Serialize};

use super::notifications::{UserPermissions, UserRole};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebUser {
    pub username: String,
    pub password_hash: String,
    #[serde(default)]
    pub role: UserRole,
    #[serde(default)]
    pub permissions: UserPermissions,
    #[serde(default)]
    pub linked_telegram: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub username: String,
    pub role: UserRole,
    pub permissions: UserPermissions,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub username: String,
    pub role: UserRole,
    pub permissions: UserPermissions,
    pub linked_telegram: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SetWebUserRoleRequest {
    pub role: UserRole,
    #[serde(default)]
    pub permissions: Option<UserPermissions>,
}
