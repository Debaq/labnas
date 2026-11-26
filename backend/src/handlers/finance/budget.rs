//! Presupuesto por item, ejecucion en vivo, bienes por proyecto, hitos, caja chica e ingresos (ideas 52, 59, 60, 64, 65). Ver docs/FINANZAS.md.

use axum::Router;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
