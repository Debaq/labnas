//! Viajes y viaticos (idea 63). Ver docs/FINANZAS.md.

use axum::Router;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
