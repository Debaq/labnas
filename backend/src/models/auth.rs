use serde::{Deserialize, Serialize};

use super::notifications::{UserPermissions, UserRole};
use crate::db::ModuleInfo;

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
    /// Codigo TOTP o de recuperacion (cuentas con doble factor)
    #[serde(default)]
    pub code: Option<String>,
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
    pub enabled_modules: Vec<ModuleInfo>,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub username: String,
    pub role: UserRole,
    pub permissions: UserPermissions,
    pub linked_telegram: Option<i64>,
    pub enabled_modules: Vec<ModuleInfo>,
    /// Doble factor activo
    pub totp_enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetWebUserRoleRequest {
    pub role: UserRole,
    #[serde(default)]
    pub permissions: Option<UserPermissions>,
}

#[derive(Debug, Deserialize)]
pub struct RenameUserRequest {
    pub new_username: String,
}
