//! Impresion CUPS: impresoras, trabajos, estadisticas de costo y duplex manual.

use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

use rusqlite::params;

use crate::db::db_op;
use crate::models::printing::{
    AllUserCostsResponse, CupsPrintJob, CupsPrinter, DuplexPrepareResponse,
    DuplexPrintStepRequest, PrintFileRequest, PrinterCosts, PrinterOption, PrinterStats,
    PrinterStatsResponse, UserCostsResponse, UserPrinterStats,
};
use crate::state::AppState;


mod printers;
mod print;
mod stats;
mod duplex;

// API publica (handlers y loops) y visibilidad entre submodulos (`use super::*`)
#[allow(unused_imports)]
pub use {printers::*, print::*, stats::*, duplex::*};

/// Extrae el username de la sesión a partir del header Authorization
async fn extract_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<(String, crate::models::notifications::UserRole)> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())?;
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&token)?;
    Some((session.username.clone(), session.role.clone()))
}

// Formatos que CUPS imprime bien nativamente (sin conversion)
const PRINTABLE_EXTENSIONS: &[&str] = &[
    "pdf", "ps", "eps", "txt", "text", "log", "conf", "cfg", "sh", "py", "rs", "js", "ts",
    "json", "xml", "csv", "md", "c", "cpp", "h", "java", "rb", "pl", "png", "jpg", "jpeg",
    "gif", "tiff", "tif", "bmp", "svg",
];

fn is_printable_file(filename: &str) -> bool {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    PRINTABLE_EXTENSIONS.contains(&ext.as_str())
}

fn validate_printer_name(name: &str) -> Result<(), (StatusCode, String)> {
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Nombre de impresora invalido".to_string(),
        ));
    }
    Ok(())
}
