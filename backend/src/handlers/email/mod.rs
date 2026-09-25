//! Correo: cuentas IMAP/POP3, filtros, clasificacion con IA y avisos.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

use crate::db::{db_op, db_op_status, get_conn};
use crate::models::email::{EmailAccount, EmailFilter, EmailMessage, FilterAction, MailProtocol};
use crate::models::notifications::UserRole;
use crate::state::AppState;


mod api;
mod filters;
mod fetch;
mod ai;
mod monitor;
mod telegram;

// API publica (handlers y loops) y visibilidad entre submodulos (`use super::*`)
#[allow(unused_imports)]
pub use {api::*, filters::*, fetch::*, ai::*, monitor::*, telegram::*};

/// Extrae el username de la sesion a partir del header Authorization
fn extract_username(
    sessions: &HashMap<String, crate::state::SessionInfo>,
    headers: &HeaderMap,
) -> Option<(String, UserRole)> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())?;
    let session = sessions.get(&token)?;
    Some((session.username.clone(), session.role.clone()))
}

// =====================
// Request types
// =====================

#[derive(Debug, Deserialize)]
pub struct ConfigureAccountRequest {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub protocol: crate::models::email::MailProtocol,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SetGroqKeyRequest {
    pub key: String,
}
