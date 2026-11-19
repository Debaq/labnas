//! Impresion CUPS: impresoras, trabajos, estadisticas de costo, duplex manual,
//! impresora de red (IPP) y escaner (SANE).

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
pub(crate) mod ipp_codec;
mod ipp;
mod scan;

// API publica (handlers y loops) y visibilidad entre submodulos (`use super::*`)
#[allow(unused_imports)]
pub use {printers::*, print::*, stats::*, duplex::*, ipp::*, scan::*};

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

// ── Impresoras ocultas (el admin las esconde a todos: lista web, impresion e IPP) ──

const HIDDEN_PRINTERS_KEY: &str = "printing_hidden_printers";

async fn hidden_printers(state: &AppState) -> std::collections::HashSet<String> {
    db_op(&state.db, |conn| {
        Ok(crate::db::get_setting(conn, HIDDEN_PRINTERS_KEY)
            .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
            .unwrap_or_default())
    })
    .await
    .unwrap_or_default()
    .into_iter()
    .collect()
}

async fn is_printer_hidden(state: &AppState, printer: &str) -> bool {
    hidden_printers(state).await.contains(printer)
}

/// Rechaza una impresora oculta salvo para el admin (mismo error que si no existiera)
async fn ensure_visible(state: &AppState, printer: &str, is_admin: bool) -> Result<(), (StatusCode, String)> {
    if !is_admin && is_printer_hidden(state, printer).await {
        return Err((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()));
    }
    Ok(())
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
