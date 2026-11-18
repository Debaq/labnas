//! Correo: cuentas IMAP/POP3, filtros, clasificacion con IA y avisos.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use std::time::Duration;

use crate::db::db_op;
use crate::models::email::{EmailAccount, EmailFilter, EmailMessage, FilterAction, MailProtocol};
use crate::state::AppState;

mod ai;
mod api;
mod fetch;
mod filters;
mod ics;
mod monitor;
mod oauth;
mod proposals;
mod store;
mod telegram;

// API publica (handlers y loops) y visibilidad entre submodulos (`use super::*`)
#[allow(unused_imports)]
pub use {api::*, fetch::*, filters::*, monitor::*, oauth::*, proposals::*, telegram::*};

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
