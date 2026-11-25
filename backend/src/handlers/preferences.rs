//! Preferencias de la interfaz guardadas en la cuenta (no en el navegador): la piel
//! sigue a la persona en cualquier equipo y la usa tambien `labnas-tray`.

use axum::{extract::State, http::StatusCode, Extension, Json};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::db_op;
use crate::error::ApiError;
use crate::state::{AppState, SessionInfo};

/// Pieles de la web (frontend/src/themes/themes.ts) mas "auto"
pub const THEMES: &[&str] = &["auto", "dracula", "light", "nord", "solarized", "monokai", "catppuccin", "gruvbox", "tokyonight", "rosepine"];

#[derive(Serialize, Deserialize, Default)]
pub struct Preferences {
    /// None = nunca la eligio (la web usa la del navegador)
    pub theme: Option<String>,
}

/// GET /api/auth/preferences
pub async fn get(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<Preferences>, ApiError> {
    let u = session.username.clone();
    let theme: Option<String> = db_op(&state.db, move |conn| {
        conn.query_row("SELECT ui_theme FROM web_users WHERE username = ?1", params![u], |r| r.get(0))
            .optional()
            .map(Option::flatten)
            .map_err(|e| e.to_string())
    })
    .await?;
    Ok(Json(Preferences { theme }))
}

/// PUT /api/auth/preferences
pub async fn put(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<Preferences>,
) -> Result<Json<Preferences>, ApiError> {
    if req.theme.as_deref().is_some_and(|t| !THEMES.contains(&t)) {
        return Err((StatusCode::BAD_REQUEST, "Piel desconocida".into()));
    }
    let (u, t) = (session.username.clone(), req.theme.clone());
    db_op(&state.db, move |conn| {
        conn.execute("UPDATE web_users SET ui_theme = ?1 WHERE username = ?2", params![t, u]).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;
    Ok(Json(req))
}
