//! Modulo Finanzas (etapa 1, ver docs/FINANZAS.md). Cada parte tiene su archivo y sus rutas;
//! las piezas comunes (categorias, comprometido/gastado, respaldos, bitacora) estan en
//! `crate::finance`.

use axum::{routing::get, Router};

use crate::state::AppState;

pub mod budget;
pub mod purchases;
pub mod rendicion;
pub mod travel;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/finance/files/{id}", get(crate::finance::download_file))
        .merge(budget::routes())
        .merge(purchases::routes())
        .merge(rendicion::routes())
        .merge(travel::routes())
}
