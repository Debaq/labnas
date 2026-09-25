//! Agendar una tarea como evento de calendario

use super::*;

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
                row_to_task,
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
