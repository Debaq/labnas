use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::config::save_config;
use crate::models::notifications::UserRole;
use crate::models::tasks::*;
use crate::state::AppState;

// ---- Notificaciones Telegram para tareas ----

async fn notify_task_assigned(state: &AppState, task: &Task) {
    let config = state.config.lock().await;
    let token = match &config.notifications.bot_token {
        Some(t) => t.clone(),
        None => return,
    };
    let chats = config.notifications.telegram_chats.clone();
    drop(config);

    // @all solo aplica a operadores y admins, no observadores
    let target_ids: Vec<i64> = if task.assigned_to.contains(&"all".to_string()) {
        chats
            .iter()
            .filter(|c| c.role == UserRole::Admin || c.role == UserRole::Operador)
            .map(|c| c.chat_id)
            .collect()
    } else {
        chats
            .iter()
            .filter(|c| {
                task.assigned_to.iter().any(|a| {
                    a.to_lowercase() == c.name.to_lowercase()
                        || c.linked_web_user
                            .as_ref()
                            .map(|u| u.to_lowercase() == a.to_lowercase())
                            .unwrap_or(false)
                })
            })
            .map(|c| c.chat_id)
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
    let config = state.config.lock().await;
    let token = match &config.notifications.bot_token {
        Some(t) => t.clone(),
        None => return,
    };
    let chats = config.notifications.telegram_chats.clone();
    drop(config);

    // Notificar al creador
    let creator_id: Option<i64> = chats
        .iter()
        .find(|c| {
            c.name.to_lowercase() == task.created_by.to_lowercase()
                || c.linked_web_user
                    .as_ref()
                    .map(|u| u.to_lowercase() == task.created_by.to_lowercase())
                    .unwrap_or(false)
        })
        .map(|c| c.chat_id);

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

pub async fn list_projects(State(state): State<AppState>) -> Json<Vec<Project>> {
    let config = state.config.lock().await;
    Json(config.tasks.projects.clone())
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
        created_at: Utc::now(),
    };

    let mut config = state.config.lock().await;
    config.tasks.projects.push(project.clone());
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

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

    let mut config = state.config.lock().await;
    let project = config
        .tasks
        .projects
        .iter()
        .find(|p| p.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Proyecto no encontrado".to_string()))?;

    // Solo el creador o admin puede eliminar
    if project.created_by != username && role != UserRole::Admin {
        return Err((StatusCode::FORBIDDEN, "Sin permisos para eliminar este proyecto".to_string()));
    }

    let name = project.name.clone();
    config.tasks.projects.retain(|p| p.id != id);

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity("proyecto_eliminado", &format!("Proyecto: {}", name), &username)
        .await;

    Ok(StatusCode::NO_CONTENT)
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
) -> Json<Vec<Task>> {
    let config = state.config.lock().await;
    let mut tasks = config.tasks.tasks.clone();

    if let Some(project_id) = &query.project {
        tasks.retain(|t| t.project_id.as_deref() == Some(project_id.as_str()));
    }

    if let Some(status_str) = &query.status {
        let target_status = match status_str.as_str() {
            "pendiente" => Some(TaskStatus::Pendiente),
            "enprogreso" => Some(TaskStatus::EnProgreso),
            "completada" => Some(TaskStatus::Completada),
            "rechazada" => Some(TaskStatus::Rechazada),
            _ => None,
        };
        if let Some(status) = target_status {
            tasks.retain(|t| t.status == status);
        }
    }

    Json(tasks)
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
        requires_confirmation: req.requires_confirmation,
        insistent: req.insistent,
        reminder_minutes: req.reminder_minutes,
        confirmed_by: Vec::new(),
        rejected_by: Vec::new(),
        created_at: Utc::now(),
        last_reminder: None,
    };

    let mut config = state.config.lock().await;
    config.tasks.tasks.push(task.clone());
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
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

    let mut config = state.config.lock().await;
    let task = config
        .tasks
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;

    // Solo el creador, asignados o admin pueden actualizar
    let is_assigned = task.assigned_to.contains(&"all".to_string())
        || task.assigned_to.iter().any(|a| a.to_lowercase() == username.to_lowercase());
    if task.created_by != username && !is_assigned && role != UserRole::Admin {
        return Err((StatusCode::FORBIDDEN, "Sin permisos para modificar esta tarea".to_string()));
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
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Estado invalido".to_string(),
                ))
            }
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
    if let Some(requires_confirmation) = req.requires_confirmation {
        task.requires_confirmation = requires_confirmation;
    }
    if let Some(insistent) = req.insistent {
        task.insistent = insistent;
    }
    if let Some(reminder_minutes) = req.reminder_minutes {
        task.reminder_minutes = reminder_minutes;
    }

    let updated = task.clone();
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
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

    let mut config = state.config.lock().await;
    let task = config
        .tasks
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;

    if !task.confirmed_by.contains(&username) {
        task.confirmed_by.push(username.clone());
    }
    // Quitar de rechazados si estaba
    task.rejected_by.retain(|u| u != &username);

    let updated = task.clone();
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity(
            "tarea_confirmada",
            &format!("{} confirmo: {}", username, updated.title),
            &username,
        )
        .await;

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

    let mut config = state.config.lock().await;
    let task = config
        .tasks
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;

    if !task.rejected_by.contains(&username) {
        task.rejected_by.push(username.clone());
    }
    // Quitar de confirmados si estaba
    task.confirmed_by.retain(|u| u != &username);

    let updated = task.clone();
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
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

    let mut config = state.config.lock().await;
    let task = config
        .tasks
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;

    // Solo asignados o creador pueden marcar completada
    let is_assigned = task.assigned_to.contains(&"all".to_string())
        || task.assigned_to.iter().any(|a| a.to_lowercase() == username.to_lowercase());
    if task.created_by != username && !is_assigned {
        return Err((StatusCode::FORBIDDEN, "Sin permisos para completar esta tarea".to_string()));
    }

    task.status = TaskStatus::Completada;
    let updated = task.clone();

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
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

    let mut config = state.config.lock().await;
    let task = config
        .tasks
        .tasks
        .iter()
        .find(|t| t.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;

    // Solo el creador o admin puede eliminar
    if task.created_by != username && role != UserRole::Admin {
        return Err((StatusCode::FORBIDDEN, "Sin permisos para eliminar esta tarea".to_string()));
    }

    let title = task.title.clone();
    config.tasks.tasks.retain(|t| t.id != id);

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity("tarea_eliminada", &format!("Tarea: {}", title), &username)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

// ---- Eventos / Calendario ----

#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    pub title: String,
    pub date: String,
    pub time: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub invitees: Vec<String>,
    #[serde(default = "default_event_rem")]
    pub remind_before_min: u32,
}

fn default_event_rem() -> u32 { 15 }

pub async fn list_events(State(state): State<AppState>) -> Json<Vec<CalendarEvent>> {
    let config = state.config.lock().await;
    Json(config.tasks.events.clone())
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
        created_by: username.clone(),
        invitees: req.invitees,
        accepted: Vec::new(),
        declined: Vec::new(),
        remind_before_min: req.remind_before_min,
        reminded: false,
        created_at: Utc::now(),
    };
    let mut config = state.config.lock().await;
    config.tasks.events.push(event.clone());
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    drop(config);
    state.log_activity("evento", &event.title, &username).await;
    Ok(Json(event))
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

    let mut config = state.config.lock().await;
    let event = config
        .tasks
        .events
        .iter()
        .find(|e| e.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Evento no encontrado".to_string()))?;

    // Solo el creador o admin puede eliminar
    if event.created_by != username && role != UserRole::Admin {
        return Err((StatusCode::FORBIDDEN, "Sin permisos para eliminar este evento".to_string()));
    }

    config.tasks.events.retain(|e| e.id != id);
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
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

    let mut config = state.config.lock().await;
    let event = config.tasks.events.iter_mut().find(|e| e.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Evento no encontrado".to_string()))?;
    event.declined.retain(|u| u != &username);
    if !event.accepted.contains(&username) {
        event.accepted.push(username);
    }
    let result = event.clone();
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
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

    let mut config = state.config.lock().await;
    let event = config.tasks.events.iter_mut().find(|e| e.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Evento no encontrado".to_string()))?;
    event.accepted.retain(|u| u != &username);
    if !event.declined.contains(&username) {
        event.declined.push(username);
    }
    let result = event.clone();
    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}
