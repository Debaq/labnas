//! Proyectos

use super::*;

// ---- Proyectos ----

pub async fn list_projects(State(state): State<AppState>) -> Result<Json<Vec<Project>>, (StatusCode, String)> {
    let projects = db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, description, created_by, members, member_tags, created_at FROM projects ORDER BY created_at")
            .map_err(|e| format!("list_projects: {}", e))?;
        let rows = stmt
            .query_map([], row_to_project)
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
                row_to_project,
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
