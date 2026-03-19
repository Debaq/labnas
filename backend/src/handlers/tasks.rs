use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::config::save_config;
use crate::models::tasks::*;
use crate::state::AppState;

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
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Project>), (StatusCode, String)> {
    let project = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name.clone(),
        description: req.description,
        created_by: "web".to_string(),
        members: Vec::new(),
        created_at: Utc::now(),
    };

    let mut config = state.config.lock().await;
    config.tasks.projects.push(project.clone());
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    state
        .log_activity("proyecto_creado", &format!("Proyecto: {}", req.name), "web")
        .await;

    Ok((StatusCode::CREATED, Json(project)))
}

pub async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.tasks.projects.len();
    let name = config
        .tasks
        .projects
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .unwrap_or_default();

    config.tasks.projects.retain(|p| p.id != id);
    if config.tasks.projects.len() == before {
        return Err((
            StatusCode::NOT_FOUND,
            "Proyecto no encontrado".to_string(),
        ));
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity("proyecto_eliminado", &format!("Proyecto: {}", name), "web")
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
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Task>), (StatusCode, String)> {
    let task = Task {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: req.project_id,
        title: req.title.clone(),
        description: String::new(),
        assigned_to: req.assigned_to,
        status: TaskStatus::Pendiente,
        created_by: "web".to_string(),
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
        .log_activity("tarea_creada", &format!("Tarea: {}", req.title), "web")
        .await;

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
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let task = config
        .tasks
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;

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
            "web",
        )
        .await;

    Ok(Json(updated))
}

#[derive(Deserialize)]
pub struct ConfirmRejectRequest {
    pub user: String,
}

pub async fn confirm_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ConfirmRejectRequest>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let task = config
        .tasks
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;

    if !task.confirmed_by.contains(&req.user) {
        task.confirmed_by.push(req.user.clone());
    }
    // Quitar de rechazados si estaba
    task.rejected_by.retain(|u| u != &req.user);

    let updated = task.clone();
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity(
            "tarea_confirmada",
            &format!("{} confirmo: {}", req.user, updated.title),
            "web",
        )
        .await;

    Ok(Json(updated))
}

pub async fn reject_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ConfirmRejectRequest>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let task = config
        .tasks
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;

    if !task.rejected_by.contains(&req.user) {
        task.rejected_by.push(req.user.clone());
    }
    // Quitar de confirmados si estaba
    task.confirmed_by.retain(|u| u != &req.user);

    let updated = task.clone();
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity(
            "tarea_rechazada",
            &format!("{} rechazo: {}", req.user, updated.title),
            "web",
        )
        .await;

    Ok(Json(updated))
}

pub async fn done_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let task = config
        .tasks
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()))?;

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
            "web",
        )
        .await;

    Ok(Json(updated))
}

pub async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.tasks.tasks.len();
    let title = config
        .tasks
        .tasks
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.title.clone())
        .unwrap_or_default();

    config.tasks.tasks.retain(|t| t.id != id);
    if config.tasks.tasks.len() == before {
        return Err((StatusCode::NOT_FOUND, "Tarea no encontrada".to_string()));
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity("tarea_eliminada", &format!("Tarea: {}", title), "web")
        .await;

    Ok(StatusCode::NO_CONTENT)
}
