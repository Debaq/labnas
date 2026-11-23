//! Error comun de los handlers: codigo HTTP y mensaje. Axum ya lo convierte en
//! respuesta, asi que sigue siendo una tupla; este modulo solo junta el alias y los
//! atajos que antes se repetian archivo por archivo.

use axum::http::StatusCode;

pub type ApiError = (StatusCode, String);

/// Error 500 con el mensaje del error original: `.map_err(internal)?`
pub fn internal(e: impl std::fmt::Display) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Error 500 de base de datos, con el prefijo "DB: " que ya usaba la API
pub fn db_err(e: impl std::fmt::Display) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e))
}
