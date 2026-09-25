//! Tareas, proyectos, calendario y categorias de eventos.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Deserialize;

use std::collections::HashMap;

use crate::db::db_op;
use crate::models::notifications::UserRole;
use crate::models::tasks::*;
use crate::state::AppState;


mod projects;
mod task_crud;
mod schedule;
mod events;
mod categories;

// API publica (handlers y loops) y visibilidad entre submodulos (`use super::*`)
#[allow(unused_imports)]
pub use {projects::*, task_crud::*, schedule::*, events::*, categories::*};

// ---- helpers para leer Task / Project / CalendarEvent / EventCategory desde rows ----

fn row_to_task(row: &rusqlite::Row) -> Result<Task, rusqlite::Error> {
    let status_str: String = row.get("status")?;
    let status = match status_str.as_str() {
        "enprogreso" => TaskStatus::EnProgreso,
        "completada" => TaskStatus::Completada,
        "rechazada" => TaskStatus::Rechazada,
        _ => TaskStatus::Pendiente,
    };
    let assigned_to: String = row.get("assigned_to")?;
    let confirmed_by: String = row.get("confirmed_by")?;
    let rejected_by: String = row.get("rejected_by")?;
    let created_at_str: String = row.get("created_at")?;
    let last_reminder_str: Option<String> = row.get("last_reminder")?;

    Ok(Task {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        title: row.get("title")?,
        description: row.get("description")?,
        assigned_to: serde_json::from_str(&assigned_to).unwrap_or_default(),
        status,
        created_by: row.get("created_by")?,
        due_date: row.get("due_date")?,
        due_time: row.get("due_time")?,
        requires_confirmation: row.get::<_, i32>("requires_confirmation")? != 0,
        insistent: row.get::<_, i32>("insistent")? != 0,
        reminder_minutes: row.get::<_, u32>("reminder_minutes")?,
        confirmed_by: serde_json::from_str(&confirmed_by).unwrap_or_default(),
        rejected_by: serde_json::from_str(&rejected_by).unwrap_or_default(),
        created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        last_reminder: last_reminder_str.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        }),
    })
}

fn row_to_project(row: &rusqlite::Row) -> Result<Project, rusqlite::Error> {
    let members_str: String = row.get("members")?;
    let tags_str: String = row.get("member_tags")?;
    let created_at_str: String = row.get("created_at")?;

    Ok(Project {
        id: row.get("id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        created_by: row.get("created_by")?,
        members: serde_json::from_str(&members_str).unwrap_or_default(),
        member_tags: serde_json::from_str(&tags_str).unwrap_or_default(),
        created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_event(row: &rusqlite::Row) -> Result<CalendarEvent, rusqlite::Error> {
    let invitees_str: String = row.get("invitees")?;
    let accepted_str: String = row.get("accepted")?;
    let declined_str: String = row.get("declined")?;
    let created_at_str: String = row.get("created_at")?;

    Ok(CalendarEvent {
        id: row.get("id")?,
        title: row.get("title")?,
        description: row.get("description")?,
        date: row.get("date")?,
        time: row.get("time")?,
        end_time: row.get("end_time")?,
        location: row.get("location")?,
        created_by: row.get("created_by")?,
        invitees: serde_json::from_str(&invitees_str).unwrap_or_default(),
        accepted: serde_json::from_str(&accepted_str).unwrap_or_default(),
        declined: serde_json::from_str(&declined_str).unwrap_or_default(),
        remind_before_min: row.get::<_, u32>("remind_before_min")?,
        reminded: row.get::<_, i32>("reminded")? != 0,
        notify_telegram: row.get::<_, i32>("notify_telegram")? != 0,
        recurrence: row.get("recurrence")?,
        recurrence_end: row.get("recurrence_end")?,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        category: row.get("category")?,
    })
}

fn task_status_str(s: &TaskStatus) -> &'static str {
    match s {
        TaskStatus::Pendiente => "pendiente",
        TaskStatus::EnProgreso => "enprogreso",
        TaskStatus::Completada => "completada",
        TaskStatus::Rechazada => "rechazada",
    }
}

// ---- Notificaciones Telegram para tareas ----

/// (chat_id, name, username, role, linked_web_user)
type TelegramChatRow = (i64, String, Option<String>, String, Option<String>);

async fn notify_task_assigned(state: &AppState, task: &Task) {
    let pool = state.db.clone();
    let (token, chats) = match crate::db::db_op(&pool, |conn| {
        let token: Option<String> = conn
            .query_row(
                "SELECT labnas_decrypt(bot_token) FROM notification_config WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("bot_token: {}", e))?
            .flatten();

        let mut stmt = conn
            .prepare("SELECT chat_id, name, username, role, linked_web_user FROM telegram_chats")
            .map_err(|e| format!("telegram_chats: {}", e))?;
        let chats: Vec<TelegramChatRow> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .map_err(|e| format!("telegram_chats query: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok((token, chats))
    })
    .await
    {
        Ok((Some(t), c)) => (t, c),
        _ => return,
    };

    // @all solo aplica a operadores y admins, no observadores
    let target_ids: Vec<i64> = if task.assigned_to.contains(&"all".to_string()) {
        chats
            .iter()
            .filter(|c| c.3 == "admin" || c.3 == "operador")
            .map(|c| c.0)
            .collect()
    } else {
        chats
            .iter()
            .filter(|c| {
                task.assigned_to.iter().any(|a| {
                    a.to_lowercase() == c.1.to_lowercase()
                        || c.4
                            .as_ref()
                            .map(|u| u.to_lowercase() == a.to_lowercase())
                            .unwrap_or(false)
                })
            })
            .map(|c| c.0)
            .collect()
    };

    if target_ids.is_empty() {
        return;
    }

    let due = task
        .due_date
        .as_deref()
        .unwrap_or("sin fecha limite");
    let msg = format!(
        "📋 *Nueva tarea asignada*\n\n*{}*\nPor: {}\nVence: {}\nID: `{}`\n\n`/confirmar {}` o `/rechazar {}`",
        task.title, task.created_by, due, task.id, task.id, task.id
    );

    for id in target_ids {
        let _ = crate::handlers::notifications::send_tg_public(
            &state.http_client,
            &token,
            id,
            &msg,
        )
        .await;
    }
}

async fn notify_task_completed(state: &AppState, task: &Task, completed_by: &str) {
    let pool = state.db.clone();
    let (token, chats) = match crate::db::db_op(&pool, |conn| {
        let token: Option<String> = conn
            .query_row(
                "SELECT labnas_decrypt(bot_token) FROM notification_config WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("bot_token: {}", e))?
            .flatten();

        let mut stmt = conn
            .prepare("SELECT chat_id, name, linked_web_user FROM telegram_chats")
            .map_err(|e| format!("telegram_chats: {}", e))?;
        let chats: Vec<(i64, String, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| format!("telegram_chats query: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok((token, chats))
    })
    .await
    {
        Ok((Some(t), c)) => (t, c),
        _ => return,
    };

    // Notificar al creador
    let creator_id: Option<i64> = chats
        .iter()
        .find(|c| {
            c.1.to_lowercase() == task.created_by.to_lowercase()
                || c.2
                    .as_ref()
                    .map(|u| u.to_lowercase() == task.created_by.to_lowercase())
                    .unwrap_or(false)
        })
        .map(|c| c.0);

    if let Some(id) = creator_id {
        let msg = format!(
            "✅ *Tarea completada*\n\n*{}*\nCompletada por: {}\nID: `{}`",
            task.title, completed_by, task.id
        );
        let _ = crate::handlers::notifications::send_tg_public(
            &state.http_client,
            &token,
            id,
            &msg,
        )
        .await;
    }
}

/// Extrae el username de la sesión a partir del header Authorization
fn extract_username(_state: &AppState, sessions: &std::collections::HashMap<String, crate::state::SessionInfo>, headers: &HeaderMap) -> Option<(String, UserRole)> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())?;
    let session = sessions.get(&token)?;
    Some((session.username.clone(), session.role.clone()))
}
