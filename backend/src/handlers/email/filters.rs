//! Filtros por remitente

use super::*;

// =====================
// Email filters
// =====================

pub async fn list_filters(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<EmailFilter>>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    let filters = db_op(&state.db, move |conn| {
        let filters_str: Option<String> = conn.query_row(
            "SELECT filters FROM email_accounts WHERE username = ?1",
            params![&uname],
            |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?.flatten();
        let filters: Vec<EmailFilter> = filters_str
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Ok(filters)
    }).await?;
    Ok(Json(filters))
}

#[derive(Debug, Deserialize)]
pub struct AddFilterRequest {
    pub pattern: String,
    pub action: FilterAction,
    pub label: String,
    #[serde(default)]
    pub auto_tag: Option<String>,
}

pub async fn add_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AddFilterRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let uname = username.clone();
    db_op_status(&state.db, move |conn| {
        let row_opt: Option<String> = conn.query_row(
            "SELECT filters FROM email_accounts WHERE username = ?1",
            params![&uname],
            |row| row.get(0),
        ).optional().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let filters_str = row_opt
            .ok_or((StatusCode::NOT_FOUND, "Cuenta de correo no configurada".to_string()))?;
        let mut filters: Vec<EmailFilter> = serde_json::from_str(&filters_str).unwrap_or_default();

        // No duplicar
        if filters.iter().any(|f| f.pattern == req.pattern) {
            return Err((StatusCode::CONFLICT, "Filtro ya existe".to_string()));
        }

        filters.push(EmailFilter {
            pattern: req.pattern,
            action: req.action,
            label: req.label,
            auto_tag: req.auto_tag,
        });

        let new_json = serde_json::to_string(&filters).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE email_accounts SET filters = ?1 WHERE username = ?2",
            params![new_json, &uname],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(())
    }).await?;
    Ok(StatusCode::CREATED)
}

pub async fn delete_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(pattern): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let decoded = urlencoding::decode(&pattern).unwrap_or_default().to_string();
    let uname = username.clone();
    db_op_status(&state.db, move |conn| {
        let row_opt: Option<String> = conn.query_row(
            "SELECT filters FROM email_accounts WHERE username = ?1",
            params![&uname],
            |row| row.get(0),
        ).optional().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let filters_str = row_opt
            .ok_or((StatusCode::NOT_FOUND, "Cuenta no configurada".to_string()))?;
        let mut filters: Vec<EmailFilter> = serde_json::from_str(&filters_str).unwrap_or_default();

        let before = filters.len();
        filters.retain(|f| f.pattern != decoded);
        if filters.len() == before {
            return Err((StatusCode::NOT_FOUND, "Filtro no encontrado".to_string()));
        }

        let new_json = serde_json::to_string(&filters).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE email_accounts SET filters = ?1 WHERE username = ?2",
            params![new_json, &uname],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(())
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Aplica filtros del usuario a un email. Devuelve (label, action) o None si no matchea.
pub(super) fn apply_filters(filters: &[EmailFilter], from: &str) -> Option<(String, FilterAction, Option<String>)> {
    let from_lower = from.to_lowercase();
    for f in filters {
        if from_lower.contains(&f.pattern.to_lowercase()) {
            return Some((f.label.clone(), f.action.clone(), f.auto_tag.clone()));
        }
    }
    None
}
