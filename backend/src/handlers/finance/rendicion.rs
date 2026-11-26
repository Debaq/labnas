//! Rendiciones, expediente exportable, manual de procedimientos, horas por proyecto y detector de anomalias (ideas 61, 62, 66, 67, 68). Ver docs/FINANZAS.md.

use axum::Router;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
