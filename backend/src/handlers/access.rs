//! Niveles de acceso de un registro o archivo (ver `docs/ACCESO.md`):
//!
//! - **Privado** (por defecto): lo ve el equipo segun su rol, en su modulo de siempre.
//! - **Con autorizacion**: ademas lo ven las personas autorizadas, cada una hasta una fecha,
//!   aunque su rol no tenga el modulo (un auditor, alguien de Finanzas, el financiador).
//! - **Publico**: cualquiera con el enlace, sin cuenta.
//!
//! Lo que se muestra fuera del modulo (a autorizados y al publico) es una ficha de solo
//! lectura que arma cada tipo de registro (`load_record`), sin datos internos como contactos
//! o notas privadas. Cada cambio queda en la auditoria y cada vista en el registro de accesos.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::{internal, ApiError};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Privado,
    Autorizado,
    Publico,
}

impl Visibility {
    fn as_str(self) -> &'static str {
        match self {
            Visibility::Privado => "privado",
            Visibility::Autorizado => "autorizado",
            Visibility::Publico => "publico",
        }
    }
    fn parse(s: &str) -> Visibility {
        match s {
            "autorizado" => Visibility::Autorizado,
            "publico" => Visibility::Publico,
            _ => Visibility::Privado,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Visibility::Privado => "privado",
            Visibility::Autorizado => "con autorizacion",
            Visibility::Publico => "publico",
        }
    }
}

/// Tipo de registro: (id, modulo, nombre)
pub const KINDS: &[(&str, &str, &str)] = &[
    ("proyecto", "portfolio", "Proyecto"),
    ("bien", "inventory", "Bien del inventario"),
    ("nota", "notes", "Nota"),
    ("plazo", "deadlines", "Plazo"),
    ("archivo", "files", "Archivo"),
];

fn kind_module(kind: &str) -> Option<&'static str> {
    KINDS.iter().find(|(k, _, _)| *k == kind).map(|(_, m, _)| *m)
}

fn kind_label(kind: &str) -> &'static str {
    KINDS.iter().find(|(k, _, _)| *k == kind).map(|(_, _, l)| *l).unwrap_or("Registro")
}

#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub label: String,
    pub value: String,
}

/// Ficha de solo lectura que ven autorizados y publico
#[derive(Debug, Clone, Serialize)]
pub struct RecordView {
    pub kind: String,
    pub kind_label: &'static str,
    pub title: String,
    pub fields: Vec<Field>,
    /// Texto largo (descripcion, contenido de la nota)
    pub text: String,
    /// Archivo: se puede descargar
    pub download: bool,
    /// Quien lo creo (decide su nivel junto a quien gestiona el modulo)
    #[serde(skip)]
    pub owner: Option<String>,
}

fn field(fields: &mut Vec<Field>, label: &str, value: impl Into<String>) {
    let value = value.into();
    if !value.trim().is_empty() {
        fields.push(Field { label: label.to_string(), value });
    }
}

fn money(v: f64) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    format!("{}${}", if n < 0 { "-" } else { "" }, out)
}

/// Arma la ficha de un registro (None = no existe)
pub fn load_record(conn: &Connection, kind: &str, id: &str) -> Option<RecordView> {
    let mut fields = Vec::new();
    let view = |title: String, fields: Vec<Field>, text: String, owner: Option<String>| RecordView {
        kind: kind.to_string(),
        kind_label: kind_label(kind),
        title,
        fields,
        text,
        download: false,
        owner,
    };
    match kind {
        "proyecto" => {
            type Row = (String, String, String, String, String, Option<f64>, String, String, Option<String>, String, String);
            let r: Row = conn
                .query_row(
                    "SELECT title, entry_type, institution, funding_source, principal_investigator, budget,
                            start_date, status, end_date, description, milestones
                     FROM portfolio_entries WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?)),
                )
                .optional()
                .ok()??;
            let (title, entry_type, institution, funding, pi, budget, start, status, end, description, milestones) = r;
            let tipo = match entry_type.as_str() {
                "course" => "Curso",
                "diploma" => "Diplomado",
                "workshop" => "Taller",
                _ => "Proyecto",
            };
            let estado = match status.as_str() {
                "active" => "En curso",
                "completed" => "Terminado",
                "cancelled" => "Cancelado",
                "submitted" => "Postulado",
                _ => "Planificado",
            };
            field(&mut fields, "Tipo", tipo);
            field(&mut fields, "Estado", estado);
            field(&mut fields, "Institucion", institution);
            field(&mut fields, "Financiamiento", funding);
            field(&mut fields, "Responsable", pi);
            if let Some(b) = budget {
                field(&mut fields, "Monto", money(b));
            }
            field(&mut fields, "Inicio", start);
            field(&mut fields, "Termino", end.unwrap_or_default());
            let hitos: Vec<crate::models::portfolio::Milestone> = serde_json::from_str(&milestones).unwrap_or_default();
            if !hitos.is_empty() {
                let list: Vec<String> = hitos
                    .iter()
                    .map(|h| format!("{}{}{}", if h.completed { "✓ " } else { "" }, h.title, if h.date.is_empty() { String::new() } else { format!(" ({})", h.date) }))
                    .collect();
                field(&mut fields, "Hitos", list.join("\n"));
            }
            Some(view(title, fields, description, None))
        }
        "bien" => {
            type Row = (String, String, f64, String, f64, String, String, String, String, String, String);
            let r: Row = conn
                .query_row(
                    "SELECT i.name, COALESCE(c.name, ''), i.quantity, i.unit, i.cost, i.supplier, i.location,
                            i.serial_number, i.purchase_date, i.status, i.description
                     FROM inventory_items i LEFT JOIN inventory_categories c ON c.id = i.category_id
                     WHERE i.id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?)),
                )
                .optional()
                .ok()??;
            let (name, category, qty, unit, cost, supplier, location, serial, purchase, status, description) = r;
            field(&mut fields, "Categoria", category);
            field(&mut fields, "Cantidad", format!("{} {}", qty, unit).trim().to_string());
            if cost > 0.0 {
                field(&mut fields, "Costo", money(cost));
            }
            field(&mut fields, "Proveedor", supplier);
            field(&mut fields, "Ubicacion", location);
            field(&mut fields, "N° de serie", serial);
            field(&mut fields, "Fecha de compra", purchase);
            field(&mut fields, "Estado", status);
            Some(view(name, fields, description, None))
        }
        "nota" => {
            let (title, content, owner, updated): (String, String, String, String) = conn
                .query_row("SELECT title, content, created_by, updated_at FROM notes WHERE id = ?1", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .optional()
                .ok()??;
            field(&mut fields, "Autor", owner.clone());
            field(&mut fields, "Actualizada", updated.get(..10).unwrap_or(&updated).to_string());
            Some(view(title, fields, content, Some(owner)))
        }
        "plazo" => {
            let (title, category, due, notes, done, owner): (String, String, String, String, Option<String>, String) = conn
                .query_row(
                    "SELECT title, category, due_date, notes, done_at, created_by FROM deadlines WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
                )
                .optional()
                .ok()??;
            field(&mut fields, "Categoria", category);
            field(&mut fields, "Fecha", due);
            field(&mut fields, "Estado", if done.is_some() { "Cumplido" } else { "Pendiente" });
            Some(view(title, fields, notes, Some(owner)))
        }
        "archivo" => {
            let roots = crate::storage::canonical_roots(conn);
            let path = crate::storage::resolve_existing(&roots, id).ok()?;
            let meta = std::fs::metadata(&path).ok()?;
            if !meta.is_file() {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().to_string();
            field(&mut fields, "Tamaño", human_size(meta.len()));
            if let Ok(m) = meta.modified() {
                let dt: chrono::DateTime<chrono::Local> = m.into();
                field(&mut fields, "Modificado", dt.format("%Y-%m-%d %H:%M").to_string());
            }
            let mut v = view(name, fields, String::new(), None);
            v.download = true;
            Some(v)
        }
        _ => None,
    }
}

fn human_size(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{} B", b) } else { format!("{:.1} {}", v, U[i]) }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn random_token() -> String {
    let mut buf = [0u8; 24];
    getrandom::fill(&mut buf).expect("getrandom");
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Id normalizado del registro: los archivos se guardan por su ruta real
fn normalize_id(conn: &Connection, kind: &str, id: &str) -> Option<String> {
    if kind == "archivo" {
        let roots = crate::storage::canonical_roots(conn);
        return crate::storage::resolve_existing(&roots, id).ok().map(|p| p.to_string_lossy().to_string());
    }
    Some(id.to_string())
}

/// Puede decidir el nivel de acceso: quien gestiona el modulo, o quien creo el registro (con
/// nivel Usar). Un archivo: quien puede leerlo y tiene Usar en Archivos (lo mismo que pide un
/// enlace publico).
fn can_manage(conn: &Connection, s: &SessionInfo, kind: &str, rec: &RecordView, id: &str) -> bool {
    let Some(module) = kind_module(kind) else { return false };
    if s.is_admin() {
        return true;
    }
    if kind == "archivo" {
        let st = crate::storage::Storage::from_conn(conn);
        return s.can("files", Level::Usar) && st.can(s, std::path::Path::new(id), crate::storage::Op::Read);
    }
    s.can(module, Level::Gestionar) || (rec.owner.as_deref() == Some(s.username.as_str()) && s.can(module, Level::Usar))
}

async fn module_on(state: &AppState, kind: &str) -> Result<&'static str, ApiError> {
    let module = kind_module(kind).ok_or((StatusCode::BAD_REQUEST, "Tipo de registro desconocido".to_string()))?;
    if !state.enabled_modules.lock().await.contains(module) {
        return Err((StatusCode::NOT_FOUND, "Modulo no disponible".to_string()));
    }
    Ok(module)
}

fn get_visibility(conn: &Connection, kind: &str, id: &str) -> (Visibility, Option<String>) {
    conn.query_row(
        "SELECT visibility, public_token FROM record_access WHERE kind = ?1 AND record_id = ?2",
        params![kind, id],
        |r| Ok((Visibility::parse(&r.get::<_, String>(0)?), r.get(1)?)),
    )
    .unwrap_or((Visibility::Privado, None))
}

#[derive(Debug, Serialize)]
pub struct GrantRow {
    pub id: String,
    pub username: String,
    pub expires_at: i64,
    pub note: String,
    pub granted_by: String,
    pub created_at: i64,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct AccessInfo {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub visibility: Visibility,
    pub public_token: Option<String>,
    pub grants: Vec<GrantRow>,
}

fn load_grants(conn: &Connection, kind: &str, id: &str) -> Result<Vec<GrantRow>, String> {
    let t = now();
    let mut stmt = conn
        .prepare(
            "SELECT id, username, expires_at, note, granted_by, created_at FROM record_grants
             WHERE kind = ?1 AND record_id = ?2 ORDER BY expires_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![kind, id], |r| {
            let expires_at: i64 = r.get(2)?;
            Ok(GrantRow {
                id: r.get(0)?,
                username: r.get(1)?,
                expires_at,
                note: r.get(3)?,
                granted_by: r.get(4)?,
                created_at: r.get(5)?,
                active: expires_at > t,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct RecordQuery {
    pub kind: String,
    pub id: String,
}

/// Registro existente que la sesion puede gestionar: (id normalizado, ficha)
fn managed(conn: &Connection, s: &SessionInfo, kind: &str, raw_id: &str) -> Result<(String, RecordView), ApiError> {
    let not_found = || (StatusCode::NOT_FOUND, "Registro no encontrado".to_string());
    let id = normalize_id(conn, kind, raw_id).ok_or_else(not_found)?;
    let rec = load_record(conn, kind, &id).ok_or_else(not_found)?;
    if !can_manage(conn, s, kind, &rec, &id) {
        return Err((StatusCode::FORBIDDEN, "No puedes cambiar quien ve este registro".to_string()));
    }
    Ok((id, rec))
}

/// GET /api/access?kind=&id= — nivel, autorizaciones y enlace publico
pub async fn get_access(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<RecordQuery>,
) -> Result<Json<AccessInfo>, ApiError> {
    module_on(&state, &q.kind).await?;
    db_op_status(&state.db, move |conn| {
        let (id, rec) = managed(conn, &session, &q.kind, &q.id)?;
        let (visibility, public_token) = get_visibility(conn, &q.kind, &id);
        let grants = load_grants(conn, &q.kind, &id).map_err(internal)?;
        Ok(AccessInfo { kind: q.kind, id, title: rec.title, visibility, public_token, grants })
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct SetVisibility {
    pub kind: String,
    pub id: String,
    pub visibility: Visibility,
}

/// PUT /api/access — cambia el nivel. Al dejar de ser publico el enlace muere; al volver a
/// publicarlo se genera otro.
pub async fn set_access(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<SetVisibility>,
) -> Result<Json<AccessInfo>, ApiError> {
    module_on(&state, &req.kind).await?;
    let user = session.username.clone();
    let (info, change) = db_op_status(&state.db, move |conn| {
        let (id, rec) = managed(conn, &session, &req.kind, &req.id)?;
        let (before, token) = get_visibility(conn, &req.kind, &id);
        let token = match req.visibility {
            Visibility::Publico => Some(token.unwrap_or_else(random_token)),
            _ => None,
        };
        conn.execute(
            "INSERT INTO record_access (kind, record_id, visibility, public_token, updated_by, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(kind, record_id) DO UPDATE SET visibility = ?3, public_token = ?4, updated_by = ?5, updated_at = ?6",
            params![req.kind, id, req.visibility.as_str(), token, session.username, now()],
        )
        .map_err(internal)?;
        let grants = load_grants(conn, &req.kind, &id).map_err(internal)?;
        let change = (before != req.visibility).then(|| {
            format!("{} «{}»: {} → {}", rec.kind_label, rec.title, before.label(), req.visibility.label())
        });
        Ok((AccessInfo { kind: req.kind, id, title: rec.title, visibility: req.visibility, public_token: token, grants }, change))
    })
    .await?;
    if let Some(c) = change {
        state.log_activity("Nivel de acceso", &c, &user).await;
    }
    Ok(Json(info))
}

#[derive(Deserialize)]
pub struct NewGrant {
    pub kind: String,
    pub id: String,
    pub username: String,
    /// Unix (segundos): hasta cuando
    pub expires_at: i64,
    #[serde(default)]
    pub note: String,
}

/// POST /api/access/grants — autoriza a una persona hasta una fecha. Si el registro era
/// privado pasa a «con autorizacion».
pub async fn add_grant(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<NewGrant>,
) -> Result<Json<AccessInfo>, ApiError> {
    module_on(&state, &req.kind).await?;
    if req.expires_at <= now() {
        return Err((StatusCode::BAD_REQUEST, "La fecha de termino tiene que ser futura".into()));
    }
    if req.note.len() > 300 {
        return Err((StatusCode::BAD_REQUEST, "Nota demasiado larga".into()));
    }
    let user = session.username.clone();
    let (info, msg, grantee) = db_op_status(&state.db, move |conn| {
        let (id, rec) = managed(conn, &session, &req.kind, &req.id)?;
        let exists: bool = conn
            .query_row("SELECT 1 FROM web_users WHERE username = ?1 AND role != 'pendiente'", [&req.username], |_| Ok(true))
            .optional()
            .map_err(internal)?
            .unwrap_or(false);
        if !exists {
            return Err((StatusCode::BAD_REQUEST, "Esa cuenta no existe o esta pendiente".into()));
        }
        // Una autorizacion vigente por persona: renovar reemplaza la anterior
        conn.execute(
            "DELETE FROM record_grants WHERE kind = ?1 AND record_id = ?2 AND username = ?3",
            params![req.kind, id, req.username],
        )
        .map_err(internal)?;
        conn.execute(
            "INSERT INTO record_grants (id, kind, record_id, username, expires_at, note, granted_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![uuid::Uuid::new_v4().simple().to_string(), req.kind, id, req.username, req.expires_at, req.note.trim(), session.username, now()],
        )
        .map_err(internal)?;
        let (vis, token) = get_visibility(conn, &req.kind, &id);
        let vis = if vis == Visibility::Privado {
            conn.execute(
                "INSERT INTO record_access (kind, record_id, visibility, public_token, updated_by, updated_at)
                 VALUES (?1, ?2, 'autorizado', NULL, ?3, ?4)
                 ON CONFLICT(kind, record_id) DO UPDATE SET visibility = 'autorizado', updated_by = ?3, updated_at = ?4",
                params![req.kind, id, session.username, now()],
            )
            .map_err(internal)?;
            Visibility::Autorizado
        } else {
            vis
        };
        let until = chrono::DateTime::from_timestamp(req.expires_at, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let msg = format!("Autorizo a {} hasta {}: {} «{}»", req.username, until, rec.kind_label, rec.title);
        let grants = load_grants(conn, &req.kind, &id).map_err(internal)?;
        let grantee = (req.username.clone(), format!("{} «{}»", rec.kind_label, rec.title), until);
        Ok((AccessInfo { kind: req.kind, id, title: rec.title, visibility: vis, public_token: token, grants }, msg, grantee))
    })
    .await?;
    state.log_activity("Nivel de acceso", &msg, &user).await;
    // Aviso a la persona autorizada (si tiene Telegram vinculado)
    let (who, what, until) = grantee;
    let text = format!("🤝 {} te dio acceso a {} hasta el {}. Lo ves en LabNAS > Compartido conmigo.", user, what, until);
    crate::handlers::notifications::telegram::notify_users(&state, &[who], &text).await;
    Ok(Json(info))
}

/// DELETE /api/access/grants/{id}
pub async fn delete_grant(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(grant_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let row: Option<(String, String, String)> = {
        let gid = grant_id.clone();
        db_op(&state.db, move |conn| {
            conn.query_row("SELECT kind, record_id, username FROM record_grants WHERE id = ?1", [gid], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()
            .map_err(|e| e.to_string())
        })
        .await?
    };
    let (kind, id, who) = row.ok_or((StatusCode::NOT_FOUND, "Autorizacion no encontrada".to_string()))?;
    module_on(&state, &kind).await?;
    let user = session.username.clone();
    let msg = db_op_status(&state.db, move |conn| {
        let (_, rec) = managed(conn, &session, &kind, &id)?;
        conn.execute("DELETE FROM record_grants WHERE id = ?1", [grant_id]).map_err(internal)?;
        Ok(format!("Quito la autorizacion de {}: {} «{}»", who, rec.kind_label, rec.title))
    })
    .await?;
    state.log_activity("Nivel de acceso", &msg, &user).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct LevelsQuery {
    pub kind: String,
    /// Ids separados por salto de linea
    pub ids: String,
}

/// GET /api/access/levels?kind=&ids=a%0Ab — nivel de varios registros (para las insignias de
/// las listas). Solo los que no son privados.
pub async fn get_levels(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<LevelsQuery>,
) -> Result<Json<std::collections::HashMap<String, Visibility>>, ApiError> {
    let module = module_on(&state, &q.kind).await?;
    if !session.can(module, Level::Ver) {
        return Err((StatusCode::FORBIDDEN, "Sin permisos para esta accion".into()));
    }
    db_op(&state.db, move |conn| {
        let mut out = std::collections::HashMap::new();
        // Separados por salto de linea (una ruta de archivo puede tener comas)
        for id in q.ids.split('\n').filter(|s| !s.is_empty()).take(500) {
            let Some(norm) = normalize_id(conn, &q.kind, id) else { continue };
            let (v, _) = get_visibility(conn, &q.kind, &norm);
            if v != Visibility::Privado {
                out.insert(id.to_string(), v);
            }
        }
        Ok(out)
    })
    .await
    .map(Json)
}

#[derive(Debug, Serialize)]
pub struct SharedItem {
    pub kind: String,
    pub kind_label: &'static str,
    pub id: String,
    pub title: String,
    pub expires_at: i64,
    pub granted_by: String,
}

/// GET /api/shared — lo que me autorizaron y sigue vigente
pub async fn list_shared(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<Vec<SharedItem>>, ApiError> {
    let on = state.enabled_modules.lock().await.clone();
    db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT g.kind, g.record_id, g.expires_at, g.granted_by FROM record_grants g
                 JOIN record_access a ON a.kind = g.kind AND a.record_id = g.record_id
                 WHERE g.username = ?1 AND g.expires_at > ?2 AND a.visibility != 'privado'
                 ORDER BY g.expires_at",
            )
            .map_err(|e| e.to_string())?;
        let rows: Vec<(String, String, i64, String)> = stmt
            .query_map(params![session.username, now()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows
            .into_iter()
            .filter(|(k, ..)| kind_module(k).is_some_and(|m| on.contains(m)))
            .filter_map(|(kind, id, expires_at, granted_by)| {
                let rec = load_record(conn, &kind, &id)?;
                Some(SharedItem { kind_label: rec.kind_label, kind, id, title: rec.title, expires_at, granted_by })
            })
            .collect())
    })
    .await
    .map(Json)
}

/// Autorizacion vigente de `user` sobre el registro (y el registro no es privado)
fn has_grant(conn: &Connection, user: &str, kind: &str, id: &str) -> bool {
    let (vis, _) = get_visibility(conn, kind, id);
    if vis == Visibility::Publico {
        return true;
    }
    vis == Visibility::Autorizado
        && conn
            .query_row(
                "SELECT 1 FROM record_grants WHERE kind = ?1 AND record_id = ?2 AND username = ?3 AND expires_at > ?4",
                params![kind, id, user, now()],
                |_| Ok(()),
            )
            .is_ok()
}

async fn shared_record(state: &AppState, session: &SessionInfo, kind: String, raw_id: String) -> Result<(String, RecordView), ApiError> {
    module_on(state, &kind).await?;
    let user = session.username.clone();
    let (id, rec) = db_op_status(&state.db, move |conn| {
        let none = || (StatusCode::NOT_FOUND, "No encontrado o sin autorizacion vigente".to_string());
        let id = normalize_id(conn, &kind, &raw_id).ok_or_else(none)?;
        if !has_grant(conn, &user, &kind, &id) {
            return Err(none());
        }
        let rec = load_record(conn, &kind, &id).ok_or_else(none)?;
        Ok((id, rec))
    })
    .await?;
    Ok((id, rec))
}

/// GET /api/shared/view?kind=&id= — la ficha de algo que me autorizaron
pub async fn view_shared(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<RecordQuery>,
) -> Result<Json<RecordView>, ApiError> {
    let (_, rec) = shared_record(&state, &session, q.kind, q.id).await?;
    state.log_access(&format!("Vio por autorizacion: {} «{}»", rec.kind_label, rec.title), &session.username).await;
    Ok(Json(rec))
}

/// GET /api/shared/download?kind=archivo&id= — descargar un archivo autorizado
pub async fn download_shared(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<RecordQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if q.kind != "archivo" {
        return Err((StatusCode::BAD_REQUEST, "Solo los archivos se descargan".into()));
    }
    let (id, rec) = shared_record(&state, &session, q.kind, q.id).await?;
    state.log_access(&format!("Descargo por autorizacion: {}", id), &session.username).await;
    stream_file(&id, &rec.title).await
}

async fn public_record(state: &AppState, token: String) -> Result<(String, RecordView), ApiError> {
    let none = || (StatusCode::NOT_FOUND, "No encontrado".to_string());
    if token.len() != 48 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(none());
    }
    let found: Option<(String, String)> = db_op(&state.db, move |conn| {
        conn.query_row(
            "SELECT kind, record_id FROM record_access WHERE public_token = ?1 AND visibility = 'publico'",
            [token],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())
    })
    .await?;
    let (kind, id) = found.ok_or_else(none)?;
    // Modulo apagado: el enlace no existe
    module_on(state, &kind).await.map_err(|_| none())?;
    let id2 = id.clone();
    let rec = db_op(&state.db, move |conn| Ok(load_record(conn, &kind, &id2))).await?.ok_or_else(none)?;
    Ok((id, rec))
}

/// GET /api/public/{token} — ficha publica, sin cuenta
pub async fn view_public(State(state): State<AppState>, Path(token): Path<String>) -> Result<Json<RecordView>, ApiError> {
    let (_, rec) = public_record(&state, token).await?;
    state.log_access(&format!("Vista publica: {} «{}»", rec.kind_label, rec.title), "enlace publico").await;
    Ok(Json(rec))
}

/// GET /api/public/{token}/download — archivo publico
pub async fn download_public(State(state): State<AppState>, Path(token): Path<String>) -> Result<impl IntoResponse, ApiError> {
    let (id, rec) = public_record(&state, token).await?;
    if !rec.download {
        return Err((StatusCode::NOT_FOUND, "No encontrado".into()));
    }
    state.log_access(&format!("Descarga publica: {}", id), "enlace publico").await;
    stream_file(&id, &rec.title).await
}

async fn stream_file(path: &str, name: &str) -> Result<impl IntoResponse, ApiError> {
    let file = tokio::fs::File::open(path).await.map_err(|_| (StatusCode::NOT_FOUND, "No encontrado".to_string()))?;
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let safe = name.replace('"', "");
    let headers = [
        (header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (header::CONTENT_LENGTH, len.to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"; filename*=UTF-8''{}", safe, urlencoding::encode(&safe)),
        ),
    ];
    Ok((headers, Body::from_stream(tokio_util::io::ReaderStream::new(file))))
}

/// Al borrar un registro: olvidar su nivel y sus autorizaciones
pub fn forget(conn: &Connection, kind: &str, id: &str) {
    conn.execute("DELETE FROM record_access WHERE kind = ?1 AND record_id = ?2", params![kind, id]).ok();
    conn.execute("DELETE FROM record_grants WHERE kind = ?1 AND record_id = ?2", params![kind, id]).ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tipos_con_modulo_conocido() {
        let ids = crate::modules::ids();
        for (_, m, _) in KINDS {
            assert!(ids.contains(m), "{m}");
        }
    }

    #[test]
    fn montos_con_puntos() {
        assert_eq!(money(1234567.0), "$1.234.567");
        assert_eq!(money(999.0), "$999");
        assert_eq!(money(-1000.0), "-$1.000");
    }
}
