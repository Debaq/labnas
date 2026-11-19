//! Bitacora inalterable: consulta, verificacion, exportacion, notas con huella de
//! archivos, anulaciones y ancla externa de la huella final. Solo admin por ahora
//! (el middleware niega por defecto; los permisos por modulo y el rol auditor vendran
//! en la etapa 0 de la hoja de ruta). La logica de la cadena esta en `crate::ledger`.

use axum::{
    extract::{Path as AxPath, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Read;
use std::path::Path;

use crate::ledger::{self, Entry, VerifyReport, KIND_ANNUL, KIND_NOTE};
use crate::state::{AppState, SessionInfo};
use crate::storage::{Op, Storage};

type ApiError = (StatusCode, String);

const MAX_PAGE: u32 = 500;
const MAX_TITLE: usize = 200;
const MAX_TEXT: usize = 20_000;
const MAX_SUBJECT: usize = 200;
const MAX_FILES: usize = 20;

fn bad(msg: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

#[derive(Debug, Serialize)]
pub struct EntryView {
    #[serde(flatten)]
    pub entry: Entry,
    /// Registro que anula a este
    pub annulled_by: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub kind: Option<String>,
    pub before_seq: Option<i64>,
    pub limit: Option<u32>,
}

/// GET /api/ledger — mas recientes primero
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<EntryView>>, ApiError> {
    let out = crate::db::db_op(&state.db, move |conn| {
        let cols = ledger::COLS.split(", ").map(|c| format!("e.{}", c)).collect::<Vec<_>>().join(", ");
        let mut sql = format!(
            "SELECT {}, (SELECT a.seq FROM ledger_entries a WHERE a.annuls = e.seq) FROM ledger_entries e WHERE 1=1",
            cols
        );
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(q) = query.q.filter(|q| !q.trim().is_empty()) {
            sql.push_str(" AND (e.data LIKE ? OR e.subject LIKE ? OR e.username LIKE ?)");
            let pattern = format!("%{}%", q.trim());
            for _ in 0..3 {
                args.push(pattern.clone().into());
            }
        }
        if let Some(kind) = query.kind.filter(|k| !k.trim().is_empty()) {
            sql.push_str(" AND e.kind = ?");
            args.push(kind.trim().to_string().into());
        }
        if let Some(before) = query.before_seq {
            sql.push_str(" AND e.seq < ?");
            args.push(before.into());
        }
        let limit = query.limit.unwrap_or(100).clamp(1, MAX_PAGE);
        sql.push_str(&format!(" ORDER BY e.seq DESC LIMIT {}", limit));

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args), |r| {
                Ok(EntryView { entry: ledger::row_to_entry(r)?, annulled_by: r.get(9)? })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
    .await?;
    Ok(Json(out))
}

#[derive(Debug, Serialize)]
pub struct Anchor {
    pub seq: i64,
    pub hash: String,
    pub at: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    #[serde(flatten)]
    pub report: VerifyReport,
    pub last_anchor: Option<Anchor>,
}

fn load_anchor(conn: &rusqlite::Connection) -> Option<Anchor> {
    Some(Anchor {
        seq: crate::db::get_setting(conn, "ledger_anchor_seq")?.parse().ok()?,
        hash: crate::db::get_setting(conn, "ledger_anchor_hash")?,
        at: crate::db::get_setting(conn, "ledger_anchor_at")?,
    })
}

/// GET /api/ledger/verify — recalcula la cadena completa
pub async fn verify(State(state): State<AppState>) -> Result<Json<VerifyResponse>, ApiError> {
    let out = crate::db::db_op(&state.db, |conn| {
        Ok(VerifyResponse { report: ledger::verify(conn)?, last_anchor: load_anchor(conn) })
    })
    .await?;
    Ok(Json(out))
}

/// GET /api/ledger/export — un registro JSON por linea, del primero al ultimo
pub async fn export(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<impl IntoResponse, ApiError> {
    let body = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare(&format!("SELECT {} FROM ledger_entries ORDER BY seq ASC", ledger::COLS))
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], ledger::row_to_entry).map_err(|e| e.to_string())?;
        let mut out = String::new();
        for e in rows {
            let e = e.map_err(|e| e.to_string())?;
            out.push_str(&serde_json::to_string(&e).map_err(|e| e.to_string())?);
            out.push('\n');
        }
        Ok(out)
    })
    .await?;
    state.log_activity("Bitacora exportada", "", &session.username).await;

    let name = format!("bitacora-labnas-{}.jsonl", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-ndjson; charset=utf-8"));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", name)) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok((headers, body))
}

/// Huella SHA-256 y tamano de un archivo, leyendo por partes
fn hash_file(path: &Path) -> std::io::Result<(String, u64)> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect(), size))
}

async fn hash_file_async(path: std::path::PathBuf) -> Result<(String, u64), ApiError> {
    tokio::task::spawn_blocking(move || hash_file(&path))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo leer el archivo: {}", e)))
}

#[derive(Debug, Deserialize)]
pub struct NoteInput {
    pub title: String,
    #[serde(default)]
    pub text: String,
    /// Referencia libre: proyecto, compra, acta...
    #[serde(default)]
    pub subject: String,
    /// Rutas de archivos del NAS cuya huella queda registrada (facturas, actas, fotos)
    #[serde(default)]
    pub files: Vec<String>,
}

/// POST /api/ledger/notes — anotacion manual, opcionalmente con huella de archivos
pub async fn add_note(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(input): Json<NoteInput>,
) -> Result<(StatusCode, Json<Entry>), ApiError> {
    let title = input.title.trim().to_string();
    let subject = input.subject.trim().to_string();
    if title.is_empty() {
        return Err(bad("Falta el titulo"));
    }
    if title.chars().count() > MAX_TITLE || subject.chars().count() > MAX_SUBJECT {
        return Err(bad("Titulo o asunto demasiado largo"));
    }
    if input.text.chars().count() > MAX_TEXT {
        return Err(bad("Texto demasiado largo"));
    }
    if input.files.len() > MAX_FILES {
        return Err(bad("Demasiados archivos (maximo 20)"));
    }

    let st = Storage::load(&state.db).await?;
    let mut files = Vec::new();
    for raw in input.files.iter().map(|f| f.trim()).filter(|f| !f.is_empty()) {
        let path = st.resolve_existing_for(&session, raw, Op::Read)?;
        if !path.is_file() {
            return Err(bad(&format!("No es un archivo: {}", raw)));
        }
        let (sha256, size) = hash_file_async(path.clone()).await?;
        files.push(json!({ "path": path.to_string_lossy(), "sha256": sha256, "size": size }));
    }

    let data = json!({ "title": title, "text": input.text, "files": files });
    let user = session.username.clone();
    let entry = crate::db::db_op(&state.db, move |conn| ledger::append(conn, &user, KIND_NOTE, &subject, None, &data))
        .await?;
    state
        .log_activity("Bitacora: nota", &format!("#{} {}", entry.seq, title), &session.username)
        .await;
    Ok((StatusCode::CREATED, Json(entry)))
}

#[derive(Debug, Deserialize)]
pub struct AnnulInput {
    pub reason: String,
}

/// POST /api/ledger/{seq}/annul — nada se borra: se agrega un registro que lo anula
pub async fn annul(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    AxPath(seq): AxPath<i64>,
    Json(input): Json<AnnulInput>,
) -> Result<(StatusCode, Json<Entry>), ApiError> {
    let reason = input.reason.trim().to_string();
    if reason.is_empty() {
        return Err(bad("Indica el motivo de la anulacion"));
    }
    if reason.chars().count() > MAX_TEXT {
        return Err(bad("Motivo demasiado largo"));
    }
    let user = session.username.clone();
    let entry = crate::db::db_op_status(&state.db, move |conn| {
        let err = |e: String| (StatusCode::INTERNAL_SERVER_ERROR, e);
        let target = ledger::get(conn, seq)
            .map_err(err)?
            .ok_or((StatusCode::NOT_FOUND, "No existe ese registro".to_string()))?;
        if target.kind == KIND_ANNUL {
            return Err(bad("Una anulacion no se anula: registra una nota que lo explique"));
        }
        if let Some(by) = ledger::annulled_by(conn, seq).map_err(err)? {
            return Err((StatusCode::CONFLICT, format!("Ya fue anulado por el registro #{}", by)));
        }
        // El indice unico de `annuls` cubre la carrera entre dos anulaciones simultaneas
        ledger::append(conn, &user, KIND_ANNUL, &target.subject, Some(seq), &json!({ "reason": reason }))
            .map_err(|e| {
                if e.contains("UNIQUE") {
                    (StatusCode::CONFLICT, "Ya fue anulado".to_string())
                } else {
                    (StatusCode::INTERNAL_SERVER_ERROR, e)
                }
            })
    })
    .await?;
    state
        .log_activity("Bitacora: anulacion", &format!("#{} anula #{}", entry.seq, seq), &session.username)
        .await;
    Ok((StatusCode::CREATED, Json(entry)))
}

#[derive(Debug, Serialize)]
pub struct FileCheck {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    /// "igual" | "cambiado" | "no_existe" | "sin_acceso"
    pub status: &'static str,
    pub current_sha256: Option<String>,
}

/// GET /api/ledger/{seq}/files — compara los archivos registrados con los actuales
pub async fn check_files(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    AxPath(seq): AxPath<i64>,
) -> Result<Json<Vec<FileCheck>>, ApiError> {
    let entry = crate::db::db_op(&state.db, move |conn| ledger::get(conn, seq))
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No existe ese registro".to_string()))?;
    let data: serde_json::Value = serde_json::from_str(&entry.data).unwrap_or_default();
    let st = Storage::load(&state.db).await?;
    let mut out = Vec::new();
    for f in data["files"].as_array().into_iter().flatten() {
        let path = f["path"].as_str().unwrap_or_default().to_string();
        let sha256 = f["sha256"].as_str().unwrap_or_default().to_string();
        let size = f["size"].as_u64().unwrap_or_default();
        let (status, current_sha256) = match st.resolve_for(&session, &path, Op::Read) {
            Err(_) => ("sin_acceso", None),
            Ok(p) if !p.is_file() => ("no_existe", None),
            Ok(p) => match hash_file_async(p).await {
                Ok((h, _)) if h == sha256 => ("igual", Some(h)),
                Ok((h, _)) => ("cambiado", Some(h)),
                Err(_) => ("sin_acceso", None),
            },
        };
        out.push(FileCheck { path, sha256, size, status, current_sha256 });
    }
    Ok(Json(out))
}

fn anchor_text(report: &VerifyReport, lab: &str) -> String {
    format!(
        "Bitacora de {}: {} registros. Huella final (registro #{}):\n{}\n{}",
        lab,
        report.count,
        report.head_seq.unwrap_or(0),
        report.head_hash,
        chrono::Utc::now().format("%Y-%m-%d %H:%M UTC"),
    )
}

#[derive(Debug, Serialize)]
pub struct AnchorResponse {
    /// Texto para guardar fuera del servidor (correo, acta, chat)
    pub text: String,
    pub report: VerifyReport,
}

/// Verifica y, si cuadra, envia la huella final a los admins por Telegram y la recuerda
async fn send_anchor(state: &AppState, by: &str) -> Result<AnchorResponse, ApiError> {
    let (report, lab) = crate::db::db_op(&state.db, |conn| {
        let lab: String = conn
            .query_row("SELECT lab_name FROM branding WHERE id = 1", [], |r| r.get(0))
            .ok()
            .filter(|s: &String| !s.trim().is_empty())
            .unwrap_or_else(|| "LabNAS".into());
        Ok((ledger::verify(conn)?, lab))
    })
    .await?;
    if !report.ok {
        return Err((StatusCode::CONFLICT, "La cadena no cuadra: no se ancla una bitacora alterada".to_string()));
    }
    let text = anchor_text(&report, &lab);
    let tg = format!(
        "🔗 *Bitácora* ({} registros)\nHuella final del registro #{}:\n`{}`",
        report.count,
        report.head_seq.unwrap_or(0),
        report.head_hash
    );
    crate::handlers::notifications::notify_admins(state, &tg).await;
    let (seq, hash) = (report.head_seq.unwrap_or(0), report.head_hash.clone());
    crate::db::db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, "ledger_anchor_seq", &seq.to_string())?;
        crate::db::set_setting(conn, "ledger_anchor_hash", &hash)?;
        crate::db::set_setting(conn, "ledger_anchor_at", &chrono::Utc::now().to_rfc3339())
    })
    .await?;
    state
        .log_activity("Bitacora anclada", &format!("#{} {}", seq, report.head_hash), by)
        .await;
    Ok(AnchorResponse { text, report })
}

/// POST /api/ledger/anchor
pub async fn anchor(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<AnchorResponse>, ApiError> {
    Ok(Json(send_anchor(&state, &session.username).await?))
}

/// Revision diaria: si la cadena se rompio, alarma; si crecio desde el ultimo ancla, ancla.
/// Tambien detecta que la cadena se haya cortado por el final (ya no contiene el ancla).
pub async fn ledger_watch_loop(state: AppState) {
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    loop {
        check_once(&state).await;
        tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
    }
}

pub(crate) async fn check_once(state: &AppState) {
    let res = crate::db::db_op(&state.db, |conn| {
        let report = ledger::verify(conn)?;
        let anchor = load_anchor(conn);
        // El registro anclado debe seguir ahi con la misma huella
        let anchor_ok = match &anchor {
            None => true,
            Some(a) => ledger::get(conn, a.seq)?.map(|e| e.hash == a.hash).unwrap_or(false),
        };
        Ok((report, anchor, anchor_ok))
    })
    .await;
    let Ok((report, anchor, anchor_ok)) = res else { return };

    let problem = if !report.ok {
        Some(format!(
            "la cadena no cuadra desde el registro #{} ({})",
            report.broken_at.unwrap_or(0),
            report.reason.clone().unwrap_or_default()
        ))
    } else if !anchor_ok {
        let a = anchor.as_ref().map(|a| a.seq).unwrap_or(0);
        Some(format!("el registro anclado #{} ya no esta o cambio (se borraron o reescribieron registros)", a))
    } else {
        None
    };

    if let Some(p) = problem {
        eprintln!("[Bitacora] ALERTA: {}", p);
        state.log_activity("Bitacora ALTERADA", &p, "sistema").await;
        crate::events::notify(state, crate::events::Audience::Admins, None, crate::events::Level::Error, "Bitácora alterada", &p);
        crate::handlers::notifications::notify_admins(state, &format!("⚠️ Bitácora alterada: {}", p)).await;
        return;
    }
    let grew = report.head_seq.unwrap_or(0) > anchor.map(|a| a.seq).unwrap_or(0);
    if grew {
        if let Err((_, e)) = send_anchor(state, "sistema").await {
            eprintln!("[Bitacora] No se pudo anclar: {}", e);
        }
    }
}
