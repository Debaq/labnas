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

async fn notify_task_assigned(state: &AppState, task: &Task) {
    let pool = state.db.clone();
    let (token, chats) = match crate::db::db_op(&pool, |conn| {
        let token: Option<String> = conn
            .query_row(
                "SELECT bot_token FROM notification_config WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("bot_token: {}", e))?
            .flatten();

        let mut stmt = conn
            .prepare("SELECT chat_id, name, username, role, linked_web_user FROM telegram_chats")
            .map_err(|e| format!("telegram_chats: {}", e))?;
        let chats: Vec<(i64, String, Option<String>, String, Option<String>)> = stmt
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
                "SELECT bot_token FROM notification_config WHERE id = 1",
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

// ---- Proyectos ----

pub async fn list_projects(State(state): State<AppState>) -> Result<Json<Vec<Project>>, (StatusCode, String)> {
    let projects = db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, description, created_by, members, member_tags, created_at FROM projects ORDER BY created_at")
            .map_err(|e| format!("list_projects: {}", e))?;
        let rows = stmt
            .query_map([], |row| row_to_project(row))
            .map_err(|e| format!("list_projects query: {}", e))?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        Ok(rows)
    })
    .await?;
    Ok(Json(projects))
}

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

pub async fn create_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Project>), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let project = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name.clone(),
        description: req.description,
        created_by: username.clone(),
        members: vec![username.clone()],
        member_tags: HashMap::new(),
        created_at: Utc::now(),
    };

    let p = project.clone();
    db_op(&state.db, move |conn| {
        let members_json = serde_json::to_string(&p.members).unwrap_or_else(|_| "[]".into());
        let tags_json = serde_json::to_string(&p.member_tags).unwrap_or_else(|_| "{}".into());
        conn.execute(
            "INSERT INTO projects (id, name, description, created_by, members, member_tags, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![p.id, p.name, p.description, p.created_by, members_json, tags_json, p.created_at.to_rfc3339()],
        ).map_err(|e| format!("create_project: {}", e))?;
        Ok(())
    })
    .await?;

    state
        .log_activity("proyecto_creado", &format!("Proyecto: {}", req.name), &username)
        .await;

    Ok((StatusCode::CREATED, Json(project)))
}

pub async fn delete_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let name = db_op(&state.db, move |conn| {
        let (created_by, name): (String, String) = conn
            .query_row(
                "SELECT created_by, name FROM projects WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| "Proyecto no encontrado".to_string())?;

        // Solo el creador o admin puede eliminar
        if created_by != uname && role != UserRole::Admin {
            return Err("Sin permisos para eliminar este proyecto".to_string());
        }

        conn.execute("DELETE FROM projects WHERE id = ?1", params![id])
            .map_err(|e| format!("delete_project: {}", e))?;

        Ok(name)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrado") {
            (StatusCode::NOT_FOUND, msg)
        } else if msg.contains("permisos") {
            (StatusCode::FORBIDDEN, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    state
        .log_activity("proyecto_eliminado", &format!("Proyecto: {}", name), &username)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub members: Option<Vec<String>>,
    pub member_tags: Option<HashMap<String, Vec<String>>>,
}

pub async fn update_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> Result<Json<Project>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let updated = db_op(&state.db, move |conn| {
        // Leer proyecto actual
        let mut project = conn
            .query_row(
                "SELECT id, name, description, created_by, members, member_tags, created_at FROM projects WHERE id = ?1",
                params![id],
                |row| row_to_project(row),
            )
            .map_err(|_| "Proyecto no encontrado".to_string())?;

        if project.created_by != uname && role != UserRole::Admin {
            return Err("Sin permisos para modificar este proyecto".to_string());
        }

        if let Some(name) = req.name {
            project.name = name;
        }
        if let Some(description) = req.description {
            project.description = description;
        }
        if let Some(members) = req.members {
            project.members = members;
        }
        if let Some(member_tags) = req.member_tags {
            project.member_tags = member_tags;
        }

        let members_json = serde_json::to_string(&project.members).unwrap_or_else(|_| "[]".into());
        let tags_json = serde_json::to_string(&project.member_tags).unwrap_or_else(|_| "{}".into());
        conn.execute(
            "UPDATE projects SET name = ?1, description = ?2, members = ?3, member_tags = ?4 WHERE id = ?5",
            params![project.name, project.description, members_json, tags_json, project.id],
        )
        .map_err(|e| format!("update_project: {}", e))?;

        Ok(project)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrado") {
            (StatusCode::NOT_FOUND, msg)
        } else if msg.contains("permisos") {
            (StatusCode::FORBIDDEN, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    state
        .log_activity("proyecto_actualizado", &format!("Proyecto: {}", updated.name), &username)
        .await;

    Ok(Json(updated))
}

// ---- Tareas ----

#[derive(Deserialize)]
pub struct TasksQuery {
    pub project: Option<String>,
    pub status: Option<String>,
}

pub async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TasksQuery>,
) -> Result<Json<Vec<Task>>, (StatusCode, String)> {
    let tasks = db_op(&state.db, move |conn| {
        let mut sql = "SELECT id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder FROM tasks".to_string();
        let mut conditions = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref project_id) = query.project {
            conditions.push("project_id = ?".to_string());
            param_values.push(Box::new(project_id.clone()));
        }

        if let Some(ref status_str) = query.status {
            let valid = matches!(status_str.as_str(), "pendiente" | "enprogreso" | "completada" | "rechazada");
            if valid {
                conditions.push("status = ?".to_string());
                param_values.push(Box::new(status_str.clone()));
            }
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(" ORDER BY created_at");

        let mut stmt = conn.prepare(&sql).map_err(|e| format!("list_tasks: {}", e))?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| row_to_task(row))
            .map_err(|e| format!("list_tasks query: {}", e))?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        Ok(rows)
    })
    .await?;
    Ok(Json(tasks))
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub assigned_to: Vec<String>,
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub insistent: bool,
    #[serde(default = "default_reminder")]
    pub reminder_minutes: u32,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub due_time: Option<String>,
}

fn default_reminder() -> u32 {
    8
}

pub async fn create_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Task>), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let task = Task {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: req.project_id,
        title: req.title.clone(),
        description: String::new(),
        assigned_to: req.assigned_to,
        status: TaskStatus::Pendiente,
        created_by: username.clone(),
        due_date: req.due_date,
        due_time: req.due_time,
        requires_confirmation: req.requires_confirmation,
        insistent: req.insistent,
        reminder_minutes: req.reminder_minutes,
        confirmed_by: Vec::new(),
        rejected_by: Vec::new(),
        created_at: Utc::now(),
        last_reminder: None,
    };

    let t = task.clone();
    db_op(&state.db, move |conn| {
        let assigned_json = serde_json::to_string(&t.assigned_to).unwrap_or_else(|_| "[]".into());
        let confirmed_json = serde_json::to_string(&t.confirmed_by).unwrap_or_else(|_| "[]".into());
        let rejected_json = serde_json::to_string(&t.rejected_by).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT INTO tasks (id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                t.id, t.project_id, t.title, t.description,
                assigned_json, task_status_str(&t.status), t.created_by,
                t.due_date, t.due_time,
                t.requires_confirmation as i32, t.insistent as i32, t.reminder_minutes,
                confirmed_json, rejected_json,
                t.created_at.to_rfc3339(),
                t.last_reminder.map(|dt| dt.to_rfc3339()),
            ],
        ).map_err(|e| format!("create_task: {}", e))?;
        Ok(())
    })
    .await?;

    state
        .log_activity("tarea_creada", &format!("Tarea: {}", req.title), &username)
        .await;

    // Notificar a los asignados por Telegram
    if !task.assigned_to.is_empty() {
        notify_task_assigned(&state, &task).await;
    }

    Ok((StatusCode::CREATED, Json(task)))
}

#[derive(Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub status: Option<String>,
    pub project_id: Option<Option<String>>,
    pub assigned_to: Option<Vec<String>>,
    pub due_date: Option<Option<String>>,
    pub due_time: Option<Option<String>>,
    pub requires_confirmation: Option<bool>,
    pub insistent: Option<bool>,
    pub reminder_minutes: Option<u32>,
}

pub async fn update_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let updated = db_op(&state.db, move |conn| {
        let mut task = conn
            .query_row(
                "SELECT id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder FROM tasks WHERE id = ?1",
                params![id],
                |row| row_to_task(row),
            )
            .map_err(|_| "Tarea no encontrada".to_string())?;

        // Solo el creador, asignados o admin pueden actualizar
        let is_assigned = task.assigned_to.contains(&"all".to_string())
            || task.assigned_to.iter().any(|a| a.to_lowercase() == uname.to_lowercase());
        if task.created_by != uname && !is_assigned && role != UserRole::Admin {
            return Err("Sin permisos para modificar esta tarea".to_string());
        }

        if let Some(title) = req.title {
            task.title = title;
        }
        if let Some(status_str) = &req.status {
            task.status = match status_str.as_str() {
                "pendiente" => TaskStatus::Pendiente,
                "enprogreso" => TaskStatus::EnProgreso,
                "completada" => TaskStatus::Completada,
                "rechazada" => TaskStatus::Rechazada,
                _ => return Err("Estado invalido".to_string()),
            };
        }
        if let Some(project_id) = req.project_id {
            task.project_id = project_id;
        }
        if let Some(assigned_to) = req.assigned_to {
            task.assigned_to = assigned_to;
        }
        if let Some(due_date) = req.due_date {
            task.due_date = due_date;
        }
        if let Some(due_time) = req.due_time {
            task.due_time = due_time;
        }
        if let Some(requires_confirmation) = req.requires_confirmation {
            task.requires_confirmation = requires_confirmation;
        }
        if let Some(insistent) = req.insistent {
            task.insistent = insistent;
        }
        if let Some(reminder_minutes) = req.reminder_minutes {
            task.reminder_minutes = reminder_minutes;
        }

        let assigned_json = serde_json::to_string(&task.assigned_to).unwrap_or_else(|_| "[]".into());
        let confirmed_json = serde_json::to_string(&task.confirmed_by).unwrap_or_else(|_| "[]".into());
        let rejected_json = serde_json::to_string(&task.rejected_by).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE tasks SET title = ?1, status = ?2, project_id = ?3, assigned_to = ?4, due_date = ?5, due_time = ?6, requires_confirmation = ?7, insistent = ?8, reminder_minutes = ?9, confirmed_by = ?10, rejected_by = ?11 WHERE id = ?12",
            params![
                task.title, task_status_str(&task.status), task.project_id,
                assigned_json, task.due_date, task.due_time,
                task.requires_confirmation as i32, task.insistent as i32, task.reminder_minutes,
                confirmed_json, rejected_json,
                task.id,
            ],
        ).map_err(|e| format!("update_task: {}", e))?;

        Ok(task)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else if msg.contains("permisos") {
            (StatusCode::FORBIDDEN, msg)
        } else if msg.contains("invalido") {
            (StatusCode::BAD_REQUEST, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    state
        .log_activity(
            "tarea_actualizada",
            &format!("Tarea: {}", updated.title),
            &username,
        )
        .await;

    Ok(Json(updated))
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct ConfirmRejectRequest {
    pub user: String,
}

pub async fn confirm_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(_req): Json<ConfirmRejectRequest>,
) -> Result<Json<Task>, (StatusCode, String)> {
    // Ignorar user del body, usar el de la sesion
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let (updated, created_event_opt) = db_op(&state.db, move |conn| {
        let mut task = conn
            .query_row(
                "SELECT id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder FROM tasks WHERE id = ?1",
                params![id],
                |row| row_to_task(row),
            )
            .map_err(|_| "Tarea no encontrada".to_string())?;

        if !task.confirmed_by.contains(&uname) {
            task.confirmed_by.push(uname.clone());
        }
        // Quitar de rechazados si estaba
        task.rejected_by.retain(|u| u != &uname);

        let confirmed_json = serde_json::to_string(&task.confirmed_by).unwrap_or_else(|_| "[]".into());
        let rejected_json = serde_json::to_string(&task.rejected_by).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE tasks SET confirmed_by = ?1, rejected_by = ?2 WHERE id = ?3",
            params![confirmed_json, rejected_json, task.id],
        ).map_err(|e| format!("confirm_task: {}", e))?;

        // Si la tarea tiene fecha y hora, crear evento automaticamente
        let mut created_event: Option<CalendarEvent> = None;
        if let (Some(date), Some(time)) = (task.due_date.clone(), task.due_time.clone()) {
            let event = CalendarEvent {
                id: uuid::Uuid::new_v4().to_string()[..6].to_string(),
                title: task.title.clone(),
                description: format!("Actividad desde tarea ({})", task.id),
                date,
                time,
                end_time: None,
                location: None,
                created_by: task.created_by.clone(),
                invitees: task.assigned_to.clone(),
                accepted: vec![uname.clone()],
                declined: Vec::new(),
                remind_before_min: 15,
                reminded: false,
                notify_telegram: true,
                recurrence: String::new(),
                recurrence_end: None,
                created_at: Utc::now(),
                category: None,
            };

            let invitees_json = serde_json::to_string(&event.invitees).unwrap_or_else(|_| "[]".into());
            let accepted_json = serde_json::to_string(&event.accepted).unwrap_or_else(|_| "[]".into());
            let declined_json = serde_json::to_string(&event.declined).unwrap_or_else(|_| "[]".into());
            conn.execute(
                "INSERT INTO calendar_events (id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    event.id, event.title, event.description, event.date, event.time,
                    event.end_time, event.location, event.created_by,
                    invitees_json, accepted_json, declined_json,
                    event.remind_before_min, event.reminded as i32, event.notify_telegram as i32,
                    event.recurrence, event.recurrence_end,
                    event.created_at.to_rfc3339(), event.category,
                ],
            ).map_err(|e| format!("confirm_task event: {}", e))?;

            created_event = Some(event);
        }

        Ok((task, created_event))
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    if created_event_opt.is_some() {
        state
            .log_activity(
                "tarea_confirmada_agendada",
                &format!("{} confirmo y agendo: {}", username, updated.title),
                &username,
            )
            .await;
    } else {
        state
            .log_activity(
                "tarea_confirmada",
                &format!("{} confirmo: {}", username, updated.title),
                &username,
            )
            .await;
    }

    Ok(Json(updated))
}

pub async fn reject_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(_req): Json<ConfirmRejectRequest>,
) -> Result<Json<Task>, (StatusCode, String)> {
    // Ignorar user del body, usar el de la sesion
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let updated = db_op(&state.db, move |conn| {
        let mut task = conn
            .query_row(
                "SELECT id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder FROM tasks WHERE id = ?1",
                params![id],
                |row| row_to_task(row),
            )
            .map_err(|_| "Tarea no encontrada".to_string())?;

        if !task.rejected_by.contains(&uname) {
            task.rejected_by.push(uname.clone());
        }
        // Quitar de confirmados si estaba
        task.confirmed_by.retain(|u| u != &uname);

        let confirmed_json = serde_json::to_string(&task.confirmed_by).unwrap_or_else(|_| "[]".into());
        let rejected_json = serde_json::to_string(&task.rejected_by).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE tasks SET confirmed_by = ?1, rejected_by = ?2 WHERE id = ?3",
            params![confirmed_json, rejected_json, task.id],
        ).map_err(|e| format!("reject_task: {}", e))?;

        Ok(task)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    state
        .log_activity(
            "tarea_rechazada",
            &format!("{} rechazo: {}", username, updated.title),
            &username,
        )
        .await;

    Ok(Json(updated))
}

pub async fn done_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let updated = db_op(&state.db, move |conn| {
        let mut task = conn
            .query_row(
                "SELECT id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder FROM tasks WHERE id = ?1",
                params![id],
                |row| row_to_task(row),
            )
            .map_err(|_| "Tarea no encontrada".to_string())?;

        // Solo asignados o creador pueden marcar completada
        let is_assigned = task.assigned_to.contains(&"all".to_string())
            || task.assigned_to.iter().any(|a| a.to_lowercase() == uname.to_lowercase());
        if task.created_by != uname && !is_assigned {
            return Err("Sin permisos para completar esta tarea".to_string());
        }

        task.status = TaskStatus::Completada;
        conn.execute(
            "UPDATE tasks SET status = ?1 WHERE id = ?2",
            params![task_status_str(&task.status), task.id],
        ).map_err(|e| format!("done_task: {}", e))?;

        Ok(task)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else if msg.contains("permisos") {
            (StatusCode::FORBIDDEN, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    state
        .log_activity(
            "tarea_completada",
            &format!("Tarea: {}", updated.title),
            &username,
        )
        .await;

    // Notificar al creador por Telegram
    notify_task_completed(&state, &updated, &username).await;

    Ok(Json(updated))
}

pub async fn delete_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let title = db_op(&state.db, move |conn| {
        let (created_by, title): (String, String) = conn
            .query_row(
                "SELECT created_by, title FROM tasks WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| "Tarea no encontrada".to_string())?;

        // Solo el creador o admin puede eliminar
        if created_by != uname && role != UserRole::Admin {
            return Err("Sin permisos para eliminar esta tarea".to_string());
        }

        conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])
            .map_err(|e| format!("delete_task: {}", e))?;

        Ok(title)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else if msg.contains("permisos") {
            (StatusCode::FORBIDDEN, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    state
        .log_activity("tarea_eliminada", &format!("Tarea: {}", title), &username)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

// ---- Agendar tarea como evento ----

#[derive(Debug, Deserialize)]
pub struct ScheduleTaskRequest {
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
}

pub async fn schedule_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ScheduleTaskRequest>,
) -> Result<Json<CalendarEvent>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let event = db_op(&state.db, move |conn| {
        let mut task = conn
            .query_row(
                "SELECT id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder FROM tasks WHERE id = ?1",
                params![id],
                |row| row_to_task(row),
            )
            .map_err(|_| "Tarea no encontrada".to_string())?;

        // Usar fecha/hora de la tarea si existen, o los del request
        let date = req.date.or_else(|| task.due_date.clone())
            .ok_or("Se requiere fecha".to_string())?;
        let time = req.time.or_else(|| task.due_time.clone())
            .ok_or("Se requiere hora".to_string())?;

        // Actualizar la tarea con fecha/hora si no los tenia
        if task.due_date.is_none() {
            task.due_date = Some(date.clone());
        }
        if task.due_time.is_none() {
            task.due_time = Some(time.clone());
        }

        conn.execute(
            "UPDATE tasks SET due_date = ?1, due_time = ?2 WHERE id = ?3",
            params![task.due_date, task.due_time, task.id],
        ).map_err(|e| format!("schedule_task update: {}", e))?;

        let event = CalendarEvent {
            id: uuid::Uuid::new_v4().to_string()[..6].to_string(),
            title: task.title.clone(),
            description: format!("Actividad desde tarea ({})", task.id),
            date,
            time,
            end_time: None,
            location: None,
            created_by: task.created_by.clone(),
            invitees: task.assigned_to.clone(),
            accepted: vec![uname],
            declined: Vec::new(),
            remind_before_min: 15,
            reminded: false,
            notify_telegram: true,
            recurrence: String::new(),
            recurrence_end: None,
            created_at: Utc::now(),
            category: None,
        };

        let invitees_json = serde_json::to_string(&event.invitees).unwrap_or_else(|_| "[]".into());
        let accepted_json = serde_json::to_string(&event.accepted).unwrap_or_else(|_| "[]".into());
        let declined_json = serde_json::to_string(&event.declined).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT INTO calendar_events (id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                event.id, event.title, event.description, event.date, event.time,
                event.end_time, event.location, event.created_by,
                invitees_json, accepted_json, declined_json,
                event.remind_before_min, event.reminded as i32, event.notify_telegram as i32,
                event.recurrence, event.recurrence_end,
                event.created_at.to_rfc3339(), event.category,
            ],
        ).map_err(|e| format!("schedule_task event: {}", e))?;

        Ok(event)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else if msg.contains("requiere") {
            (StatusCode::BAD_REQUEST, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    state
        .log_activity("tarea_agendada", &format!("Agendada: {}", event.title), &username)
        .await;

    Ok(Json(event))
}

// ---- Eventos / Calendario ----

#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    pub title: String,
    pub date: String,
    pub time: String,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub invitees: Vec<String>,
    #[serde(default = "default_event_rem")]
    pub remind_before_min: u32,
    #[serde(default = "default_notify_tg")]
    pub notify_telegram: bool,
    #[serde(default)]
    pub recurrence: String,
    #[serde(default)]
    pub recurrence_end: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

fn default_notify_tg() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct UpdateEventRequest {
    pub title: Option<String>,
    pub date: Option<String>,
    pub time: Option<String>,
    pub end_time: Option<Option<String>>,
    pub description: Option<String>,
    pub location: Option<Option<String>>,
    pub invitees: Option<Vec<String>>,
    pub remind_before_min: Option<u32>,
    pub notify_telegram: Option<bool>,
    pub recurrence: Option<String>,
    pub recurrence_end: Option<Option<String>>,
    pub category: Option<Option<String>>,
}

fn default_event_rem() -> u32 { 15 }

pub async fn list_events(State(state): State<AppState>) -> Result<Json<Vec<CalendarEvent>>, (StatusCode, String)> {
    let events = db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare("SELECT id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category FROM calendar_events ORDER BY date, time")
            .map_err(|e| format!("list_events: {}", e))?;
        let rows = stmt
            .query_map([], |row| row_to_event(row))
            .map_err(|e| format!("list_events query: {}", e))?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        Ok(rows)
    })
    .await?;
    Ok(Json(events))
}

pub async fn create_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateEventRequest>,
) -> Result<Json<CalendarEvent>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    if req.title.trim().is_empty() || req.date.is_empty() || req.time.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Titulo, fecha y hora requeridos".to_string()));
    }
    let event = CalendarEvent {
        id: uuid::Uuid::new_v4().to_string()[..6].to_string(),
        title: req.title.trim().to_string(),
        description: req.description,
        date: req.date,
        time: req.time,
        end_time: req.end_time,
        location: req.location,
        created_by: username.clone(),
        invitees: req.invitees,
        accepted: Vec::new(),
        declined: Vec::new(),
        remind_before_min: req.remind_before_min,
        reminded: false,
        notify_telegram: req.notify_telegram,
        recurrence: req.recurrence,
        recurrence_end: req.recurrence_end,
        created_at: Utc::now(),
        category: req.category,
    };

    let e = event.clone();
    db_op(&state.db, move |conn| {
        let invitees_json = serde_json::to_string(&e.invitees).unwrap_or_else(|_| "[]".into());
        let accepted_json = serde_json::to_string(&e.accepted).unwrap_or_else(|_| "[]".into());
        let declined_json = serde_json::to_string(&e.declined).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT INTO calendar_events (id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                e.id, e.title, e.description, e.date, e.time,
                e.end_time, e.location, e.created_by,
                invitees_json, accepted_json, declined_json,
                e.remind_before_min, e.reminded as i32, e.notify_telegram as i32,
                e.recurrence, e.recurrence_end,
                e.created_at.to_rfc3339(), e.category,
            ],
        ).map_err(|e| format!("create_event: {}", e))?;
        Ok(())
    })
    .await?;

    state.log_activity("evento", &event.title, &username).await;
    Ok(Json(event))
}

pub async fn update_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateEventRequest>,
) -> Result<Json<CalendarEvent>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (_username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let result = db_op(&state.db, move |conn| {
        let mut event = conn
            .query_row(
                "SELECT id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category FROM calendar_events WHERE id = ?1",
                params![id],
                |row| row_to_event(row),
            )
            .map_err(|_| "Evento no encontrado".to_string())?;

        if let Some(title) = req.title { event.title = title; }
        if let Some(date) = req.date { event.date = date; }
        if let Some(time) = req.time { event.time = time; }
        if let Some(end_time) = req.end_time { event.end_time = end_time; }
        if let Some(description) = req.description { event.description = description; }
        if let Some(location) = req.location { event.location = location; }
        if let Some(invitees) = req.invitees { event.invitees = invitees; }
        if let Some(remind) = req.remind_before_min { event.remind_before_min = remind; }
        if let Some(notify) = req.notify_telegram { event.notify_telegram = notify; }
        if let Some(recurrence) = req.recurrence { event.recurrence = recurrence; }
        if let Some(recurrence_end) = req.recurrence_end { event.recurrence_end = recurrence_end; }
        if let Some(category) = req.category { event.category = category; }

        let invitees_json = serde_json::to_string(&event.invitees).unwrap_or_else(|_| "[]".into());
        let accepted_json = serde_json::to_string(&event.accepted).unwrap_or_else(|_| "[]".into());
        let declined_json = serde_json::to_string(&event.declined).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE calendar_events SET title = ?1, description = ?2, date = ?3, time = ?4, end_time = ?5, location = ?6, invitees = ?7, accepted = ?8, declined = ?9, remind_before_min = ?10, notify_telegram = ?11, recurrence = ?12, recurrence_end = ?13, category = ?14 WHERE id = ?15",
            params![
                event.title, event.description, event.date, event.time,
                event.end_time, event.location,
                invitees_json, accepted_json, declined_json,
                event.remind_before_min, event.notify_telegram as i32,
                event.recurrence, event.recurrence_end, event.category,
                event.id,
            ],
        ).map_err(|e| format!("update_event: {}", e))?;

        Ok(event)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrado") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    Ok(Json(result))
}

pub async fn delete_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    db_op(&state.db, move |conn| {
        let created_by: String = conn
            .query_row(
                "SELECT created_by FROM calendar_events WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|_| "Evento no encontrado".to_string())?;

        // Solo el creador o admin puede eliminar
        if created_by != username && role != UserRole::Admin {
            return Err("Sin permisos para eliminar este evento".to_string());
        }

        conn.execute("DELETE FROM calendar_events WHERE id = ?1", params![id])
            .map_err(|e| format!("delete_event: {}", e))?;

        Ok(())
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrado") {
            (StatusCode::NOT_FOUND, msg)
        } else if msg.contains("permisos") {
            (StatusCode::FORBIDDEN, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct EventUserAction {
    pub user: String,
}

pub async fn accept_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(_req): Json<EventUserAction>,
) -> Result<Json<CalendarEvent>, (StatusCode, String)> {
    // Ignorar user del body, usar el de la sesion
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let result = db_op(&state.db, move |conn| {
        let mut event = conn
            .query_row(
                "SELECT id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category FROM calendar_events WHERE id = ?1",
                params![id],
                |row| row_to_event(row),
            )
            .map_err(|_| "Evento no encontrado".to_string())?;

        event.declined.retain(|u| u != &username);
        if !event.accepted.contains(&username) {
            event.accepted.push(username);
        }

        let accepted_json = serde_json::to_string(&event.accepted).unwrap_or_else(|_| "[]".into());
        let declined_json = serde_json::to_string(&event.declined).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE calendar_events SET accepted = ?1, declined = ?2 WHERE id = ?3",
            params![accepted_json, declined_json, event.id],
        ).map_err(|e| format!("accept_event: {}", e))?;

        Ok(event)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrado") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    Ok(Json(result))
}

pub async fn decline_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(_req): Json<EventUserAction>,
) -> Result<Json<CalendarEvent>, (StatusCode, String)> {
    // Ignorar user del body, usar el de la sesion
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&state, &sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let result = db_op(&state.db, move |conn| {
        let mut event = conn
            .query_row(
                "SELECT id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category FROM calendar_events WHERE id = ?1",
                params![id],
                |row| row_to_event(row),
            )
            .map_err(|_| "Evento no encontrado".to_string())?;

        event.accepted.retain(|u| u != &username);
        if !event.declined.contains(&username) {
            event.declined.push(username);
        }

        let accepted_json = serde_json::to_string(&event.accepted).unwrap_or_else(|_| "[]".into());
        let declined_json = serde_json::to_string(&event.declined).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE calendar_events SET accepted = ?1, declined = ?2 WHERE id = ?3",
            params![accepted_json, declined_json, event.id],
        ).map_err(|e| format!("decline_event: {}", e))?;

        Ok(event)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrado") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    Ok(Json(result))
}

// ── Event Categories ──

use crate::models::tasks::EventCategory;

pub async fn list_categories(State(state): State<AppState>) -> Result<Json<Vec<EventCategory>>, (StatusCode, String)> {
    let cats = db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, color FROM event_categories ORDER BY name")
            .map_err(|e| format!("list_categories: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(EventCategory {
                    id: row.get("id")?,
                    name: row.get("name")?,
                    color: row.get("color")?,
                })
            })
            .map_err(|e| format!("list_categories query: {}", e))?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        Ok(rows)
    })
    .await?;
    Ok(Json(cats))
}

#[derive(Debug, Deserialize)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub color: String,
}

pub async fn create_category(
    State(state): State<AppState>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<Json<EventCategory>, (StatusCode, String)> {
    if req.name.trim().is_empty() || req.color.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Nombre y color requeridos".to_string()));
    }
    let cat = EventCategory {
        id: uuid::Uuid::new_v4().to_string()[..6].to_string(),
        name: req.name.trim().to_string(),
        color: req.color.trim().to_string(),
    };

    let c = cat.clone();
    db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO event_categories (id, name, color) VALUES (?1, ?2, ?3)",
            params![c.id, c.name, c.color],
        ).map_err(|e| format!("create_category: {}", e))?;
        Ok(())
    })
    .await?;

    Ok(Json(cat))
}

pub async fn update_category(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<Json<EventCategory>, (StatusCode, String)> {
    let result = db_op(&state.db, move |conn| {
        let mut cat: EventCategory = conn
            .query_row(
                "SELECT id, name, color FROM event_categories WHERE id = ?1",
                params![id],
                |row| {
                    Ok(EventCategory {
                        id: row.get("id")?,
                        name: row.get("name")?,
                        color: row.get("color")?,
                    })
                },
            )
            .map_err(|_| "Categoria no encontrada".to_string())?;

        if !req.name.trim().is_empty() { cat.name = req.name.trim().to_string(); }
        if !req.color.trim().is_empty() { cat.color = req.color.trim().to_string(); }

        conn.execute(
            "UPDATE event_categories SET name = ?1, color = ?2 WHERE id = ?3",
            params![cat.name, cat.color, cat.id],
        ).map_err(|e| format!("update_category: {}", e))?;

        Ok(cat)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    Ok(Json(result))
}

pub async fn delete_category(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    db_op(&state.db, move |conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM event_categories WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|e| format!("delete_category: {}", e))?;

        if count == 0 {
            return Err("Categoria no encontrada".to_string());
        }

        // Limpiar la categoria de los eventos que la usan
        conn.execute(
            "UPDATE calendar_events SET category = NULL WHERE category = ?1",
            params![id],
        ).map_err(|e| format!("delete_category clear: {}", e))?;

        conn.execute("DELETE FROM event_categories WHERE id = ?1", params![id])
            .map_err(|e| format!("delete_category: {}", e))?;

        Ok(())
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    Ok(StatusCode::NO_CONTENT)
}
