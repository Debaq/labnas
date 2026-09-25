//! Bot de Telegram y notificaciones.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::time::Duration;
use sysinfo::{Disks, System};

use chrono::Timelike;
use rusqlite::params;
use rusqlite::OptionalExtension;

use crate::models::notifications::*;
use crate::state::AppState;


mod api;
mod telegram;
mod bot;
mod terminal;
mod calendar;
mod tasks;
mod reminders;
mod printers;
mod system_info;
mod music;

// Los submodulos se ven entre si a traves de este modulo (`use super::*`)
#[allow(unused_imports)]
use {api::*, telegram::*, bot::*, terminal::*, calendar::*, tasks::*, reminders::*, printers::*, system_info::*, music::*};

// API publica del modulo (usada por main.rs y otros handlers)
pub use api::{delete_bot_token, delete_chat, get_config, send_test, set_bot_token, set_chat_role, set_schedule};
pub use bot::telegram_bot_loop;
pub use reminders::{daily_notification_loop, task_reminder_loop};
pub use telegram::{notify_active_chats, notify_admins, send_tg_public};

/// Permisos por defecto segun el rol (cuando no se envian explicitamente)
pub fn default_permissions_for_role(role: &UserRole) -> UserPermissions {
    match role {
        UserRole::Admin => UserPermissions {
            terminal: true,
            impresion: true,
            archivos_escritura: true,
        },
        UserRole::Operador => UserPermissions {
            terminal: true,
            impresion: true,
            archivos_escritura: true,
        },
        UserRole::Observador => UserPermissions {
            terminal: false,
            impresion: true,
            archivos_escritura: false,
        },
        UserRole::Pendiente => UserPermissions::default(),
    }
}

fn parse_role(s: &str) -> UserRole {
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

/// Respuesta sanitizada de NotificationConfig que nunca expone el bot_token
#[derive(serde::Serialize)]
pub struct NotificationConfigResponse {
    pub bot_configured: bool,
    pub bot_username: Option<String>,
    pub telegram_chats: Vec<TelegramChat>,
    pub daily_enabled: bool,
    pub daily_hour: u8,
    pub daily_minute: u8,
}

// =====================
// DB helper: read notification_config singleton
// =====================

/// (bot_token, bot_username, daily_enabled, daily_hour, daily_minute)
type NotifConfigRow = (Option<String>, Option<String>, bool, u8, u8);

fn read_notif_config(conn: &rusqlite::Connection) -> Result<NotifConfigRow, String> {
    conn.query_row(
        "SELECT labnas_decrypt(bot_token), bot_username, daily_enabled, daily_hour, daily_minute FROM notification_config WHERE id = 1",
        [],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, u8>(3)?,
                row.get::<_, u8>(4)?,
            ))
        },
    )
    .optional()
    .map_err(|e| format!("DB: {}", e))?
    .ok_or_else(|| "notification_config not found".to_string())
}

fn read_bot_token(conn: &rusqlite::Connection) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT labnas_decrypt(bot_token) FROM notification_config WHERE id = 1",
        [],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(|e| format!("DB: {}", e))
    .map(|o| o.flatten())
}

fn read_all_chats(conn: &rusqlite::Connection) -> Result<Vec<TelegramChat>, String> {
    let mut stmt = conn
        .prepare("SELECT chat_id, name, username, role, perm_terminal, perm_impresion, perm_archivos_escritura, linked_web_user, daily_enabled, daily_hour, daily_minute FROM telegram_chats")
        .map_err(|e| format!("DB: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            let role_str: String = row.get(3)?;
            Ok(TelegramChat {
                chat_id: row.get(0)?,
                name: row.get(1)?,
                username: row.get(2)?,
                role: parse_role(&role_str),
                permissions: UserPermissions {
                    terminal: row.get(4)?,
                    impresion: row.get(5)?,
                    archivos_escritura: row.get(6)?,
                },
                linked_web_user: row.get(7)?,
                daily_enabled: row.get(8)?,
                daily_hour: row.get(9)?,
                daily_minute: row.get(10)?,
            })
        })
        .map_err(|e| format!("DB: {}", e))?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r.map_err(|e| format!("DB row: {}", e))?);
    }
    Ok(list)
}

fn read_chat(conn: &rusqlite::Connection, chat_id: i64) -> Result<Option<TelegramChat>, String> {
    conn.query_row(
        "SELECT chat_id, name, username, role, perm_terminal, perm_impresion, perm_archivos_escritura, linked_web_user, daily_enabled, daily_hour, daily_minute FROM telegram_chats WHERE chat_id = ?1",
        params![chat_id],
        |row| {
            let role_str: String = row.get(3)?;
            Ok(TelegramChat {
                chat_id: row.get(0)?,
                name: row.get(1)?,
                username: row.get(2)?,
                role: parse_role(&role_str),
                permissions: UserPermissions {
                    terminal: row.get(4)?,
                    impresion: row.get(5)?,
                    archivos_escritura: row.get(6)?,
                },
                linked_web_user: row.get(7)?,
                daily_enabled: row.get(8)?,
                daily_hour: row.get(9)?,
                daily_minute: row.get(10)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("DB: {}", e))
}


// =====================
// Utilities
// =====================

fn progress_bar(pct: f64) -> String {
    let filled = (pct / 10.0).round() as usize;
    let empty = 10_usize.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let k = 1024_f64;
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    let i = (bytes as f64).log(k).floor() as usize;
    let i = i.min(sizes.len() - 1);
    format!("{:.1} {}", bytes as f64 / k.powi(i as i32), sizes[i])
}
