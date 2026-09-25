//! Asistente de primer arranque: se muestra a los admins hasta que uno lo
//! termina u omite (setting `setup_done`).

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;

use crate::state::{AppState, SessionInfo};

const SETTING: &str = "setup_done";

#[derive(Serialize)]
pub struct SetupStatus {
    pub done: bool,
    /// Ruta de la clave que cifra los secretos (hay que respaldarla aparte)
    pub secret_key_path: String,
    /// Nombre del equipo, sugerido para mDNS
    pub hostname: String,
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|h| h.trim().to_lowercase())
        .unwrap_or_default()
}

/// GET /api/setup (admin)
pub async fn get_setup(State(state): State<AppState>) -> Result<Json<SetupStatus>, (StatusCode, String)> {
    let done = crate::db::db_op(&state.db, |conn| Ok(crate::db::get_setting_bool(conn, SETTING))).await?;
    Ok(Json(SetupStatus {
        done,
        secret_key_path: crate::db::data_dir().join("secret.key").to_string_lossy().to_string(),
        hostname: hostname(),
    }))
}

/// POST /api/setup/done (admin): no volver a mostrar el asistente
pub async fn finish_setup(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op(&state.db, |conn| crate::db::set_setting(conn, SETTING, "true")).await?;
    state.log_activity("Asistente inicial", "terminado", &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}
