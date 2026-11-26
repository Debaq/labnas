//! Flujo de compra con funciones separadas, cotizaciones, facturas, recepcion conforme, aprobacion con passkey y conflicto de interes (ideas 53-56, 58, 69). Ver docs/FINANZAS.md.

use axum::Router;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
