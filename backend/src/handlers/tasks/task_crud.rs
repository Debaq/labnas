//! Tareas: crear, editar, confirmar, rechazar, completar

use super::*;

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
            .query_map(params_refs.as_slice(), row_to_task)
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

pub(super) fn default_reminder() -> u32 {
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
                row_to_task,
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
                row_to_task,
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
                row_to_task,
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
                row_to_task,
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
