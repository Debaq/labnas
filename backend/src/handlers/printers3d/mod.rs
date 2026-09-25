//! Impresoras 3D: OctoPrint, Moonraker, Creality stock y FlashForge.

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::time::Duration;

use rusqlite::params;

use crate::db::{db_op, db_op_status};
use crate::models::printers3d::*;
use crate::state::AppState;


mod creality;
mod flashforge;
mod config;
mod status;
mod jobs;
mod detect;
mod motion;
mod camera;
mod monitor;
mod queue;

// API publica (handlers y loops) y visibilidad entre submodulos (`use super::*`)
#[allow(unused_imports)]
pub use {creality::*, flashforge::*, config::*, status::*, jobs::*, detect::*, motion::*, camera::*, monitor::*, queue::*};

/// Convierte texto del DB al enum Printer3DType
fn parse_printer_type(s: &str) -> Printer3DType {
    match s {
        "OctoPrint" => Printer3DType::OctoPrint,
        "CrealityStock" => Printer3DType::CrealityStock,
        "FlashForge" => Printer3DType::FlashForge,
        _ => Printer3DType::Moonraker,
    }
}

/// Convierte enum Printer3DType a texto para el DB
fn printer_type_str(pt: &Printer3DType) -> &'static str {
    match pt {
        Printer3DType::OctoPrint => "OctoPrint",
        Printer3DType::Moonraker => "Moonraker",
        Printer3DType::CrealityStock => "CrealityStock",
        Printer3DType::FlashForge => "FlashForge",
    }
}

/// Lee una fila de printers3d y construye Printer3DConfig
fn row_to_printer(row: &rusqlite::Row) -> rusqlite::Result<Printer3DConfig> {
    let pt_str: String = row.get(4)?;
    Ok(Printer3DConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        ip: row.get(2)?,
        port: row.get(3)?,
        printer_type: parse_printer_type(&pt_str),
        api_key: row.get(5)?,
        camera_url: row.get(6)?,
        power_watts: row.get(7)?,
        electricity_cost_kwh: row.get(8)?,
        section_id: row.get(9)?,
        order: row.get(10)?,
    })
}

/// Busca una impresora por ID en la DB y devuelve una copia
async fn find_printer(
    state: &AppState,
    id: &str,
) -> Result<Printer3DConfig, (StatusCode, String)> {
    let id = id.to_string();
    db_op_status(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT id, name, ip, port, printer_type, labnas_decrypt(api_key), camera_url, power_watts, electricity_cost_kwh, section_id, \"order\" FROM printers3d WHERE id = ?1",
            params![id],
            row_to_printer,
        )
        .optional()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()))
    })
    .await
}

/// Construye la URL base de la impresora
fn printer_base(printer: &Printer3DConfig) -> String {
    format!("http://{}:{}", printer.ip, printer.port)
}

/// Crea un request builder con la API key de OctoPrint si existe
fn octoprint_request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    printer: &Printer3DConfig,
) -> reqwest::RequestBuilder {
    let mut req = client.request(method, url).timeout(Duration::from_secs(10));
    if let Some(key) = &printer.api_key {
        req = req.header("X-Api-Key", key);
    }
    req
}
