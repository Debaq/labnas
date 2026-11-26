//! Modulo Transparencia: portal publico generado desde los datos reales y solicitudes de
//! informacion con plazo (ideas 70 y 71). Viene apagado. Ver docs/TRANSPARENCIA.md.

use axum::Router;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
