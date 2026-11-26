//! Presupuesto por item, ejecucion en vivo, bienes por proyecto, hitos, caja chica e ingresos (ideas 52, 59, 60, 64, 65). Ver docs/FINANZAS.md.
//!
//! - Configuracion financiera de cada proyecto del Portafolio (`fin_projects`) y presupuesto por
//!   item (`fin_budget_items`). Cambiar un monto aprobado exige motivo y deja antes/despues en
//!   la bitacora. Un proyecto cerrado queda solo de lectura.
//! - La ejecucion (aprobado, comprometido, gastado, disponible) suma `fin_commitments`, que
//!   alimenta cada flujo con `crate::finance::set_commitment`.
//! - Avisos por Telegram al cruzar el % de alerta y el 100%, una sola vez por umbral.
//! - La caja chica y los ingresos viven en `caja.rs`.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Extension, Json, Router,
};
use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::db_op_status;
use crate::error::{internal, ApiError};
use crate::finance::{self, CATEGORIES};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

mod caja;

pub use caja::pending_arqueos;

/// Porcentaje de alerta por defecto de un proyecto
const DEFAULT_ALERT_PCT: f64 = 80.0;
/// Monto maximo aceptado (evita desbordes y errores de tipeo)
const MAX_AMOUNT: f64 = 1e12;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/finance/projects/{pid}/config", get(get_config).put(put_config))
        .route("/api/finance/budget/items", post(create_item))
        .route("/api/finance/budget/items/{id}", put(update_item).delete(delete_item))
        .route("/api/finance/budget/items/{id}/detail", get(item_detail))
        .route("/api/finance/execution", get(execution_summary))
        .route("/api/finance/execution/{pid}", get(execution_project))
        .route("/api/finance/assets", get(assets))
        .merge(caja::routes())
}

// ─── Ayudas comunes (tambien las usa caja.rs) ───

pub(super) fn bad(msg: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.into())
}
pub(super) fn conflict(msg: impl Into<String>) -> ApiError {
    (StatusCode::CONFLICT, msg.into())
}
pub(super) fn forbidden(msg: impl Into<String>) -> ApiError {
    (StatusCode::FORBIDDEN, msg.into())
}
pub(super) fn not_found(msg: impl Into<String>) -> ApiError {
    (StatusCode::NOT_FOUND, msg.into())
}

pub(super) fn is_staff(s: &SessionInfo) -> bool {
    s.can("finanzas", Level::Gestionar)
}

pub(super) fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub(super) fn publish(state: &AppState) {
    state.events.publish("finance", (), crate::events::Audience::All, Some("finanzas"));
}

/// Texto obligatorio o vacio, recortado y con largo maximo
pub(super) fn clean(s: &str, max: usize, label: &str, required: bool) -> Result<String, ApiError> {
    let t = s.trim().to_string();
    if required && t.is_empty() {
        return Err(bad(format!("{} es obligatorio", label)));
    }
    if t.chars().count() > max {
        return Err(bad(format!("{} admite hasta {} caracteres", label, max)));
    }
    Ok(t)
}

pub(super) fn money_ok(v: f64) -> bool {
    v.is_finite() && (0.0..=MAX_AMOUNT).contains(&v)
}

/// Fecha `AAAA-MM-DD` valida
pub(super) fn parse_day(s: &str, label: &str) -> Result<NaiveDate, ApiError> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| bad(format!("{}: fecha invalida (AAAA-MM-DD)", label)))
}

pub(super) fn user_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM web_users WHERE username = ?1)", [name], |r| r.get(0))
}

/// Texto de usuario dentro de un mensaje Markdown de Telegram
pub(super) fn md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '_' | '*' | '`' | '[') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Asiento en la bitacora que anula uno anterior (misma regla que `finance::ledger`: llamar
/// despues del db_op, nunca dentro)
pub(super) async fn ledger_annul(
    state: &AppState,
    user: &str,
    kind: &str,
    subject: &str,
    annuls: i64,
    data: serde_json::Value,
) -> Result<i64, ApiError> {
    let (user, kind, subject) = (user.to_string(), kind.to_string(), subject.to_string());
    crate::db::db_op(&state.db, move |conn| {
        crate::ledger::append(conn, &user, &kind, &subject, Some(annuls), &data).map(|e| e.seq)
    })
    .await
}

/// Configuracion financiera de un proyecto (con valores por defecto si aun no se configura)
#[derive(Debug, Clone, Serialize)]
pub struct ProjectConfig {
    pub project_id: String,
    pub title: String,
    pub currency: String,
    pub responsible: String,
    pub alert_pct: f64,
    pub closed_at: Option<String>,
    /// Ya tiene fila propia (se configuro o tiene presupuesto)
    pub configured: bool,
}

pub(super) fn load_config(conn: &Connection, pid: &str) -> Result<ProjectConfig, ApiError> {
    let title = finance::project_title(conn, pid).map_err(internal)?.ok_or_else(|| not_found("Proyecto no encontrado en el Portafolio"))?;
    let row = conn
        .query_row("SELECT currency, responsible, alert_pct, closed_at FROM fin_projects WHERE project_id = ?1", [pid], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?, r.get::<_, Option<String>>(3)?))
        })
        .optional()
        .map_err(internal)?;
    let configured = row.is_some();
    let (currency, responsible, alert_pct, closed_at) = row.unwrap_or(("CLP".into(), String::new(), DEFAULT_ALERT_PCT, None));
    Ok(ProjectConfig { project_id: pid.to_string(), title, currency, responsible, alert_pct, closed_at, configured })
}

/// Configuracion de un proyecto que debe seguir abierto (409 si esta cerrado)
pub(super) fn open_config(conn: &Connection, pid: &str) -> Result<ProjectConfig, ApiError> {
    let c = load_config(conn, pid)?;
    if c.closed_at.is_some() {
        return Err(conflict("El proyecto esta cerrado: solo lectura"));
    }
    Ok(c)
}

/// Crea la fila de `fin_projects` con los valores por defecto si no existe
pub(super) fn ensure_project(conn: &Connection, pid: &str, user: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO fin_projects (project_id, currency, responsible, alert_pct, created_by, created_at)
         VALUES (?1, 'CLP', '', ?2, ?3, ?4)",
        params![pid, DEFAULT_ALERT_PCT, user, finance::now()],
    )?;
    Ok(())
}

/// Nombre con el que se registra el proyecto en la bitacora
fn subject(c: &ProjectConfig) -> String {
    format!("proyecto: {}", c.title)
}

// ─── Configuracion del proyecto (idea 52) ───

/// GET /api/finance/projects/{pid}/config
async fn get_config(State(state): State<AppState>, Path(pid): Path<String>) -> Result<Json<ProjectConfig>, ApiError> {
    db_op_status(&state.db, move |conn| load_config(conn, &pid)).await.map(Json)
}

#[derive(Deserialize)]
struct ConfigReq {
    #[serde(default)]
    responsible: String,
    alert_pct: Option<f64>,
    currency: Option<String>,
    /// true cierra el proyecto (queda de solo lectura); false lo reabre (exige motivo)
    closed: Option<bool>,
    #[serde(default)]
    reason: String,
}

/// PUT /api/finance/projects/{pid}/config — responsable, % de alerta, moneda y cierre
async fn put_config(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(pid): Path<String>,
    Json(req): Json<ConfigReq>,
) -> Result<Json<ProjectConfig>, ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas configura proyectos"));
    }
    let responsible = clean(&req.responsible, 60, "El responsable", false)?;
    let reason = clean(&req.reason, 300, "El motivo", false)?;
    let user = session.username.clone();
    let reason_c = reason.clone();
    let (before, after) = db_op_status(&state.db, move |conn| {
        let reason = reason_c;
        let before = load_config(conn, &pid)?;
        if !responsible.is_empty() && !user_exists(conn, &responsible).map_err(internal)? {
            return Err(bad("El responsable debe ser una cuenta de LabNAS"));
        }
        let alert_pct = req.alert_pct.unwrap_or(before.alert_pct);
        if !(1.0..=99.0).contains(&alert_pct) {
            return Err(bad("El porcentaje de alerta va entre 1 y 99"));
        }
        let currency = req.currency.map(|c| c.trim().to_uppercase()).unwrap_or_else(|| before.currency.clone());
        if currency.len() != 3 || !currency.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(bad("La moneda es un codigo de 3 letras (CLP, USD, EUR...)"));
        }
        let was_closed = before.closed_at.is_some();
        let want_closed = req.closed.unwrap_or(was_closed);
        if was_closed && want_closed {
            return Err(conflict("El proyecto esta cerrado: solo lectura"));
        }
        if was_closed && reason.is_empty() {
            return Err(bad("Para reabrir un proyecto cerrado indica el motivo"));
        }
        let closed_at = match (was_closed, want_closed) {
            (false, true) => Some(finance::now()),
            (true, false) => None,
            _ => before.closed_at.clone(),
        };
        ensure_project(conn, &before.project_id, &user).map_err(internal)?;
        conn.execute(
            "UPDATE fin_projects SET currency = ?2, responsible = ?3, alert_pct = ?4, closed_at = ?5 WHERE project_id = ?1",
            params![before.project_id, currency, responsible, alert_pct, closed_at],
        )
        .map_err(internal)?;
        let after = load_config(conn, &before.project_id)?;
        Ok((before, after))
    })
    .await?;
    let kind = match (before.closed_at.is_some(), after.closed_at.is_some()) {
        (false, true) => "proyecto_cerrado",
        (true, false) => "proyecto_reabierto",
        _ => "configuracion",
    };
    finance::ledger(
        &state,
        &session.username,
        "presupuesto",
        &subject(&after),
        json!({
            "accion": kind,
            "motivo": reason,
            "antes": { "responsable": before.responsible, "alerta_pct": before.alert_pct, "moneda": before.currency },
            "despues": { "responsable": after.responsible, "alerta_pct": after.alert_pct, "moneda": after.currency },
        }),
    )
    .await?;
    state.log_activity("Finanzas", &format!("{}: {}", kind, after.title), &session.username).await;
    publish(&state);
    Ok(Json(after))
}

// ─── Items de presupuesto (idea 52) ───

#[derive(Debug, Clone, Serialize)]
pub struct ItemExec {
    pub id: String,
    pub project_id: String,
    pub category: String,
    pub name: String,
    pub approved: f64,
    pub notes: String,
    pub committed: f64,
    pub spent: f64,
    /// aprobado - comprometido - gastado (puede ser negativo)
    pub available: f64,
    /// (comprometido + gastado) / aprobado, en %; None si no hay monto aprobado
    pub pct: Option<f64>,
    /// ok | alerta | tope
    pub level: &'static str,
}

/// Estado de un item segun lo usado: `alerta` desde el % del proyecto, `tope` desde el 100%
pub fn level_of(approved: f64, used: f64, alert_pct: f64) -> &'static str {
    if approved <= 0.0 {
        return if used > 0.0 { "tope" } else { "ok" };
    }
    let pct = used / approved * 100.0;
    if pct >= 100.0 {
        "tope"
    } else if pct >= alert_pct {
        "alerta"
    } else {
        "ok"
    }
}

const ITEM_SQL: &str = "SELECT i.id, i.project_id, i.category, i.name, i.approved, i.notes,
        COALESCE((SELECT SUM(c.committed) FROM fin_commitments c WHERE c.budget_item_id = i.id), 0),
        COALESCE((SELECT SUM(c.spent) FROM fin_commitments c WHERE c.budget_item_id = i.id), 0)
    FROM fin_budget_items i";

fn exec_row(r: &rusqlite::Row, alert_pct: f64) -> rusqlite::Result<ItemExec> {
    let approved: f64 = r.get(4)?;
    let (committed, spent): (f64, f64) = (r.get(6)?, r.get(7)?);
    let used = committed + spent;
    Ok(ItemExec {
        id: r.get(0)?,
        project_id: r.get(1)?,
        category: r.get(2)?,
        name: r.get(3)?,
        approved,
        notes: r.get(5)?,
        committed,
        spent,
        available: approved - used,
        pct: (approved > 0.0).then(|| used / approved * 100.0),
        level: level_of(approved, used, alert_pct),
    })
}

/// Ejecucion de un item (aprobado, comprometido, gastado, disponible y %). Los demas modulos
/// no dependen de esto; sirve para mostrar el estado de un item en otras pantallas.
pub fn item_status(conn: &Connection, item_id: &str) -> rusqlite::Result<Option<ItemExec>> {
    let alert_pct: f64 = conn
        .query_row(
            "SELECT COALESCE((SELECT p.alert_pct FROM fin_projects p WHERE p.project_id = i.project_id), ?2) FROM fin_budget_items i WHERE i.id = ?1",
            params![item_id, DEFAULT_ALERT_PCT],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(DEFAULT_ALERT_PCT);
    conn.query_row(&format!("{} WHERE i.id = ?1", ITEM_SQL), [item_id], |r| exec_row(r, alert_pct)).optional()
}

/// Item existente de un proyecto abierto (404 / 409 si no)
pub(super) fn open_item(conn: &Connection, item_id: &str) -> Result<(ItemExec, ProjectConfig), ApiError> {
    let pid: String = conn
        .query_row("SELECT project_id FROM fin_budget_items WHERE id = ?1", [item_id], |r| r.get(0))
        .optional()
        .map_err(internal)?
        .ok_or_else(|| not_found("Item de presupuesto no encontrado"))?;
    let cfg = open_config(conn, &pid)?;
    let item = item_status(conn, item_id).map_err(internal)?.ok_or_else(|| not_found("Item de presupuesto no encontrado"))?;
    Ok((item, cfg))
}

#[derive(Deserialize)]
struct ItemReq {
    /// Solo al crear
    #[serde(default)]
    project_id: String,
    category: String,
    name: String,
    approved: f64,
    #[serde(default)]
    notes: String,
    /// Motivo del cambio de monto (obligatorio al modificar el aprobado)
    #[serde(default)]
    reason: String,
}

fn check_item(req: &ItemReq) -> Result<(String, String, String), ApiError> {
    if !CATEGORIES.contains(&req.category.as_str()) {
        return Err(bad(format!("Categoria invalida (usa: {})", CATEGORIES.join(", "))));
    }
    if !money_ok(req.approved) {
        return Err(bad("Monto aprobado invalido"));
    }
    Ok((clean(&req.name, 120, "El nombre", true)?, clean(&req.notes, 500, "Las notas", false)?, clean(&req.reason, 300, "El motivo", false)?))
}

/// POST /api/finance/budget/items
async fn create_item(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<ItemReq>,
) -> Result<(StatusCode, Json<ItemExec>), ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas arma el presupuesto"));
    }
    let (name, notes, _) = check_item(&req)?;
    let user = session.username.clone();
    let (item, cfg) = db_op_status(&state.db, move |conn| {
        let cfg = open_config(conn, req.project_id.trim())?;
        let dup: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM fin_budget_items WHERE project_id = ?1 AND category = ?2 AND lower(name) = lower(?3))",
                params![cfg.project_id, req.category, name],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if dup {
            return Err(conflict("Ya hay un item con ese nombre en la categoria"));
        }
        ensure_project(conn, &cfg.project_id, &user).map_err(internal)?;
        let id = finance::new_id();
        conn.execute(
            "INSERT INTO fin_budget_items (id, project_id, category, name, approved, notes, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, cfg.project_id, req.category, name, req.approved, notes, user, finance::now()],
        )
        .map_err(internal)?;
        let item = item_status(conn, &id).map_err(internal)?.ok_or_else(|| internal("item"))?;
        Ok((item, cfg))
    })
    .await?;
    finance::ledger(
        &state,
        &session.username,
        "presupuesto",
        &subject(&cfg),
        json!({ "accion": "item_creado", "item": item.id, "categoria": item.category, "nombre": item.name, "aprobado": item.approved }),
    )
    .await?;
    state.log_activity("Finanzas", &format!("Item de presupuesto «{}» en {}", item.name, cfg.title), &session.username).await;
    publish(&state);
    Ok((StatusCode::CREATED, Json(item)))
}

/// PUT /api/finance/budget/items/{id} — cambiar el monto aprobado exige motivo y deja
/// antes/despues en la bitacora
async fn update_item(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<ItemReq>,
) -> Result<Json<ItemExec>, ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas edita el presupuesto"));
    }
    let (name, notes, reason) = check_item(&req)?;
    let user = session.username.clone();
    let change_id = finance::new_id();
    let cid = change_id.clone();
    let (before, after, cfg, amount_changed) = db_op_status(&state.db, move |conn| {
        let (before, cfg) = open_item(conn, &id)?;
        let amount_changed = (before.approved - req.approved).abs() > f64::EPSILON;
        if amount_changed {
            if reason.is_empty() {
                return Err(bad("Para cambiar el monto aprobado indica el motivo"));
            }
            let used = before.committed + before.spent;
            if req.approved < used {
                return Err(conflict(format!(
                    "El aprobado no puede quedar bajo lo ya comprometido y gastado ({:.0})",
                    used
                )));
            }
        }
        let dup: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM fin_budget_items WHERE project_id = ?1 AND category = ?2 AND lower(name) = lower(?3) AND id != ?4)",
                params![cfg.project_id, req.category, name, id],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if dup {
            return Err(conflict("Ya hay un item con ese nombre en la categoria"));
        }
        conn.execute(
            "UPDATE fin_budget_items SET category = ?2, name = ?3, approved = ?4, notes = ?5 WHERE id = ?1",
            params![id, req.category, name, req.approved, notes],
        )
        .map_err(internal)?;
        if amount_changed {
            conn.execute(
                "INSERT INTO fin_budget_changes (id, item_id, project_id, old_amount, new_amount, reason, changed_by, changed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![cid, id, cfg.project_id, before.approved, req.approved, reason, user, finance::now()],
            )
            .map_err(internal)?;
        }
        let after = item_status(conn, &id).map_err(internal)?.ok_or_else(|| internal("item"))?;
        Ok((before, after, cfg, amount_changed))
    })
    .await?;
    let seq = finance::ledger(
        &state,
        &session.username,
        "presupuesto",
        &subject(&cfg),
        json!({
            "accion": if amount_changed { "monto_modificado" } else { "item_editado" },
            "item": after.id,
            "motivo": req.reason.trim(),
            "antes": { "categoria": before.category, "nombre": before.name, "aprobado": before.approved, "notas": before.notes },
            "despues": { "categoria": after.category, "nombre": after.name, "aprobado": after.approved, "notas": after.notes },
        }),
    )
    .await?;
    if amount_changed {
        db_op_status(&state.db, move |conn| {
            conn.execute("UPDATE fin_budget_changes SET ledger_seq = ?2 WHERE id = ?1", params![change_id, seq]).map_err(internal)
        })
        .await?;
        state
            .log_activity(
                "Finanzas",
                &format!("Aprobado de «{}» ({}): {:.0} -> {:.0}", after.name, cfg.title, before.approved, after.approved),
                &session.username,
            )
            .await;
    }
    run_alerts(&state).await;
    publish(&state);
    Ok(Json(after))
}

/// DELETE /api/finance/budget/items/{id} — no se puede si tiene compromisos
async fn delete_item(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas edita el presupuesto"));
    }
    let (item, cfg) = db_op_status(&state.db, move |conn| {
        let (item, cfg) = open_item(conn, &id)?;
        if item.committed + item.spent > 0.0 {
            return Err(conflict("El item tiene compromisos o gastos: no se puede borrar"));
        }
        conn.execute("UPDATE fin_commitments SET budget_item_id = NULL WHERE budget_item_id = ?1", [&id]).map_err(internal)?;
        conn.execute("DELETE FROM fin_budget_alerts WHERE item_id = ?1", [&id]).map_err(internal)?;
        conn.execute("DELETE FROM fin_budget_items WHERE id = ?1", [&id]).map_err(internal)?;
        Ok((item, cfg))
    })
    .await?;
    finance::ledger(
        &state,
        &session.username,
        "presupuesto",
        &subject(&cfg),
        json!({ "accion": "item_eliminado", "item": item.id, "categoria": item.category, "nombre": item.name, "aprobado": item.approved }),
    )
    .await?;
    state.log_activity("Finanzas", &format!("Item de presupuesto «{}» eliminado ({})", item.name, cfg.title), &session.username).await;
    publish(&state);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct ChangeRow {
    old_amount: f64,
    new_amount: f64,
    reason: String,
    changed_by: String,
    changed_at: String,
    ledger_seq: Option<i64>,
}

#[derive(Serialize)]
struct CommitmentRow {
    source: String,
    source_id: String,
    label: String,
    committed: f64,
    spent: f64,
    updated_at: String,
}

#[derive(Serialize)]
struct ItemDetail {
    item: ItemExec,
    changes: Vec<ChangeRow>,
    commitments: Vec<CommitmentRow>,
}

/// GET /api/finance/budget/items/{id}/detail — historial de montos y compromisos del item
async fn item_detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<ItemDetail>, ApiError> {
    db_op_status(&state.db, move |conn| {
        let item = item_status(conn, &id).map_err(internal)?.ok_or_else(|| not_found("Item de presupuesto no encontrado"))?;
        let mut stmt = conn
            .prepare("SELECT old_amount, new_amount, reason, changed_by, changed_at, ledger_seq FROM fin_budget_changes WHERE item_id = ?1 ORDER BY changed_at")
            .map_err(internal)?;
        let changes = stmt
            .query_map([&id], |r| {
                Ok(ChangeRow { old_amount: r.get(0)?, new_amount: r.get(1)?, reason: r.get(2)?, changed_by: r.get(3)?, changed_at: r.get(4)?, ledger_seq: r.get(5)? })
            })
            .map_err(internal)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(internal)?;
        let mut stmt = conn
            .prepare(
                "SELECT source, source_id, label, committed, spent, updated_at FROM fin_commitments
                 WHERE budget_item_id = ?1 AND (committed != 0 OR spent != 0) ORDER BY updated_at DESC LIMIT 200",
            )
            .map_err(internal)?;
        let commitments = stmt
            .query_map([&id], |r| {
                Ok(CommitmentRow { source: r.get(0)?, source_id: r.get(1)?, label: r.get(2)?, committed: r.get(3)?, spent: r.get(4)?, updated_at: r.get(5)? })
            })
            .map_err(internal)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(internal)?;
        Ok(ItemDetail { item, changes, commitments })
    })
    .await
    .map(Json)
}

// ─── Ejecucion en vivo (idea 60) ───

#[derive(Debug, Clone, Default, Serialize)]
pub struct Totals {
    pub approved: f64,
    pub committed: f64,
    pub spent: f64,
    pub available: f64,
    pub pct: Option<f64>,
}

impl Totals {
    fn add(&mut self, approved: f64, committed: f64, spent: f64) {
        self.approved += approved;
        self.committed += committed;
        self.spent += spent;
        self.available = self.approved - self.committed - self.spent;
        self.pct = (self.approved > 0.0).then(|| (self.committed + self.spent) / self.approved * 100.0);
    }
}

#[derive(Serialize)]
struct CategoryExec {
    category: String,
    #[serde(flatten)]
    totals: Totals,
}

#[derive(Serialize)]
struct ProjectExec {
    config: ProjectConfig,
    items: Vec<ItemExec>,
    by_category: Vec<CategoryExec>,
    totals: Totals,
    /// Comprometido/gastado que aun no se asigno a ningun item
    unassigned: Totals,
}

fn project_exec(conn: &Connection, pid: &str) -> Result<ProjectExec, ApiError> {
    let config = load_config(conn, pid)?;
    let mut stmt = conn
        .prepare(&format!("{} WHERE i.project_id = ?1 ORDER BY i.category, i.name", ITEM_SQL))
        .map_err(internal)?;
    let items = stmt
        .query_map([pid], |r| exec_row(r, config.alert_pct))
        .map_err(internal)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(internal)?;
    let mut totals = Totals::default();
    let mut by_category: Vec<CategoryExec> = CATEGORIES.iter().map(|c| CategoryExec { category: c.to_string(), totals: Totals::default() }).collect();
    for i in &items {
        totals.add(i.approved, i.committed, i.spent);
        if let Some(c) = by_category.iter_mut().find(|c| c.category == i.category) {
            c.totals.add(i.approved, i.committed, i.spent);
        }
    }
    by_category.retain(|c| c.totals.approved > 0.0 || c.totals.committed > 0.0 || c.totals.spent > 0.0);
    let (uc, us): (f64, f64) = conn
        .query_row(
            "SELECT COALESCE(SUM(committed), 0), COALESCE(SUM(spent), 0) FROM fin_commitments
             WHERE project_id = ?1 AND (budget_item_id IS NULL OR budget_item_id NOT IN (SELECT id FROM fin_budget_items))",
            [pid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(internal)?;
    let mut unassigned = Totals::default();
    unassigned.add(0.0, uc, us);
    Ok(ProjectExec { config, items, by_category, totals, unassigned })
}

/// GET /api/finance/execution/{pid}
async fn execution_project(State(state): State<AppState>, Path(pid): Path<String>) -> Result<Json<ProjectExec>, ApiError> {
    db_op_status(&state.db, move |conn| project_exec(conn, &pid)).await.map(Json)
}

#[derive(Serialize)]
struct SummaryRow {
    project_id: String,
    title: String,
    currency: String,
    responsible: String,
    closed_at: Option<String>,
    items: i64,
    totals: Totals,
    /// Peor estado entre sus items: ok | alerta | tope
    level: &'static str,
}

/// GET /api/finance/execution — resumen de todos los proyectos con presupuesto o movimientos
async fn execution_summary(State(state): State<AppState>) -> Result<Json<Vec<SummaryRow>>, ApiError> {
    db_op_status(&state.db, move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT e.id FROM portfolio_entries e WHERE e.id IN (
                    SELECT project_id FROM fin_budget_items UNION SELECT project_id FROM fin_projects
                    UNION SELECT project_id FROM fin_commitments) ORDER BY e.title",
            )
            .map_err(internal)?;
        let ids = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(internal)?.collect::<rusqlite::Result<Vec<_>>>().map_err(internal)?;
        let mut out = Vec::new();
        for id in ids {
            let p = project_exec(conn, &id)?;
            let rank = |l: &str| match l {
                "tope" => 2,
                "alerta" => 1,
                _ => 0,
            };
            let worst = p.items.iter().map(|i| i.level).max_by_key(|l| rank(l)).unwrap_or("ok");
            let mut totals = p.totals.clone();
            // Lo sin asignar tambien se ha comprometido o gastado del proyecto
            totals.add(0.0, p.unassigned.committed, p.unassigned.spent);
            out.push(SummaryRow {
                project_id: id,
                title: p.config.title,
                currency: p.config.currency,
                responsible: p.config.responsible,
                closed_at: p.config.closed_at,
                items: p.items.len() as i64,
                totals,
                level: worst,
            });
        }
        Ok(out)
    })
    .await
    .map(Json)
}

// ─── Avisos por umbral (idea 60) ───

/// Un item que cruzo un umbral y aun no se avisó
#[derive(Debug, Clone)]
pub struct BudgetAlert {
    pub title: String,
    pub responsible: String,
    pub item: String,
    /// `alerta` (cruzo el % del proyecto) o `tope` (llego al 100%)
    pub level: &'static str,
    pub approved: f64,
    pub used: f64,
    pub alert_pct: f64,
}

/// Revisa todos los items de proyectos abiertos y devuelve los avisos que faltan, dejando
/// marcado cada umbral. Una marca se borra cuando el item baja del umbral (se rearma).
pub fn pending_alerts(conn: &Connection) -> rusqlite::Result<Vec<BudgetAlert>> {
    let mut stmt = conn.prepare(
        "SELECT i.id, i.project_id, i.name, i.approved,
                COALESCE((SELECT SUM(c.committed + c.spent) FROM fin_commitments c WHERE c.budget_item_id = i.id), 0),
                COALESCE(p.alert_pct, ?1), COALESCE(p.responsible, ''),
                COALESCE((SELECT e.title FROM portfolio_entries e WHERE e.id = i.project_id), i.project_id)
         FROM fin_budget_items i LEFT JOIN fin_projects p ON p.project_id = i.project_id
         WHERE p.closed_at IS NULL",
    )?;
    let rows = stmt
        .query_map([DEFAULT_ALERT_PCT], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    for (id, _project_id, item, approved, used, alert_pct, responsible, title) in rows {
        let level = level_of(approved, used, alert_pct);
        let needed: &[&str] = match level {
            "tope" => &["alerta", "tope"],
            "alerta" => &["alerta"],
            _ => &[],
        };
        let marked: Vec<String> = conn
            .prepare("SELECT level FROM fin_budget_alerts WHERE item_id = ?1")?
            .query_map([&id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for m in marked.iter().filter(|m| !needed.contains(&m.as_str())) {
            conn.execute("DELETE FROM fin_budget_alerts WHERE item_id = ?1 AND level = ?2", params![id, m])?;
        }
        let mut fresh: Option<&'static str> = None;
        for lv in needed {
            if !marked.iter().any(|m| m == lv) {
                conn.execute(
                    "INSERT INTO fin_budget_alerts (item_id, level, sent_at) VALUES (?1, ?2, ?3)",
                    params![id, lv, finance::now()],
                )?;
                // Si cruzo ambos a la vez, avisa solo el mas alto
                fresh = Some(if *lv == "tope" { "tope" } else { "alerta" });
            }
        }
        if let Some(level) = fresh {
            out.push(BudgetAlert { title, responsible, item, level, approved, used, alert_pct });
        }
    }
    Ok(out)
}

/// Revisa los umbrales y avisa por Telegram a quienes gestionan Finanzas y al responsable
pub async fn run_alerts(state: &AppState) {
    let Ok(alerts) = crate::db::db_op(&state.db, |conn| pending_alerts(conn).map_err(|e| e.to_string())).await else { return };
    for a in alerts {
        let staff = crate::db::db_op(&state.db, |conn| {
            crate::roles::users_with_level(conn, "finanzas", Level::Gestionar).map_err(|e| e.to_string())
        })
        .await
        .unwrap_or_default();
        let mut users = staff;
        if !a.responsible.is_empty() {
            users.push(a.responsible.clone());
        }
        users.sort();
        users.dedup();
        let pct = if a.approved > 0.0 { format!("{:.0}%", a.used / a.approved * 100.0) } else { "sin monto aprobado".to_string() };
        let text = if a.level == "tope" {
            format!(
                "🔴 *Presupuesto agotado*\n{} — «{}»: comprometido y gastado {} de {} aprobado ({}).",
                md(&a.title), md(&a.item), finance_money(a.used), finance_money(a.approved), pct
            )
        } else {
            format!(
                "🟠 *Alerta de presupuesto*\n{} — «{}» llego al {:.0}% de alerta: {} de {} aprobado ({}).",
                md(&a.title), md(&a.item), a.alert_pct, finance_money(a.used), finance_money(a.approved), pct
            )
        };
        crate::handlers::notifications::notify_users(state, &users, &text).await;
        state.log_activity("Finanzas", &format!("Alerta de presupuesto ({}): {} / {}", a.level, a.title, a.item), "sistema").await;
        publish(state);
    }
}

fn finance_money(n: f64) -> String {
    format!("${}", group_thousands(n.round() as i64))
}

fn group_thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{}", out)
    } else {
        out
    }
}

/// Tarea de fondo del modulo "finanzas": cada ~15 minutos revisa los umbrales de presupuesto
pub async fn alert_loop(state: AppState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
    loop {
        tick.tick().await;
        run_alerts(&state).await;
    }
}

// ─── Bienes por proyecto (idea 59) ───

#[derive(Deserialize)]
struct AssetsQuery {
    project_id: Option<String>,
}

#[derive(Serialize)]
struct AssetRow {
    id: String,
    name: String,
    project_id: String,
    inventory_number: String,
    responsible: String,
    location: String,
    status: String,
    quantity: f64,
    unit_cost: f64,
    total_cost: f64,
    purchase_date: String,
}

#[derive(Serialize)]
struct UsageRow {
    machine: String,
    sessions: i64,
    hours: f64,
}

#[derive(Serialize)]
struct Assets {
    items: Vec<AssetRow>,
    total_cost: f64,
    /// Horas de uso de maquinas cuya sesion declaro este proyecto por nombre
    usage: Vec<UsageRow>,
    usage_hours: f64,
}

/// GET /api/finance/assets?project_id= — bienes del inventario asociados al proyecto y horas
/// de uso de maquinas (las sesiones de uso guardan el nombre del proyecto como texto; no hay
/// vinculo entre maquinas e items de inventario)
async fn assets(State(state): State<AppState>, Query(q): Query<AssetsQuery>) -> Result<Json<Assets>, ApiError> {
    db_op_status(&state.db, move |conn| {
        let pid = q.project_id.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        let mut sql = "SELECT id, name, COALESCE(project_id, ''), inventory_number, responsible, location, status, quantity, cost, purchase_date
                       FROM inventory_items WHERE project_id IS NOT NULL AND project_id != ''".to_string();
        if pid.is_some() {
            sql.push_str(" AND project_id = ?1");
        }
        sql.push_str(" ORDER BY name LIMIT 2000");
        let mut stmt = conn.prepare(&sql).map_err(internal)?;
        let map = |r: &rusqlite::Row| -> rusqlite::Result<AssetRow> {
            let (quantity, unit_cost): (f64, f64) = (r.get(7)?, r.get(8)?);
            Ok(AssetRow {
                id: r.get(0)?,
                name: r.get(1)?,
                project_id: r.get(2)?,
                inventory_number: r.get(3)?,
                responsible: r.get(4)?,
                location: r.get(5)?,
                status: r.get(6)?,
                quantity,
                unit_cost,
                total_cost: quantity * unit_cost,
                purchase_date: r.get(9)?,
            })
        };
        let items = match &pid {
            Some(p) => stmt.query_map([p], map),
            None => stmt.query_map([], map),
        }
        .map_err(internal)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(internal)?;
        let total_cost = items.iter().map(|i| i.total_cost).sum();
        let (usage, usage_hours) = match &pid {
            Some(p) => {
                let title = finance::project_title(conn, p).map_err(internal)?.unwrap_or_default();
                let mut stmt = conn
                    .prepare(
                        "SELECT COALESCE(m.name, u.machine_id), COUNT(*),
                                COALESCE(SUM((julianday(u.ended_at) - julianday(u.started_at)) * 24), 0)
                         FROM machine_usage u LEFT JOIN machines m ON m.id = u.machine_id
                         WHERE u.ended_at IS NOT NULL AND lower(trim(u.project)) = lower(trim(?1)) AND ?1 != ''
                         GROUP BY u.machine_id ORDER BY 3 DESC",
                    )
                    .map_err(internal)?;
                let rows = stmt
                    .query_map([&title], |r| Ok(UsageRow { machine: r.get(0)?, sessions: r.get(1)?, hours: (r.get::<_, f64>(2)? * 10.0).round() / 10.0 }))
                    .map_err(internal)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(internal)?;
                let total = rows.iter().map(|u| u.hours).sum();
                (rows, total)
            }
            None => (Vec::new(), 0.0),
        };
        Ok(Assets { items, total_cost, usage, usage_hours })
    })
    .await
    .map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE portfolio_entries (id TEXT PRIMARY KEY, title TEXT NOT NULL);
             CREATE TABLE fin_projects (project_id TEXT PRIMARY KEY, currency TEXT NOT NULL DEFAULT 'CLP', responsible TEXT NOT NULL DEFAULT '',
                alert_pct REAL NOT NULL DEFAULT 80, closed_at TEXT, created_by TEXT NOT NULL, created_at TEXT NOT NULL);
             CREATE TABLE fin_budget_items (id TEXT PRIMARY KEY, project_id TEXT NOT NULL, category TEXT NOT NULL, name TEXT NOT NULL,
                approved REAL NOT NULL DEFAULT 0, notes TEXT NOT NULL DEFAULT '', created_by TEXT NOT NULL, created_at TEXT NOT NULL);
             CREATE TABLE fin_commitments (source TEXT NOT NULL, source_id TEXT NOT NULL, project_id TEXT NOT NULL, budget_item_id TEXT,
                label TEXT NOT NULL DEFAULT '', committed REAL NOT NULL DEFAULT 0, spent REAL NOT NULL DEFAULT 0, updated_at TEXT NOT NULL,
                PRIMARY KEY (source, source_id));
             INSERT INTO portfolio_entries VALUES ('p1', 'Proyecto uno');
             INSERT INTO fin_budget_items VALUES ('i1', 'p1', 'equipos', 'Cortadora', 1000, '', 'a', 'x');",
        )
        .unwrap();
        conn.execute_batch(crate::db::MIGRATION_PRESUPUESTO).unwrap();
        conn
    }

    fn use_amount(conn: &Connection, committed: f64, spent: f64) {
        finance::set_commitment(conn, "compra", "c1", "p1", Some("i1"), "Compra", committed, spent).unwrap();
    }

    #[test]
    fn niveles() {
        assert_eq!(level_of(1000.0, 799.0, 80.0), "ok");
        assert_eq!(level_of(1000.0, 800.0, 80.0), "alerta");
        assert_eq!(level_of(1000.0, 1000.0, 80.0), "tope");
        assert_eq!(level_of(0.0, 0.0, 80.0), "ok");
        assert_eq!(level_of(0.0, 5.0, 80.0), "tope");
    }

    #[test]
    fn miles_con_punto() {
        assert_eq!(finance_money(1234567.0), "$1.234.567");
        assert_eq!(finance_money(999.0), "$999");
    }

    #[test]
    fn aviso_una_sola_vez_por_umbral_y_se_rearma() {
        let conn = db();
        assert!(pending_alerts(&conn).unwrap().is_empty());

        use_amount(&conn, 850.0, 0.0);
        let a = pending_alerts(&conn).unwrap();
        assert_eq!((a.len(), a[0].level), (1, "alerta"));
        assert_eq!(pending_alerts(&conn).unwrap().len(), 0, "no repite el aviso");

        // Pasa el 100%: avisa el tope, una sola vez
        use_amount(&conn, 400.0, 700.0);
        let a = pending_alerts(&conn).unwrap();
        assert_eq!((a.len(), a[0].level), (1, "tope"));
        assert!(pending_alerts(&conn).unwrap().is_empty());

        // Baja a normal: se rearma y vuelve a avisar al subir
        use_amount(&conn, 100.0, 0.0);
        assert!(pending_alerts(&conn).unwrap().is_empty());
        use_amount(&conn, 900.0, 0.0);
        assert_eq!(pending_alerts(&conn).unwrap()[0].level, "alerta");

        // Salto directo de normal a 100%: un solo aviso, el del tope
        use_amount(&conn, 0.0, 0.0);
        pending_alerts(&conn).unwrap();
        use_amount(&conn, 0.0, 1200.0);
        let a = pending_alerts(&conn).unwrap();
        assert_eq!((a.len(), a[0].level), (1, "tope"));
    }

    #[test]
    fn proyecto_cerrado_no_avisa() {
        let conn = db();
        conn.execute("INSERT INTO fin_projects VALUES ('p1', 'CLP', '', 80, '2026-01-01', 'a', 'x')", []).unwrap();
        use_amount(&conn, 5000.0, 0.0);
        assert!(pending_alerts(&conn).unwrap().is_empty());
    }

    #[test]
    fn estado_de_un_item() {
        let conn = db();
        use_amount(&conn, 300.0, 200.0);
        let s = item_status(&conn, "i1").unwrap().unwrap();
        assert_eq!((s.available, s.pct, s.level), (500.0, Some(50.0), "ok"));
        assert!(item_status(&conn, "nope").unwrap().is_none());
    }
}
