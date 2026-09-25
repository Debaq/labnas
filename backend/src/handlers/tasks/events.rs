//! Eventos de calendario

use super::*;

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

pub(super) fn default_notify_tg() -> bool { true }

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

pub(super) fn default_event_rem() -> u32 { 15 }

pub async fn list_events(State(state): State<AppState>) -> Result<Json<Vec<CalendarEvent>>, (StatusCode, String)> {
    let events = db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare("SELECT id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category FROM calendar_events ORDER BY date, time")
            .map_err(|e| format!("list_events: {}", e))?;
        let rows = stmt
            .query_map([], row_to_event)
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
                row_to_event,
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
                row_to_event,
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
                row_to_event,
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
