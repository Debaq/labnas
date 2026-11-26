//! Piezas comunes del modulo Finanzas (etapa 1 de la hoja de ruta, ver docs/FINANZAS.md):
//! categorias de presupuesto, el registro de lo comprometido y gastado por cada flujo
//! (compras, viajes, caja chica, honorarios), los archivos de respaldo con su huella y el
//! paso a la bitacora inalterable.
//!
//! Regla de oro: lo financiero queda en la bitacora (`crate::ledger`), que no se edita ni se
//! borra. Un error se corrige con un registro nuevo.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::db_op;
use crate::error::{internal, ApiError};
use crate::state::AppState;

/// Categorias de un item de presupuesto
pub const CATEGORIES: &[&str] = &["personal", "operacion", "equipos", "viajes", "otro"];

/// Tamaño maximo de un respaldo (factura, cotizacion, acta, foto)
pub const MAX_FILE: usize = 15 * 1024 * 1024;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Titulo del proyecto del Portafolio (None si no existe)
pub fn project_title(conn: &Connection, project_id: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT title FROM portfolio_entries WHERE id = ?1", [project_id], |r| r.get(0)).optional()
}

/// Lo que un flujo (compra, viaje, caja chica, honorario...) tiene comprometido y gastado
/// contra un item de presupuesto. Cada flujo lo actualiza cuando cambia de estado:
/// comprometido al aprobarse, gastado al pagarse, ambos en 0 si se rechaza o anula. La
/// ejecucion presupuestaria suma esta tabla.
#[allow(clippy::too_many_arguments)]
pub fn set_commitment(
    conn: &Connection,
    source: &str,
    source_id: &str,
    project_id: &str,
    budget_item_id: Option<&str>,
    label: &str,
    committed: f64,
    spent: f64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO fin_commitments (source, source_id, project_id, budget_item_id, label, committed, spent, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(source, source_id) DO UPDATE SET project_id = ?3, budget_item_id = ?4, label = ?5,
             committed = ?6, spent = ?7, updated_at = ?8",
        params![source, source_id, project_id, budget_item_id, label, committed, spent, now()],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct FileRef {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub sha256: String,
}

/// Decodifica un archivo subido como data URL o base64 y reconoce su tipo por el contenido
pub fn decode_upload(data: &str) -> Result<(Vec<u8>, &'static str), ApiError> {
    let bad = |m: &str| (StatusCode::BAD_REQUEST, m.to_string());
    let b64 = data.split_once("base64,").map(|(_, b)| b).unwrap_or(data).trim();
    if b64.len() > MAX_FILE * 4 / 3 + 8 {
        return Err(bad("El archivo pesa mas de 15 MB"));
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|_| bad("Archivo invalido"))?;
    if bytes.is_empty() || bytes.len() > MAX_FILE {
        return Err(bad("El archivo esta vacio o pesa mas de 15 MB"));
    }
    let mime = sniff(&bytes).ok_or_else(|| bad("Tipo de archivo no admitido: PDF, XML, JPEG, PNG, XLSX o DOCX"))?;
    Ok((bytes, mime))
}

fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"%PDF") {
        Some("application/pdf")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if b.starts_with(b"PK\x03\x04") {
        // xlsx/docx son zip: se distinguen por su contenido
        let head = &b[..b.len().min(4096)];
        let has = |n: &[u8]| head.windows(n.len()).any(|w| w == n);
        if has(b"xl/") {
            Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
        } else if has(b"word/") {
            Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        } else {
            None
        }
    } else {
        let start = b.iter().position(|c| !c.is_ascii_whitespace() && *c != 0xEF && *c != 0xBB && *c != 0xBF)?;
        b[start..].starts_with(b"<?xml").then_some("application/xml")
    }
}

/// Guarda un respaldo con su huella SHA-256 (la huella va despues a la bitacora)
pub fn store_file(conn: &Connection, name: &str, bytes: &[u8], mime: &str, user: &str) -> rusqlite::Result<FileRef> {
    let id = new_id();
    let sha256 = crate::ledger::sha256_hex(bytes);
    let name: String = name.trim().chars().filter(|c| !c.is_control() && *c != '/' && *c != '\\').take(120).collect();
    let name = if name.is_empty() { "respaldo".to_string() } else { name };
    conn.execute(
        "INSERT INTO fin_files (id, name, mime, size, sha256, data, uploaded_by, uploaded_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, name, mime, bytes.len() as i64, sha256, bytes, user, now()],
    )?;
    Ok(FileRef { id, name, mime: mime.to_string(), size: bytes.len() as i64, sha256 })
}

pub fn file_ref(conn: &Connection, id: &str) -> rusqlite::Result<Option<FileRef>> {
    conn.query_row("SELECT id, name, mime, size, sha256 FROM fin_files WHERE id = ?1", [id], |r| {
        Ok(FileRef { id: r.get(0)?, name: r.get(1)?, mime: r.get(2)?, size: r.get(3)?, sha256: r.get(4)? })
    })
    .optional()
}

/// GET /api/finance/files/{id} — descarga un respaldo (nivel Ver en Finanzas)
pub async fn download_file(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let found: Option<(String, String, Vec<u8>)> = db_op(&state.db, move |conn| {
        conn.query_row("SELECT name, mime, data FROM fin_files WHERE id = ?1", [&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()
            .map_err(|e| e.to_string())
    })
    .await?;
    let (name, mime, data) = found.ok_or((StatusCode::NOT_FOUND, "Archivo no encontrado".to_string()))?;
    let inline = mime.starts_with("image/") || mime == "application/pdf";
    let disposition = format!(
        "{}; filename=\"{}\"",
        if inline { "inline" } else { "attachment" },
        name.replace('"', "")
    );
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(data))
        .map_err(internal)
}

/// Deja constancia en la bitacora inalterable. Corre su propia transaccion: llamarla fuera de
/// cualquier otra (despues de guardar el cambio). Devuelve el numero de registro.
pub async fn ledger(state: &AppState, user: &str, kind: &str, subject: &str, data: serde_json::Value) -> Result<i64, ApiError> {
    let (user, kind, subject) = (user.to_string(), kind.to_string(), subject.to_string());
    db_op(&state.db, move |conn| crate::ledger::append(conn, &user, &kind, &subject, None, &data).map(|e| e.seq)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconoce_tipos() {
        assert_eq!(sniff(b"%PDF-1.7"), Some("application/pdf"));
        assert_eq!(sniff(b"\xEF\xBB\xBF<?xml version=\"1.0\"?>"), Some("application/xml"));
        assert_eq!(sniff(b"  <?xml"), Some("application/xml"));
        assert_eq!(sniff(b"MZ\x90"), None);
    }
}
