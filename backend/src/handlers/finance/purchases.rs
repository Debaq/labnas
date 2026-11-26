//! Flujo de compra con funciones separadas, cotizaciones, facturas, recepcion conforme, aprobacion con passkey y conflicto de interes (ideas 53-56, 58, 69). Ver docs/FINANZAS.md.
//!
//! Estados que no se saltan: solicitud → cotizacion → aprobada → orden → recibida → facturada →
//! pagada → rendida (mas rechazada y anulada, con motivo). Quien pide no aprueba; quien recibe
//! no es quien aprobo ni quien pidio. Cada paso deja una fila en `fin_purchase_steps`, un
//! registro en la bitacora inalterable y mueve lo comprometido/gastado del item de presupuesto.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Extension, Json, Router,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::{db_op, db_op_status};
use crate::error::{internal, ApiError};
use crate::events::{Audience, Level};
use crate::finance::{self, FileRef};
use crate::handlers::machines::{bad, conflict, forbidden, md};
use crate::roles::Level as Lv;
use crate::state::{AppState, SessionInfo};

#[path = "purchases_dte.rs"]
pub mod dte;
#[path = "purchases_passkey.rs"]
pub mod passkey;

const MODULE: Option<&str> = Some("finanzas");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/finance/purchases", get(list).post(create))
        .route("/api/finance/purchases/settings", get(get_settings).put(put_settings))
        .route("/api/finance/purchases/items", get(budget_items))
        .route("/api/finance/purchases/{id}", get(detail))
        .route("/api/finance/purchases/{id}/quoting", post(start_quoting))
        .route("/api/finance/purchases/{id}/quotes", post(add_quote))
        .route("/api/finance/purchases/{id}/passkey/options", post(passkey_options))
        .route("/api/finance/purchases/{id}/approve", post(approve))
        .route("/api/finance/purchases/{id}/reject", post(reject))
        .route("/api/finance/purchases/{id}/cancel", post(cancel))
        .route("/api/finance/purchases/{id}/order", post(order))
        .route("/api/finance/purchases/{id}/receive", post(receive))
        .route("/api/finance/purchases/{id}/invoice", post(upload_invoice))
        .route("/api/finance/purchases/{id}/invoice/{doc}/confirm", post(confirm_invoice))
        .route("/api/finance/purchases/{id}/pay", post(pay))
        .route("/api/finance/purchases/{id}/settle", post(settle))
}

// ───────────────────────── Tipos ─────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Purchase {
    pub id: String,
    pub folio: i64,
    pub project_id: String,
    pub project_title: Option<String>,
    pub budget_item_id: Option<String>,
    pub budget_item_name: Option<String>,
    pub title: String,
    pub justification: String,
    pub estimated_amount: f64,
    pub final_amount: Option<f64>,
    pub status: String,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub received_by: Option<String>,
    pub chosen_quote_id: Option<String>,
    pub choice_reason: String,
    pub created_at: String,
    pub updated_at: String,
    pub quotes_count: i64,
    pub chosen_supplier: Option<String>,
}

const SELECT: &str = "SELECT p.id, p.folio, p.project_id, pe.title, p.budget_item_id, bi.name, p.title, p.justification, p.estimated_amount,
        p.final_amount, p.status, p.requested_by, p.approved_by, p.received_by, p.chosen_quote_id, p.choice_reason, p.created_at, p.updated_at,
        (SELECT COUNT(*) FROM fin_quotes q WHERE q.purchase_id = p.id),
        (SELECT supplier FROM fin_quotes q WHERE q.id = p.chosen_quote_id)
    FROM fin_purchases p
    LEFT JOIN portfolio_entries pe ON pe.id = p.project_id
    LEFT JOIN fin_budget_items bi ON bi.id = p.budget_item_id";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Purchase> {
    Ok(Purchase {
        id: r.get(0)?,
        folio: r.get(1)?,
        project_id: r.get(2)?,
        project_title: r.get(3)?,
        budget_item_id: r.get(4)?,
        budget_item_name: r.get(5)?,
        title: r.get(6)?,
        justification: r.get(7)?,
        estimated_amount: r.get(8)?,
        final_amount: r.get(9)?,
        status: r.get(10)?,
        requested_by: r.get(11)?,
        approved_by: r.get(12)?,
        received_by: r.get(13)?,
        chosen_quote_id: r.get(14)?,
        choice_reason: r.get(15)?,
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
        quotes_count: r.get(18)?,
        chosen_supplier: r.get(19)?,
    })
}

fn load(conn: &Connection, id: &str) -> Result<Purchase, ApiError> {
    conn.query_row(&format!("{} WHERE p.id = ?1", SELECT), [id], row)
        .optional()
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Compra no encontrada".to_string()))
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub id: String,
    pub step: String,
    pub username: String,
    pub at: String,
    pub note: String,
    pub passkey: bool,
    pub ledger_seq: Option<i64>,
    pub ledger_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Quote {
    pub id: String,
    pub supplier: String,
    pub supplier_rut: String,
    pub amount: f64,
    pub valid_until: Option<String>,
    pub file: Option<FileRef>,
    pub notes: String,
    pub created_by: String,
    pub created_at: String,
    pub cheapest: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Doc {
    pub id: String,
    pub kind: String,
    pub folio: String,
    pub issuer_rut: String,
    pub issuer_name: String,
    pub issued_on: Option<String>,
    pub net_amount: Option<f64>,
    pub tax_amount: Option<f64>,
    pub total_amount: Option<f64>,
    pub file: Option<FileRef>,
    pub extracted_by: String,
    pub confirmed_by: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reception {
    pub id: String,
    pub received_by: String,
    pub received_at: String,
    pub conforming: bool,
    pub notes: String,
    pub photo: Option<FileRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Declaration {
    pub id: String,
    pub username: String,
    pub supplier: String,
    pub has_conflict: bool,
    pub detail: String,
    pub at: String,
    pub ledger_seq: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Availability {
    pub item_name: String,
    pub approved: f64,
    pub committed: f64,
    pub spent: f64,
    pub available: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Cotizaciones minimas sobre el monto umbral
    pub min_quotes: i64,
    /// Monto (pesos) desde el cual rige el minimo
    pub quote_threshold: f64,
    /// Exigir passkey para aprobar
    pub require_passkey: bool,
}

#[derive(Debug, Serialize)]
pub struct Detail {
    pub purchase: Purchase,
    pub steps: Vec<Step>,
    pub quotes: Vec<Quote>,
    pub documents: Vec<Doc>,
    pub receptions: Vec<Reception>,
    pub declarations: Vec<Declaration>,
    pub settings: Settings,
    /// Cotizaciones que se exigen para el monto estimado
    pub required_quotes: i64,
    pub budget: Option<Availability>,
    pub warnings: Vec<String>,
}

// ───────────────────────── Ajustes ─────────────────────────

fn setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM fin_purchase_settings WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
}

pub fn load_settings(conn: &Connection) -> Settings {
    Settings {
        min_quotes: setting(conn, "min_quotes").and_then(|v| v.parse().ok()).unwrap_or(3),
        quote_threshold: setting(conn, "quote_threshold").and_then(|v| v.parse().ok()).unwrap_or(1_000_000.0),
        require_passkey: setting(conn, "require_passkey").is_some_and(|v| v == "1"),
    }
}

fn required_quotes(s: &Settings, amount: f64) -> i64 {
    if amount >= s.quote_threshold {
        s.min_quotes
    } else {
        0
    }
}

/// GET /api/finance/purchases/settings
pub async fn get_settings(State(state): State<AppState>) -> Result<Json<Settings>, ApiError> {
    db_op(&state.db, |conn| Ok(load_settings(conn))).await.map(Json)
}

/// PUT /api/finance/purchases/settings (quien gestiona Finanzas)
pub async fn put_settings(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<Settings>,
) -> Result<Json<Settings>, ApiError> {
    if !session.can("finanzas", Lv::Gestionar) {
        return Err(forbidden("Solo quien gestiona Finanzas cambia estos ajustes"));
    }
    if !(1..=10).contains(&req.min_quotes) || !(0.0..1e10).contains(&req.quote_threshold) {
        return Err(bad("Cotizaciones minimas entre 1 y 10, y monto umbral entre 0 y 10.000.000.000"));
    }
    let s = Settings { quote_threshold: req.quote_threshold.round(), ..req };
    let s2 = s.clone();
    db_op(&state.db, move |conn| {
        for (k, v) in [
            ("min_quotes", s2.min_quotes.to_string()),
            ("quote_threshold", s2.quote_threshold.to_string()),
            ("require_passkey", (s2.require_passkey as i32).to_string()),
        ] {
            conn.execute(
                "INSERT INTO fin_purchase_settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
                params![k, v],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await?;
    finance::ledger(
        &state,
        &session.username,
        "compra",
        "ajustes de compras",
        json!({ "min_quotes": s.min_quotes, "quote_threshold": s.quote_threshold, "require_passkey": s.require_passkey }),
    )
    .await?;
    state.log_activity("Finanzas", "Cambio los ajustes de compras", &session.username).await;
    Ok(Json(s))
}

// ───────────────────────── Lectura ─────────────────────────

fn availability(conn: &Connection, item_id: &str, exclude_purchase: &str) -> Option<Availability> {
    let (name, approved): (String, f64) =
        conn.query_row("SELECT name, approved FROM fin_budget_items WHERE id = ?1", [item_id], |r| Ok((r.get(0)?, r.get(1)?))).optional().ok()??;
    let (committed, spent): (f64, f64) = conn
        .query_row(
            "SELECT COALESCE(SUM(committed), 0), COALESCE(SUM(spent), 0) FROM fin_commitments
             WHERE budget_item_id = ?1 AND NOT (source = 'compra' AND source_id = ?2)",
            params![item_id, exclude_purchase],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()?;
    Some(Availability { item_name: name, approved, committed, spent, available: approved - committed - spent })
}

fn file_of(conn: &Connection, id: Option<String>) -> Option<FileRef> {
    id.and_then(|i| finance::file_ref(conn, &i).ok().flatten())
}

pub fn load_detail(conn: &Connection, id: &str) -> Result<Detail, ApiError> {
    let purchase = load(conn, id)?;
    let q = |e: rusqlite::Error| internal(e);

    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.step, s.username, s.at, s.note, s.passkey, s.ledger_seq, l.hash
             FROM fin_purchase_steps s LEFT JOIN ledger_entries l ON l.seq = s.ledger_seq
             WHERE s.purchase_id = ?1 ORDER BY s.at, s.rowid",
        )
        .map_err(q)?;
    let steps = stmt
        .query_map([id], |r| {
            Ok(Step { id: r.get(0)?, step: r.get(1)?, username: r.get(2)?, at: r.get(3)?, note: r.get(4)?, passkey: r.get::<_, i64>(5)? != 0, ledger_seq: r.get(6)?, ledger_hash: r.get(7)? })
        })
        .map_err(q)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(q)?;

    let mut stmt = conn
        .prepare("SELECT id, supplier, supplier_rut, amount, valid_until, file_id, notes, created_by, created_at FROM fin_quotes WHERE purchase_id = ?1 ORDER BY amount, created_at")
        .map_err(q)?;
    let raw: Vec<(Quote, Option<String>)> = stmt
        .query_map([id], |r| {
            Ok((
                Quote { id: r.get(0)?, supplier: r.get(1)?, supplier_rut: r.get(2)?, amount: r.get(3)?, valid_until: r.get(4)?, file: None, notes: r.get(6)?, created_by: r.get(7)?, created_at: r.get(8)?, cheapest: false },
                r.get(5)?,
            ))
        })
        .map_err(q)?
        .collect::<rusqlite::Result<_>>()
        .map_err(q)?;
    let min = raw.iter().map(|(q, _)| q.amount).fold(f64::MAX, f64::min);
    let quotes: Vec<Quote> = raw.into_iter().map(|(mut qu, f)| {
        qu.file = file_of(conn, f);
        qu.cheapest = (qu.amount - min).abs() < 0.005;
        qu
    }).collect();

    let mut stmt = conn
        .prepare("SELECT id, kind, folio, issuer_rut, issuer_name, issued_on, net_amount, tax_amount, total_amount, file_id, extracted_by, confirmed_by, created_by, created_at FROM fin_documents WHERE purchase_id = ?1 ORDER BY created_at")
        .map_err(q)?;
    let raw: Vec<(Doc, Option<String>)> = stmt
        .query_map([id], |r| {
            Ok((
                Doc { id: r.get(0)?, kind: r.get(1)?, folio: r.get(2)?, issuer_rut: r.get(3)?, issuer_name: r.get(4)?, issued_on: r.get(5)?, net_amount: r.get(6)?, tax_amount: r.get(7)?, total_amount: r.get(8)?, file: None, extracted_by: r.get(10)?, confirmed_by: r.get(11)?, created_by: r.get(12)?, created_at: r.get(13)? },
                r.get(9)?,
            ))
        })
        .map_err(q)?
        .collect::<rusqlite::Result<_>>()
        .map_err(q)?;
    let documents = raw.into_iter().map(|(mut d, f)| { d.file = file_of(conn, f); d }).collect();

    let mut stmt = conn
        .prepare("SELECT id, received_by, received_at, conforming, notes, photo_file_id FROM fin_receptions WHERE purchase_id = ?1 ORDER BY received_at")
        .map_err(q)?;
    let raw: Vec<(Reception, Option<String>)> = stmt
        .query_map([id], |r| {
            Ok((Reception { id: r.get(0)?, received_by: r.get(1)?, received_at: r.get(2)?, conforming: r.get::<_, i64>(3)? != 0, notes: r.get(4)?, photo: None }, r.get(5)?))
        })
        .map_err(q)?
        .collect::<rusqlite::Result<_>>()
        .map_err(q)?;
    let receptions = raw.into_iter().map(|(mut x, f)| { x.photo = file_of(conn, f); x }).collect();

    let mut stmt = conn
        .prepare("SELECT id, username, supplier, has_conflict, detail, at, ledger_seq FROM fin_conflict_declarations WHERE purchase_id = ?1 ORDER BY at")
        .map_err(q)?;
    let declarations = stmt
        .query_map([id], |r| {
            Ok(Declaration { id: r.get(0)?, username: r.get(1)?, supplier: r.get(2)?, has_conflict: r.get::<_, i64>(3)? != 0, detail: r.get(4)?, at: r.get(5)?, ledger_seq: r.get(6)? })
        })
        .map_err(q)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(q)?;

    let settings = load_settings(conn);
    let budget = purchase.budget_item_id.as_deref().and_then(|i| availability(conn, i, id));
    let mut warnings = Vec::new();
    if let (Some(b), "solicitud" | "cotizacion") = (&budget, purchase.status.as_str()) {
        if purchase.estimated_amount > b.available {
            warnings.push(format!(
                "El monto estimado ({}) supera lo disponible del item «{}» ({}): aprobado {} menos comprometido {} y gastado {}",
                money(purchase.estimated_amount), b.item_name, money(b.available), money(b.approved), money(b.committed), money(b.spent)
            ));
        }
    }
    let required = required_quotes(&settings, purchase.estimated_amount);
    Ok(Detail { purchase, steps, quotes, documents, receptions, declarations, settings, required_quotes: required, budget, warnings })
}

fn money(n: f64) -> String {
    let s = (n.round() as i64).abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    format!("{}${}", if n < 0.0 { "-" } else { "" }, out)
}

#[derive(Deserialize)]
pub struct ItemsQuery {
    pub project: String,
}

/// GET /api/finance/purchases/items?project= — items de presupuesto del proyecto con lo disponible
/// (aprobado − comprometido − gastado), para elegir a cual se carga una compra
pub async fn budget_items(State(state): State<AppState>, Query(q): Query<ItemsQuery>) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    db_op(&state.db, move |conn| {
        let mut stmt = conn.prepare("SELECT id FROM fin_budget_items WHERE project_id = ?1 ORDER BY category, name").map_err(|e| e.to_string())?;
        let ids: Vec<String> = stmt.query_map([&q.project], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())?;
        Ok(ids
            .iter()
            .filter_map(|i| availability(conn, i, "").map(|a| json!({ "id": i, "name": a.item_name, "approved": a.approved, "available": a.available })))
            .collect())
    })
    .await
    .map(Json)
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    pub project: Option<String>,
    pub status: Option<String>,
}

/// GET /api/finance/purchases?project=&status=
pub async fn list(State(state): State<AppState>, Query(q): Query<ListQuery>) -> Result<Json<Vec<Purchase>>, ApiError> {
    db_op(&state.db, move |conn| {
        let mut sql = format!("{} WHERE 1 = 1", SELECT);
        let mut args: Vec<String> = Vec::new();
        for (col, v) in [("p.project_id", &q.project), ("p.status", &q.status)] {
            if let Some(v) = v.as_ref().map(|v| v.trim()).filter(|v| !v.is_empty()) {
                args.push(v.to_string());
                sql.push_str(&format!(" AND {} = ?{}", col, args.len()));
            }
        }
        sql.push_str(" ORDER BY p.folio DESC LIMIT 1000");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row).map_err(|e| e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
        Ok(rows)
    })
    .await
    .map(Json)
}

/// GET /api/finance/purchases/{id}: ficha completa (tambien sirve de base para el acta)
pub async fn detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Detail>, ApiError> {
    db_op_status(&state.db, move |conn| load_detail(conn, &id)).await.map(Json)
}

async fn reload(state: &AppState, id: &str) -> Result<Json<Detail>, ApiError> {
    let id = id.to_string();
    db_op_status(&state.db, move |conn| load_detail(conn, &id)).await.map(Json)
}

// ───────────────────────── Piezas de cada paso ─────────────────────────

fn staff(session: &SessionInfo) -> bool {
    session.can("finanzas", Lv::Gestionar)
}

fn today() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn valid_date(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

fn text(s: &str, max: usize, what: &str) -> Result<String, ApiError> {
    let s = s.trim().to_string();
    if s.chars().count() > max {
        return Err(bad(format!("{} demasiado largo (maximo {} caracteres)", what, max)));
    }
    Ok(s)
}

fn reason(s: &str, what: &str) -> Result<String, ApiError> {
    let s = text(s, 1000, what)?;
    if s.chars().count() < 5 {
        return Err(bad(format!("{}: explica brevemente (minimo 5 caracteres)", what)));
    }
    Ok(s)
}

fn pesos(v: f64, what: &str) -> Result<f64, ApiError> {
    if !v.is_finite() || v <= 0.0 || v >= 1e10 {
        return Err(bad(format!("{} invalido: debe ser mayor que 0", what)));
    }
    Ok(v.round())
}

fn rut(s: &str) -> Result<String, ApiError> {
    let r: String = s.chars().filter(|c| !matches!(c, '.' | ' ')).collect::<String>().to_uppercase();
    if r.chars().count() > 12 || r.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '-')) {
        return Err(bad("RUT invalido (ej. 76123456-7)"));
    }
    Ok(r)
}

fn add_step(conn: &Connection, purchase_id: &str, step: &str, user: &str, note: &str, passkey: bool) -> Result<String, ApiError> {
    let id = finance::new_id();
    conn.execute(
        "INSERT INTO fin_purchase_steps (id, purchase_id, step, username, at, note, passkey) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, purchase_id, step, user, finance::now(), note, passkey as i64],
    )
    .map_err(internal)?;
    conn.execute("UPDATE fin_purchases SET updated_at = ?2 WHERE id = ?1", params![purchase_id, finance::now()]).map_err(internal)?;
    Ok(id)
}

fn staff_users(conn: &Connection) -> Vec<String> {
    crate::roles::users_with_level(conn, "finanzas", Lv::Gestionar).unwrap_or_default()
}

fn commit(conn: &Connection, p: &Purchase, committed: f64, spent: f64) -> Result<(), ApiError> {
    finance::set_commitment(conn, "compra", &p.id, &p.project_id, p.budget_item_id.as_deref(), &format!("Compra #{} {}", p.folio, p.title), committed, spent)
        .map_err(internal)
}

/// Lo que va a la bitacora y a quienes se avisa
struct Seal<'a> {
    user: &'a str,
    id: &'a str,
    folio: i64,
    step_id: String,
    step: &'a str,
    note: String,
    extra: serde_json::Value,
    /// Declaracion de conflicto que queda atada a este registro
    decl_id: Option<String>,
    /// (destinatarios, texto)
    tell: Option<(Vec<String>, String)>,
}

/// Bitacora inalterable + actividad + aviso + evento en vivo. Devuelve el numero de registro.
async fn seal(state: &AppState, s: Seal<'_>) -> Result<i64, ApiError> {
    let mut data = json!({ "compra": s.id, "folio": s.folio, "paso": s.step, "nota": s.note });
    if let (Some(d), Some(e)) = (data.as_object_mut(), s.extra.as_object()) {
        d.extend(e.clone());
    }
    let seq = finance::ledger(state, s.user, "compra", &format!("compra #{}", s.folio), data).await?;
    let (step_id, decl_id) = (s.step_id.clone(), s.decl_id.clone());
    db_op(&state.db, move |conn| {
        conn.execute("UPDATE fin_purchase_steps SET ledger_seq = ?2 WHERE id = ?1", params![step_id, seq]).map_err(|e| e.to_string())?;
        if let Some(d) = decl_id {
            conn.execute("UPDATE fin_conflict_declarations SET ledger_seq = ?2 WHERE id = ?1", params![d, seq]).map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await?;
    state.log_activity("Finanzas", &format!("Compra #{} ({}): {}", s.folio, s.step, s.note), s.user).await;
    state.events.publish("finance", (), Audience::All, MODULE);
    if let Some((users, msg)) = s.tell {
        let users: Vec<String> = users.into_iter().filter(|u| u != s.user).collect();
        for u in &users {
            crate::events::notify(state, Audience::User(u.clone()), MODULE, Level::Info, "Compras", &msg);
        }
        let st = state.clone();
        let tg = format!("*Compras*\n{}", md(&msg));
        tokio::spawn(async move {
            crate::handlers::notifications::notify_users(&st, &users, &tg).await;
        });
    }
    Ok(seq)
}

/// Pasa la compra de `from` a `to` solo si sigue en ese estado (evita carreras y saltos)
fn transition(conn: &Connection, id: &str, from: &[&str], to: &str) -> Result<(), ApiError> {
    let st: String = conn.query_row("SELECT status FROM fin_purchases WHERE id = ?1", [id], |r| r.get(0)).map_err(internal)?;
    if !from.contains(&st.as_str()) {
        return Err(conflict(format!(
            "La compra esta en «{}»: este paso no corresponde (no se pueden saltar pasos)",
            st
        )));
    }
    let n = conn
        .execute("UPDATE fin_purchases SET status = ?3, updated_at = ?4 WHERE id = ?1 AND status = ?2", params![id, st, to, finance::now()])
        .map_err(internal)?;
    if n == 0 {
        return Err(conflict("La compra cambio mientras la editabas: vuelve a abrirla"));
    }
    Ok(())
}

// ───────────────────────── Solicitud ─────────────────────────

#[derive(Deserialize)]
pub struct CreateReq {
    pub project_id: String,
    pub budget_item_id: Option<String>,
    pub title: String,
    pub justification: String,
    pub estimated_amount: f64,
}

/// POST /api/finance/purchases (nivel Usar): crea la solicitud
pub async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<CreateReq>,
) -> Result<(StatusCode, Json<Detail>), ApiError> {
    let title = text(&req.title, 200, "Titulo")?;
    if title.is_empty() {
        return Err(bad("Indica que se quiere comprar"));
    }
    let justification = text(&req.justification, 2000, "Justificacion")?;
    if justification.chars().count() < 10 {
        return Err(bad("La justificacion es obligatoria: explica para que se necesita (minimo 10 caracteres)"));
    }
    let amount = pesos(req.estimated_amount, "Monto estimado")?;
    let user = session.username.clone();
    let item = req.budget_item_id.filter(|i| !i.trim().is_empty());
    let project_id = req.project_id.trim().to_string();
    let (u, title_c, just_c) = (user.clone(), title.clone(), justification.clone());
    let (id, folio, staff_list) = db_op_status(&state.db, move |conn| {
        if finance::project_title(conn, &project_id).map_err(internal)?.is_none() {
            return Err(bad("El proyecto no existe en el Portafolio"));
        }
        let closed: Option<Option<String>> = conn
            .query_row("SELECT closed_at FROM fin_projects WHERE project_id = ?1", [&project_id], |r| r.get(0))
            .optional()
            .map_err(internal)?;
        if closed.flatten().is_some() {
            return Err(conflict("El proyecto esta cerrado: no admite compras nuevas"));
        }
        if let Some(i) = &item {
            let ok: Option<i64> = conn
                .query_row("SELECT 1 FROM fin_budget_items WHERE id = ?1 AND project_id = ?2", params![i, project_id], |r| r.get(0))
                .optional()
                .map_err(internal)?;
            if ok.is_none() {
                return Err(bad("El item de presupuesto no es de ese proyecto"));
            }
        }
        let id = finance::new_id();
        let now = finance::now();
        conn.execute(
            "INSERT INTO fin_purchases (id, folio, project_id, budget_item_id, title, justification, estimated_amount, status, requested_by, created_at, updated_at)
             VALUES (?1, (SELECT COALESCE(MAX(folio), 0) + 1 FROM fin_purchases), ?2, ?3, ?4, ?5, ?6, 'solicitud', ?7, ?8, ?8)",
            params![id, project_id, item, title_c, just_c, amount, u, now],
        )
        .map_err(internal)?;
        let folio: i64 = conn.query_row("SELECT folio FROM fin_purchases WHERE id = ?1", [&id], |r| r.get(0)).map_err(internal)?;
        Ok((id, folio, staff_users(conn)))
    })
    .await?;
    let step_id = {
        let (id, u) = (id.clone(), user.clone());
        let j = justification.clone();
        db_op_status(&state.db, move |conn| add_step(conn, &id, "solicitud", &u, &j, false)).await?
    };
    seal(
        &state,
        Seal {
            user: &user,
            id: &id,
            folio,
            step_id,
            step: "solicitud",
            note: format!("Solicita «{}» por {}", title, money(amount)),
            extra: json!({ "titulo": title, "monto_estimado": amount, "proyecto": req.project_id, "justificacion": justification }),
            decl_id: None,
            tell: Some((staff_list, format!("Nueva solicitud de compra #{}: {} ({}) de {}", folio, title, money(amount), user))),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, reload(&state, &id).await?))
}

// ───────────────────────── Cotizaciones ─────────────────────────

#[derive(Deserialize)]
pub struct FileIn {
    pub name: String,
    pub data: String,
}

fn may_edit(p: &Purchase, user: &str, staff: bool) -> bool {
    p.requested_by == user || staff
}

/// POST /api/finance/purchases/{id}/quoting: pasa de solicitud a cotizacion (sin cotizacion todavia)
pub async fn start_quoting(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<Detail>, ApiError> {
    let user = session.username.clone();
    let (u, i, s) = (user.clone(), id.clone(), staff(&session));
    let (p, step_id) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        if p.requested_by != u && !s {
            return Err(forbidden("Solo quien pidio la compra o quien gestiona Finanzas la pasa a cotizacion"));
        }
        transition(conn, &i, &["solicitud"], "cotizacion")?;
        let step = add_step(conn, &i, "cotizacion", &u, "Pasa a cotizacion", false)?;
        Ok((p, step))
    })
    .await?;
    seal(&state, Seal { user: &user, id: &id, folio: p.folio, step_id, step: "cotizacion", note: "Pasa a cotizacion".into(), extra: json!({}), decl_id: None, tell: None }).await?;
    reload(&state, &id).await
}

#[derive(Deserialize)]
pub struct QuoteReq {
    pub supplier: String,
    #[serde(default)]
    pub supplier_rut: String,
    pub amount: f64,
    pub valid_until: Option<String>,
    #[serde(default)]
    pub notes: String,
    pub file: FileIn,
}

/// POST /api/finance/purchases/{id}/quotes: agrega una cotizacion con su archivo
pub async fn add_quote(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<QuoteReq>,
) -> Result<Json<Detail>, ApiError> {
    let supplier = text(&req.supplier, 120, "Proveedor")?;
    if supplier.is_empty() {
        return Err(bad("Indica el proveedor"));
    }
    let supplier_rut = rut(&req.supplier_rut)?;
    let amount = pesos(req.amount, "Monto de la cotizacion")?;
    let notes = text(&req.notes, 1000, "Notas")?;
    let valid_until = req.valid_until.filter(|v| !v.trim().is_empty());
    if valid_until.as_deref().is_some_and(|v| !valid_date(v)) {
        return Err(bad("Fecha de validez invalida (AAAA-MM-DD)"));
    }
    let (bytes, mime) = finance::decode_upload(&req.file.data)?;
    let fname = req.file.name.clone();
    let user = session.username.clone();
    let (u, i, s) = (user.clone(), id.clone(), staff(&session));
    let sup = supplier.clone();
    let (p, step_id, file, ready, staff_list, first) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        if !may_edit(&p, &u, s) {
            return Err(forbidden("Solo quien pidio la compra o quien gestiona Finanzas agrega cotizaciones"));
        }
        if !["solicitud", "cotizacion"].contains(&p.status.as_str()) {
            return Err(conflict(format!("La compra esta en «{}»: ya no admite cotizaciones", p.status)));
        }
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM fin_quotes WHERE purchase_id = ?1", [&i], |r| r.get(0)).map_err(internal)?;
        if n >= 20 {
            return Err(bad("Maximo 20 cotizaciones por compra"));
        }
        let first = p.status == "solicitud";
        if first {
            transition(conn, &i, &["solicitud"], "cotizacion")?;
        }
        let file = finance::store_file(conn, &fname, &bytes, mime, &u).map_err(internal)?;
        conn.execute(
            "INSERT INTO fin_quotes (id, purchase_id, supplier, supplier_rut, amount, valid_until, file_id, notes, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![finance::new_id(), i, sup, supplier_rut, amount, valid_until, file.id, notes, u, finance::now()],
        )
        .map_err(internal)?;
        let step = add_step(conn, &i, "cotizacion", &u, &format!("Cotizacion de {} por {}", sup, money(amount)), false)?;
        let settings = load_settings(conn);
        let ready = n + 1 >= required_quotes(&settings, p.estimated_amount).max(1);
        Ok((p, step, file, ready, staff_users(conn), first))
    })
    .await?;
    seal(
        &state,
        Seal {
            user: &user,
            id: &id,
            folio: p.folio,
            step_id,
            step: "cotizacion",
            note: format!("Cotizacion de {} por {}", supplier, money(amount)),
            extra: json!({ "proveedor": supplier, "monto": amount, "archivo": file.name, "sha256": file.sha256, "primera": first }),
            decl_id: None,
            tell: ready.then(|| (staff_list, format!("La compra #{} ({}) ya tiene cotizaciones y esta lista para aprobar", p.folio, p.title))),
        },
    )
    .await?;
    reload(&state, &id).await
}

fn expect_status(p: &Purchase, from: &[&str]) -> Result<(), ApiError> {
    if from.contains(&p.status.as_str()) {
        Ok(())
    } else {
        Err(conflict(format!("La compra esta en «{}»: este paso no corresponde (no se pueden saltar pasos)", p.status)))
    }
}

fn tell_requester(p: &Purchase, msg: String) -> Option<(Vec<String>, String)> {
    Some((vec![p.requested_by.clone()], msg))
}

// ───────────────────────── Aprobacion ─────────────────────────

#[derive(Deserialize)]
pub struct ConflictIn {
    pub has_conflict: bool,
    #[serde(default)]
    pub detail: String,
}

#[derive(Deserialize)]
pub struct ApproveReq {
    pub quote_id: Option<String>,
    #[serde(default)]
    pub reason: String,
    pub conflict: Option<ConflictIn>,
    pub passkey: Option<passkey::Assertion>,
}

enum Pre {
    /// Declaro conflicto: queda registrado y no aprueba
    Conflict { step_id: String, decl_id: String, p: Purchase, supplier: String },
    Ready { p: Purchase, supplier: String, amount: f64, quote_sha: Option<String>, over: bool, require_passkey: bool, conflict: ConflictIn, reason: String },
}

fn chosen_quote(conn: &Connection, purchase_id: &str, quote_id: &str) -> Result<(String, f64, Option<String>), ApiError> {
    conn.query_row(
        "SELECT supplier, amount, (SELECT sha256 FROM fin_files f WHERE f.id = q.file_id) FROM fin_quotes q WHERE id = ?1 AND purchase_id = ?2",
        params![quote_id, purchase_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
    .map_err(internal)?
    .ok_or_else(|| bad("Esa cotizacion no es de esta compra"))
}

/// POST /api/finance/purchases/{id}/approve (Gestionar): elige cotizacion, declara conflicto y aprueba
pub async fn approve(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<ApproveReq>,
) -> Result<Json<Detail>, ApiError> {
    if !staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas aprueba compras"));
    }
    let user = session.username.clone();
    let (u, i, quote_id) = (user.clone(), id.clone(), req.quote_id.clone().filter(|q| !q.trim().is_empty()));
    let (conflict_in, reason_in) = (req.conflict, req.reason);
    let pre = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        expect_status(&p, &["cotizacion"])?;
        if p.requested_by == u {
            return Err(forbidden("Separacion de funciones: quien pidio la compra no puede aprobarla"));
        }
        let c = conflict_in.ok_or_else(|| bad("Antes de aprobar declara si tienes conflicto de interes con el proveedor"))?;
        let detail = text(&c.detail, 1000, "Detalle del conflicto")?;
        if c.has_conflict && detail.chars().count() < 3 {
            return Err(bad("Describe el conflicto de interes"));
        }
        let settings = load_settings(conn);
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM fin_quotes WHERE purchase_id = ?1", [&i], |r| r.get(0)).map_err(internal)?;
        let min: Option<f64> = conn.query_row("SELECT MIN(amount) FROM fin_quotes WHERE purchase_id = ?1", [&i], |r| r.get(0)).map_err(internal)?;
        let chosen = quote_id.as_deref().map(|q| chosen_quote(conn, &i, q)).transpose()?;
        let amount = chosen.as_ref().map(|c| c.1).unwrap_or(p.estimated_amount);
        let required = required_quotes(&settings, amount.max(p.estimated_amount));
        if n < required {
            return Err(conflict(format!(
                "Esta compra necesita al menos {} cotizaciones (monto desde {}) y tiene {}",
                required, money(settings.quote_threshold), n
            )));
        }
        if n > 0 && chosen.is_none() {
            return Err(bad("Elige la cotizacion que se acepta"));
        }
        let reason = text(&reason_in, 1000, "Razon de la eleccion")?;
        if let (Some(ch), Some(m)) = (&chosen, min) {
            if n > 1 && ch.1 > m + 0.005 && reason.chars().count() < 5 {
                return Err(bad("No elegiste la cotizacion mas barata: explica la razon de la eleccion"));
            }
        }
        let supplier = chosen.as_ref().map(|c| c.0.clone()).unwrap_or_else(|| "(sin cotizacion: monto estimado)".to_string());
        if c.has_conflict {
            conn.execute(
                "INSERT INTO fin_conflict_declarations (id, purchase_id, username, supplier, has_conflict, detail, at) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
                params![finance::new_id(), i, u, supplier, detail, finance::now()],
            )
            .map_err(internal)?;
            let decl_id: String = conn.query_row("SELECT id FROM fin_conflict_declarations WHERE purchase_id = ?1 ORDER BY rowid DESC LIMIT 1", [&i], |r| r.get(0)).map_err(internal)?;
            let step_id = add_step(conn, &i, "conflicto", &u, &format!("Declara conflicto de interes con {}: {}", supplier, detail), false)?;
            return Ok(Pre::Conflict { step_id, decl_id, p, supplier });
        }
        let over = p.budget_item_id.as_deref().and_then(|b| availability(conn, b, &i)).is_some_and(|a| amount > a.available);
        Ok(Pre::Ready { p, supplier, amount, quote_sha: chosen.and_then(|c| c.2), over, require_passkey: settings.require_passkey, conflict: ConflictIn { has_conflict: false, detail }, reason })
    })
    .await?;

    let (p, supplier, amount, quote_sha, over, require_passkey, conflict_in, reason) = match pre {
        Pre::Conflict { step_id, decl_id, p, supplier } => {
            seal(
                &state,
                Seal {
                    user: &user,
                    id: &id,
                    folio: p.folio,
                    step_id,
                    step: "conflicto",
                    note: format!("Declara conflicto de interes con {}: no aprueba", supplier),
                    extra: json!({ "proveedor": supplier, "conflicto": true }),
                    decl_id: Some(decl_id),
                    tell: Some((staff_users_of(&state).await, format!("{} declaro conflicto de interes en la compra #{} ({}): la debe aprobar otra persona", user, p.folio, p.title))),
                },
            )
            .await?;
            return Err(forbidden("Declaraste conflicto de interes con el proveedor: no puedes aprobar esta compra. Queda registrado; que la apruebe otra persona"));
        }
        Pre::Ready { p, supplier, amount, quote_sha, over, require_passkey, conflict, reason } => (p, supplier, amount, quote_sha, over, require_passkey, conflict, reason),
    };

    // Firma con passkey (obligatoria si el ajuste lo exige; si viene una, se verifica siempre)
    let signed = match req.passkey {
        Some(a) => {
            passkey::verify(&state, &user, &id, "aprobada", amount, &a).await?;
            true
        }
        None if require_passkey => return Err(bad("Esta compra se aprueba firmando con tu passkey")),
        None => false,
    };

    let (u, i, q2, r2, detail, sup2) = (user.clone(), id.clone(), req.quote_id.filter(|q| !q.trim().is_empty()), reason.clone(), conflict_in.detail.clone(), supplier.clone());
    let p2 = p.clone();
    let (step_id, decl_id) = db_op_status(&state.db, move |conn| {
        transition(conn, &i, &["cotizacion"], "aprobada")?;
        conn.execute(
            "UPDATE fin_purchases SET approved_by = ?2, chosen_quote_id = ?3, choice_reason = ?4 WHERE id = ?1",
            params![i, u, q2, r2],
        )
        .map_err(internal)?;
        let decl_id = finance::new_id();
        conn.execute(
            "INSERT INTO fin_conflict_declarations (id, purchase_id, username, supplier, has_conflict, detail, at) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            params![decl_id, i, u, sup2, detail, finance::now()],
        )
        .map_err(internal)?;
        commit(conn, &p2, amount, 0.0)?;
        let step = add_step(conn, &i, "aprobada", &u, &format!("Aprueba {} por {}{}", sup2, money(amount), if signed { " (firmada con passkey)" } else { "" }), signed)?;
        Ok((step, decl_id))
    })
    .await?;
    seal(
        &state,
        Seal {
            user: &user,
            id: &id,
            folio: p.folio,
            step_id,
            step: "aprobada",
            note: format!("Aprueba {} por {}", supplier, money(amount)),
            extra: json!({ "proveedor": supplier, "monto": amount, "razon_eleccion": reason, "conflicto": false, "passkey": signed, "sha256_cotizacion": quote_sha, "excede_disponible": over }),
            decl_id: Some(decl_id),
            tell: tell_requester(&p, format!("Tu compra #{} ({}) fue aprobada por {}", p.folio, p.title, user)),
        },
    )
    .await?;
    reload(&state, &id).await
}

async fn staff_users_of(state: &AppState) -> Vec<String> {
    db_op(&state.db, |conn| Ok(staff_users(conn))).await.unwrap_or_default()
}

#[derive(Deserialize, Default)]
pub struct PasskeyOptionsReq {
    pub quote_id: Option<String>,
}

/// POST /api/finance/purchases/{id}/passkey/options: desafio ligado a esta compra, paso y monto
pub async fn passkey_options(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Option<Json<PasskeyOptionsReq>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas aprueba compras"));
    }
    let quote_id = body.and_then(|Json(b)| b.quote_id).filter(|q| !q.trim().is_empty());
    let i = id.clone();
    let amount = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        expect_status(&p, &["cotizacion"])?;
        Ok(match quote_id {
            Some(q) => chosen_quote(conn, &i, &q)?.1,
            None => p.estimated_amount,
        })
    })
    .await?;
    passkey::options(&state, &headers, &session.username, &id, "aprobada", amount).await.map(Json)
}

#[derive(Deserialize)]
pub struct ReasonReq {
    #[serde(default)]
    pub reason: String,
}

/// POST /api/finance/purchases/{id}/reject (Gestionar, no quien pidio): rechaza con motivo
pub async fn reject(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<ReasonReq>,
) -> Result<Json<Detail>, ApiError> {
    if !staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas rechaza compras"));
    }
    let why = reason(&req.reason, "Motivo del rechazo")?;
    let user = session.username.clone();
    let (u, i, w) = (user.clone(), id.clone(), why.clone());
    let (p, step_id) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        expect_status(&p, &["solicitud", "cotizacion"])?;
        if p.requested_by == u {
            return Err(forbidden("Separacion de funciones: quien pidio la compra no la rechaza (puede anularla)"));
        }
        transition(conn, &i, &["solicitud", "cotizacion"], "rechazada")?;
        commit(conn, &p, 0.0, 0.0)?;
        let s = add_step(conn, &i, "rechazada", &u, &w, false)?;
        Ok((p, s))
    })
    .await?;
    seal(&state, Seal { user: &user, id: &id, folio: p.folio, step_id, step: "rechazada", note: why.clone(), extra: json!({ "motivo": why }), decl_id: None, tell: tell_requester(&p, format!("Tu compra #{} ({}) fue rechazada: {}", p.folio, p.title, why)) }).await?;
    reload(&state, &id).await
}

/// POST /api/finance/purchases/{id}/cancel: anula con motivo (quien pidio, solo hasta cotizar; despues, Gestionar)
pub async fn cancel(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<ReasonReq>,
) -> Result<Json<Detail>, ApiError> {
    let why = reason(&req.reason, "Motivo de la anulacion")?;
    let user = session.username.clone();
    let (u, i, w, s) = (user.clone(), id.clone(), why.clone(), staff(&session));
    let (p, step_id, staff_list) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        expect_status(&p, &["solicitud", "cotizacion", "aprobada", "orden", "recibida", "facturada"])?;
        let early = ["solicitud", "cotizacion"].contains(&p.status.as_str());
        if !s && !(early && p.requested_by == u) {
            return Err(forbidden("Quien pidio la compra solo la anula antes de aprobarse; despues, quien gestiona Finanzas"));
        }
        transition(conn, &i, &["solicitud", "cotizacion", "aprobada", "orden", "recibida", "facturada"], "anulada")?;
        commit(conn, &p, 0.0, 0.0)?;
        let st = add_step(conn, &i, "anulada", &u, &w, false)?;
        Ok((p, st, staff_users(conn)))
    })
    .await?;
    let mut to = vec![p.requested_by.clone()];
    to.extend(staff_list);
    seal(&state, Seal { user: &user, id: &id, folio: p.folio, step_id, step: "anulada", note: why.clone(), extra: json!({ "motivo": why, "estado_anterior": p.status }), decl_id: None, tell: Some((to, format!("La compra #{} ({}) fue anulada por {}: {}", p.folio, p.title, user, why))) }).await?;
    reload(&state, &id).await
}

// ───────────────────────── Orden, recepcion ─────────────────────────

#[derive(Deserialize, Default)]
pub struct NoteReq {
    #[serde(default)]
    pub note: String,
}

/// POST /api/finance/purchases/{id}/order (Gestionar): emite la orden de compra
pub async fn order(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    body: Option<Json<NoteReq>>,
) -> Result<Json<Detail>, ApiError> {
    if !staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas emite la orden de compra"));
    }
    let note = text(&body.map(|Json(b)| b.note).unwrap_or_default(), 500, "Nota")?;
    let user = session.username.clone();
    let (u, i, n) = (user.clone(), id.clone(), note.clone());
    let (p, step_id) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        transition(conn, &i, &["aprobada"], "orden")?;
        let s = add_step(conn, &i, "orden", &u, &n, false)?;
        Ok((p, s))
    })
    .await?;
    let msg = format!("Se emitio la orden de compra de #{} ({}){}", p.folio, p.title, if note.is_empty() { String::new() } else { format!(": {}", note) });
    seal(&state, Seal { user: &user, id: &id, folio: p.folio, step_id, step: "orden", note: note.clone(), extra: json!({ "orden": note }), decl_id: None, tell: tell_requester(&p, msg) }).await?;
    reload(&state, &id).await
}

#[derive(Deserialize)]
pub struct InventoryIn {
    pub name: Option<String>,
    pub quantity: Option<f64>,
}

#[derive(Deserialize)]
pub struct ReceiveReq {
    #[serde(default = "yes")]
    pub conforming: bool,
    #[serde(default)]
    pub notes: String,
    pub photo: Option<FileIn>,
    /// Crear el bien en Inventario
    pub inventory: Option<InventoryIn>,
}

fn yes() -> bool {
    true
}

const INVENTORY_CATEGORY: &str = "Bienes de proyectos";

/// Crea el bien recibido en Inventario (categoria «Bienes de proyectos»)
fn create_inventory_item(conn: &Connection, p: &Purchase, name: &str, quantity: f64, cost: f64, supplier: &str) -> Result<String, ApiError> {
    let cat: Option<String> = conn.query_row("SELECT id FROM inventory_categories WHERE name = ?1", [INVENTORY_CATEGORY], |r| r.get(0)).optional().map_err(internal)?;
    let cat = match cat {
        Some(c) => c,
        None => {
            let c = finance::new_id();
            conn.execute(
                "INSERT INTO inventory_categories (id, name, icon, description) VALUES (?1, ?2, 'Package', 'Bienes comprados con fondos de proyectos (Finanzas)')",
                params![c, INVENTORY_CATEGORY],
            )
            .map_err(internal)?;
            c
        }
    };
    let id = finance::new_id();
    conn.execute(
        "INSERT INTO inventory_items (id, category_id, name, quantity, cost, supplier, purchase_date, status, project_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'activo', ?8, ?9)",
        params![id, cat, name, quantity, cost, supplier, today(), p.project_id, chrono::Utc::now().to_rfc3339()],
    )
    .map_err(internal)?;
    Ok(id)
}

/// POST /api/finance/purchases/{id}/receive (Usar): recepcion conforme (o con observaciones).
/// Quien recibe no es quien aprobo ni quien pidio.
pub async fn receive(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<ReceiveReq>,
) -> Result<Json<Detail>, ApiError> {
    let notes = text(&req.notes, 2000, "Observaciones")?;
    if !req.conforming && notes.is_empty() {
        return Err(bad("Si la recepcion no es conforme, describe las observaciones"));
    }
    let photo = req.photo.as_ref().map(|f| finance::decode_upload(&f.data).map(|(b, m)| (f.name.clone(), b, m))).transpose()?;
    let inv = match req.inventory {
        Some(i) if req.conforming => {
            let q = i.quantity.unwrap_or(1.0);
            if !q.is_finite() || q <= 0.0 || q > 1e6 {
                return Err(bad("Cantidad invalida para Inventario"));
            }
            Some((i.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()), q))
        }
        _ => None,
    };
    let (conforming, user) = (req.conforming, session.username.clone());
    let inv_allowed = session.can("inventory", Lv::Usar);
    let (u, i, n) = (user.clone(), id.clone(), notes.clone());
    let (p, step_id, staff_list, item_id, photo_ref) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        expect_status(&p, &["orden"])?;
        if p.requested_by == u {
            return Err(forbidden("Separacion de funciones: quien pidio la compra no la recibe"));
        }
        if p.approved_by.as_deref() == Some(u.as_str()) {
            return Err(forbidden("Separacion de funciones: quien aprobo la compra no la recibe"));
        }
        if inv.is_some() && !inv_allowed {
            return Err(forbidden("Para crear el bien en Inventario necesitas permiso de Inventario"));
        }
        let photo_ref = photo.map(|(name, b, m)| finance::store_file(conn, &name, &b, m, &u)).transpose().map_err(internal)?;
        conn.execute(
            "INSERT INTO fin_receptions (id, purchase_id, received_by, received_at, conforming, notes, photo_file_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![finance::new_id(), i, u, finance::now(), conforming as i64, n, photo_ref.as_ref().map(|f| f.id.clone())],
        )
        .map_err(internal)?;
        let mut item_id = None;
        let step = if conforming {
            transition(conn, &i, &["orden"], "recibida")?;
            conn.execute("UPDATE fin_purchases SET received_by = ?2 WHERE id = ?1", params![i, u]).map_err(internal)?;
            if let Some((name, qty)) = &inv {
                let (supplier, cost): (String, f64) = conn
                    .query_row(
                        "SELECT COALESCE((SELECT supplier FROM fin_quotes q WHERE q.id = p.chosen_quote_id), ''), COALESCE((SELECT committed FROM fin_commitments c WHERE c.source = 'compra' AND c.source_id = p.id), p.estimated_amount)
                         FROM fin_purchases p WHERE p.id = ?1",
                        [&i],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .map_err(internal)?;
                let unit_cost = cost / qty;
                item_id = Some(create_inventory_item(conn, &p, name.as_deref().unwrap_or(&p.title), *qty, unit_cost, &supplier)?);
            }
            add_step(conn, &i, "recibida", &u, &if n.is_empty() { "Recepcion conforme".to_string() } else { format!("Recepcion conforme: {}", n) }, false)?
        } else {
            add_step(conn, &i, "recepcion_no_conforme", &u, &n, false)?
        };
        Ok((p, step, staff_users(conn), item_id, photo_ref))
    })
    .await?;
    let step_name = if conforming { "recibida" } else { "recepcion_no_conforme" };
    let msg = if conforming {
        format!("La compra #{} ({}) fue recibida conforme por {}", p.folio, p.title, user)
    } else {
        format!("Recepcion NO conforme de la compra #{} ({}) por {}: {}", p.folio, p.title, user, notes)
    };
    let mut to = vec![p.requested_by.clone()];
    if !conforming {
        to.extend(staff_list);
    }
    seal(
        &state,
        Seal {
            user: &user,
            id: &id,
            folio: p.folio,
            step_id,
            step: step_name,
            note: notes.clone(),
            extra: json!({ "conforme": conforming, "foto": photo_ref.as_ref().map(|f| &f.name), "sha256_foto": photo_ref.as_ref().map(|f| &f.sha256), "inventario": item_id }),
            decl_id: None,
            tell: Some((to, msg)),
        },
    )
    .await?;
    reload(&state, &id).await
}

// ───────────────────────── Factura ─────────────────────────

#[derive(Deserialize, Default, Clone)]
pub struct DocFields {
    pub kind: Option<String>,
    pub folio: Option<String>,
    pub issuer_rut: Option<String>,
    pub issuer_name: Option<String>,
    pub issued_on: Option<String>,
    pub net: Option<f64>,
    pub tax: Option<f64>,
    pub total: Option<f64>,
}

#[derive(Deserialize)]
pub struct InvoiceReq {
    pub file: FileIn,
    #[serde(flatten)]
    pub fields: DocFields,
}

/// Otra factura confirmada con el mismo RUT de emisor y folio (numero de la compra)
fn duplicate_of(conn: &Connection, rut: &str, folio: &str, except_doc: &str) -> Option<i64> {
    if rut.is_empty() || folio.is_empty() {
        return None;
    }
    conn.query_row(
        "SELECT COALESCE((SELECT folio FROM fin_purchases p WHERE p.id = d.purchase_id), 0) FROM fin_documents d
         WHERE d.issuer_rut = ?1 AND d.folio = ?2 AND d.id != ?3 AND d.confirmed_by IS NOT NULL LIMIT 1",
        params![rut, folio, except_doc],
        |r| r.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

fn opt_text(v: &Option<String>, max: usize, what: &str) -> Result<String, ApiError> {
    text(v.as_deref().unwrap_or(""), max, what)
}

fn opt_amount(v: Option<f64>, what: &str) -> Result<Option<f64>, ApiError> {
    match v {
        Some(x) if !x.is_finite() || !(0.0..1e11).contains(&x) => Err(bad(format!("{} invalido", what))),
        Some(x) => Ok(Some(x.round())),
        None => Ok(None),
    }
}

/// POST /api/finance/purchases/{id}/invoice: sube la factura. Si es XML del SII se lee exacto;
/// si es PDF y la IA esta configurada, ella propone los datos; si no, se llenan a mano.
/// Siempre una persona confirma (Gestionar) antes de que cuente.
pub async fn upload_invoice(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<InvoiceReq>,
) -> Result<Json<Detail>, ApiError> {
    let (bytes, mime) = finance::decode_upload(&req.file.data)?;
    let mut f = req.fields.clone();
    let mut extracted_by = "manual";
    if mime == "application/xml" {
        let d = dte::parse_dte(&bytes).map_err(bad)?;
        f = DocFields {
            kind: Some(d.kind().to_string()),
            folio: Some(d.folio),
            issuer_rut: Some(d.rut_emisor),
            issuer_name: Some(d.razon_social),
            issued_on: Some(d.fecha),
            net: d.neto,
            tax: d.iva,
            total: Some(d.total),
        };
        extracted_by = "sii_xml";
    } else if mime == "application/pdf" {
        let cfg = db_op(&state.db, |conn| Ok(crate::ai::AiConfig::load(conn))).await?;
        let text_pdf = dte::pdf_text(&bytes);
        if let (Some(cfg), true) = (cfg, text_pdf.trim().len() >= 30) {
            if let Ok(pr) = dte::propose_with_ai(&state.http_client, &cfg, &text_pdf).await {
                // Lo que la persona escribio manda sobre lo que propone la IA
                let keep = |mine: &Option<String>, ai: String| mine.clone().filter(|s| !s.trim().is_empty()).or((!ai.is_empty()).then_some(ai));
                f = DocFields {
                    kind: keep(&f.kind, pr.kind),
                    folio: keep(&f.folio, pr.folio),
                    issuer_rut: keep(&f.issuer_rut, pr.issuer_rut),
                    issuer_name: keep(&f.issuer_name, pr.issuer_name),
                    issued_on: keep(&f.issued_on, pr.issued_on),
                    net: f.net.or(pr.net),
                    tax: f.tax.or(pr.tax),
                    total: f.total.or(pr.total),
                };
                extracted_by = "ia";
            }
        }
    }
    let kind = f.kind.clone().filter(|k| !k.is_empty()).unwrap_or_else(|| "factura".to_string());
    if !["factura", "boleta", "otro"].contains(&kind.as_str()) {
        return Err(bad("Tipo de documento: factura, boleta u otro"));
    }
    let folio = opt_text(&f.folio, 20, "Folio")?;
    let issuer_rut = rut(f.issuer_rut.as_deref().unwrap_or(""))?;
    let issuer_name = opt_text(&f.issuer_name, 200, "Razon social")?;
    let issued_on = f.issued_on.clone().filter(|d| !d.trim().is_empty());
    if issued_on.as_deref().is_some_and(|d| !valid_date(d)) {
        return Err(bad("Fecha de emision invalida (AAAA-MM-DD)"));
    }
    let (net, tax, total) = (opt_amount(f.net, "Neto")?, opt_amount(f.tax, "IVA")?, opt_amount(f.total, "Total")?);

    let user = session.username.clone();
    let (u, i, s, fname) = (user.clone(), id.clone(), staff(&session), req.file.name.clone());
    let (fo, ru, ex) = (folio.clone(), issuer_rut.clone(), extracted_by);
    let (p, step_id, file) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        if !s && p.requested_by != u && p.received_by.as_deref() != Some(u.as_str()) {
            return Err(forbidden("Solo quien pidio o recibio la compra, o quien gestiona Finanzas, sube la factura"));
        }
        expect_status(&p, &["recibida"])?;
        if let Some(n) = duplicate_of(conn, &ru, &fo, "") {
            return Err(conflict(format!("Esa factura ya esta registrada (folio {} del RUT {}, compra #{})", fo, ru, n)));
        }
        // Un borrador anterior sin confirmar se reemplaza (el archivo queda guardado)
        conn.execute("DELETE FROM fin_documents WHERE purchase_id = ?1 AND confirmed_by IS NULL", [&i]).map_err(internal)?;
        let file = finance::store_file(conn, &fname, &bytes, mime, &u).map_err(internal)?;
        conn.execute(
            "INSERT INTO fin_documents (id, project_id, purchase_id, kind, folio, issuer_rut, issuer_name, issued_on, net_amount, tax_amount, total_amount, file_id, extracted_by, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![finance::new_id(), p.project_id, i, kind, fo, ru, issuer_name, issued_on, net, tax, total, file.id, ex, u, finance::now()],
        )
        .map_err(internal)?;
        let step = add_step(conn, &i, "factura_subida", &u, &format!("Respaldo {} (datos: {})", file.name, ex), false)?;
        Ok((p, step, file))
    })
    .await?;
    let staff_list = staff_users_of(&state).await;
    seal(
        &state,
        Seal {
            user: &user,
            id: &id,
            folio: p.folio,
            step_id,
            step: "factura_subida",
            note: format!("Sube {} ({})", file.name, extracted_by),
            extra: json!({ "archivo": file.name, "sha256": file.sha256, "lectura": extracted_by, "folio_factura": folio, "rut_emisor": issuer_rut }),
            decl_id: None,
            tell: Some((staff_list, format!("La compra #{} ({}) tiene una factura por confirmar", p.folio, p.title))),
        },
    )
    .await?;
    reload(&state, &id).await
}

/// POST /api/finance/purchases/{id}/invoice/{doc}/confirm (Gestionar): una persona confirma los
/// datos (en XML del SII quedan los exactos); la compra pasa a facturada con el monto de la factura
pub async fn confirm_invoice(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path((id, doc)): Path<(String, String)>,
    Json(f): Json<DocFields>,
) -> Result<Json<Detail>, ApiError> {
    if !staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas confirma facturas"));
    }
    let user = session.username.clone();
    let (u, i, d) = (user.clone(), id.clone(), doc.clone());
    let (p, step_id, total, extracted_by, folio, issuer_rut, agreed) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        expect_status(&p, &["recibida"])?;
        type Stored = (String, String, String, String, Option<String>, Option<f64>, Option<f64>, Option<f64>, String, Option<String>, Option<String>);
        let cur: Stored = conn
            .query_row(
                "SELECT kind, folio, issuer_rut, issuer_name, issued_on, net_amount, tax_amount, total_amount, extracted_by, confirmed_by, file_id FROM fin_documents WHERE id = ?1 AND purchase_id = ?2",
                params![d, i],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?)),
            )
            .optional()
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Documento no encontrado".to_string()))?;
        if cur.9.is_some() {
            return Err(conflict("Ese documento ya fue confirmado"));
        }
        // En XML del SII los datos son los exactos del documento: no se editan
        let exact = cur.8 == "sii_xml";
        let pick = |new: &Option<String>, old: &str| -> String {
            if exact { old.to_string() } else { new.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| old.to_string()) }
        };
        let kind = pick(&f.kind, &cur.0);
        let folio = text(&pick(&f.folio, &cur.1), 20, "Folio")?;
        let issuer_rut = rut(&pick(&f.issuer_rut, &cur.2))?;
        let issuer_name = text(&pick(&f.issuer_name, &cur.3), 200, "Razon social")?;
        let issued_on = if exact { cur.4.clone() } else { f.issued_on.clone().filter(|s| !s.trim().is_empty()).or(cur.4.clone()) };
        let net = if exact { cur.5 } else { opt_amount(f.net, "Neto")?.or(cur.5) };
        let tax = if exact { cur.6 } else { opt_amount(f.tax, "IVA")?.or(cur.6) };
        let total = if exact { cur.7 } else { opt_amount(f.total, "Total")?.or(cur.7) };
        if !["factura", "boleta", "otro"].contains(&kind.as_str()) {
            return Err(bad("Tipo de documento: factura, boleta u otro"));
        }
        if folio.is_empty() || issuer_rut.is_empty() {
            return Err(bad("Para confirmar faltan el folio y el RUT del emisor"));
        }
        if !issued_on.as_deref().is_some_and(valid_date) {
            return Err(bad("Para confirmar falta la fecha de emision (AAAA-MM-DD)"));
        }
        let total = pesos(total.unwrap_or(0.0), "Total de la factura")?;
        if let Some(n) = duplicate_of(conn, &issuer_rut, &folio, &d) {
            return Err(conflict(format!("Esa factura ya esta registrada (folio {} del RUT {}, compra #{})", folio, issuer_rut, n)));
        }
        let agreed: f64 = conn
            .query_row("SELECT committed FROM fin_commitments WHERE source = 'compra' AND source_id = ?1", [&i], |r| r.get(0))
            .optional()
            .map_err(internal)?
            .unwrap_or(p.estimated_amount);
        conn.execute(
            "UPDATE fin_documents SET kind = ?2, folio = ?3, issuer_rut = ?4, issuer_name = ?5, issued_on = ?6, net_amount = ?7, tax_amount = ?8, total_amount = ?9, confirmed_by = ?10 WHERE id = ?1",
            params![d, kind, folio, issuer_rut, issuer_name, issued_on, net, tax, total, u],
        )
        .map_err(internal)?;
        transition(conn, &i, &["recibida"], "facturada")?;
        conn.execute("UPDATE fin_purchases SET final_amount = ?2 WHERE id = ?1", params![i, total]).map_err(internal)?;
        commit(conn, &p, total, 0.0)?;
        let step = add_step(conn, &i, "facturada", &u, &format!("Confirma {} folio {} de {} por {}", kind, folio, issuer_rut, money(total)), false)?;
        Ok((p, step, total, cur.8, folio, issuer_rut, agreed))
    })
    .await?;
    let differs = (total - agreed).abs() > 0.5;
    seal(
        &state,
        Seal {
            user: &user,
            id: &id,
            folio: p.folio,
            step_id,
            step: "facturada",
            note: format!("Confirma factura folio {} por {}", folio, money(total)),
            extra: json!({ "documento": doc, "folio_factura": folio, "rut_emisor": issuer_rut, "total": total, "lectura": extracted_by, "monto_acordado": agreed, "difiere_del_acordado": differs }),
            decl_id: None,
            tell: tell_requester(&p, format!("La factura de tu compra #{} ({}) fue confirmada por {}", p.folio, p.title, money(total))),
        },
    )
    .await?;
    reload(&state, &id).await
}

// ───────────────────────── Pago y rendicion ─────────────────────────

#[derive(Deserialize, Default)]
pub struct PayReq {
    pub paid_on: Option<String>,
    #[serde(default)]
    pub reference: String,
}

/// POST /api/finance/purchases/{id}/pay (Gestionar): registra el pago; lo gastado pasa a ser el monto de la factura
pub async fn pay(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    body: Option<Json<PayReq>>,
) -> Result<Json<Detail>, ApiError> {
    if !staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas registra pagos"));
    }
    let req = body.map(|Json(b)| b).unwrap_or_default();
    let paid_on = req.paid_on.filter(|d| !d.trim().is_empty()).unwrap_or_else(today);
    if !valid_date(&paid_on) {
        return Err(bad("Fecha de pago invalida (AAAA-MM-DD)"));
    }
    let reference = text(&req.reference, 200, "Referencia")?;
    let user = session.username.clone();
    let (u, i, d, r) = (user.clone(), id.clone(), paid_on.clone(), reference.clone());
    let (p, step_id, amount) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        expect_status(&p, &["facturada"])?;
        let amount = p.final_amount.ok_or_else(|| conflict("La compra no tiene factura confirmada"))?;
        transition(conn, &i, &["facturada"], "pagada")?;
        commit(conn, &p, 0.0, amount)?;
        let step = add_step(conn, &i, "pagada", &u, &format!("Pago de {} el {}{}", money(amount), d, if r.is_empty() { String::new() } else { format!(" ({})", r) }), false)?;
        Ok((p, step, amount))
    })
    .await?;
    seal(&state, Seal { user: &user, id: &id, folio: p.folio, step_id, step: "pagada", note: format!("Pago de {} el {}", money(amount), paid_on), extra: json!({ "monto": amount, "fecha_pago": paid_on, "referencia": reference }), decl_id: None, tell: tell_requester(&p, format!("Se pago tu compra #{} ({}): {}", p.folio, p.title, money(amount))) }).await?;
    reload(&state, &id).await
}

/// POST /api/finance/purchases/{id}/settle (Gestionar): la compra queda rendida
pub async fn settle(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    body: Option<Json<NoteReq>>,
) -> Result<Json<Detail>, ApiError> {
    if !staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas rinde compras"));
    }
    let note = text(&body.map(|Json(b)| b.note).unwrap_or_default(), 500, "Nota")?;
    let user = session.username.clone();
    let (u, i, n) = (user.clone(), id.clone(), note.clone());
    let (p, step_id) = db_op_status(&state.db, move |conn| {
        let p = load(conn, &i)?;
        transition(conn, &i, &["pagada"], "rendida")?;
        let s = add_step(conn, &i, "rendida", &u, &n, false)?;
        Ok((p, s))
    })
    .await?;
    seal(&state, Seal { user: &user, id: &id, folio: p.folio, step_id, step: "rendida", note: note.clone(), extra: json!({ "nota": note }), decl_id: None, tell: None }).await?;
    reload(&state, &id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pesos_con_puntos() {
        assert_eq!(money(1234567.0), "$1.234.567");
        assert_eq!(money(950.0), "$950");
        assert_eq!(money(-1500.0), "-$1.500");
    }

    #[test]
    fn minimo_de_cotizaciones() {
        let s = Settings { min_quotes: 3, quote_threshold: 1_000_000.0, require_passkey: false };
        assert_eq!(required_quotes(&s, 999_999.0), 0);
        assert_eq!(required_quotes(&s, 1_000_000.0), 3);
    }

    #[test]
    fn rut_normalizado() {
        assert_eq!(rut("76.123.456-k").unwrap(), "76123456-K");
        assert!(rut("76123456-7; drop").is_err());
    }
}
