//! Inventario ampliado (ideas 1, 31, 33 y 34). Ver docs/INVENTARIO.md.
//!
//! - **Stock minimo**: si la cantidad de un item baja de su minimo se avisa una vez por
//!   Telegram a quienes gestionan Inventario; al reponer se rearma el aviso.
//! - **Vencimientos** (resinas, reactivos, baterias, licencias): salen en Plazos.
//! - **Prestamos** de herramientas con fecha de devolucion: quien gestiona Inventario presta a
//!   cualquiera; los demas se pueden llevar un item marcado como prestable. Al vencer, aviso
//!   diario a quien lo tiene y al staff.
//! - **Etiqueta QR** por item, que abre su ficha en Inventario.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{db_op, db_op_status};
use crate::error::{internal, ApiError};
use crate::roles::Level;
use crate::state::{AppState, SessionInfo};

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn now_str() -> String {
    Local::now().naive_local().format("%Y-%m-%dT%H:%M").to_string()
}

fn bad(msg: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.into())
}

fn md(s: &str) -> String {
    crate::handlers::machines::md(s)
}

fn staff_of(s: &SessionInfo) -> bool {
    s.can("inventory", Level::Gestionar)
}

fn staff(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    crate::roles::users_with_level(conn, "inventory", Level::Gestionar)
}

fn fmt_qty(q: f64) -> String {
    if (q - q.round()).abs() < 1e-9 { format!("{}", q as i64) } else { format!("{:.2}", q) }
}

// ─── Stock minimo ───

/// Revisa el stock de un item tras cambiar su cantidad: avisa una vez al bajar del minimo y
/// rearma el aviso al reponer. Devuelve el texto del aviso si toca mandarlo.
pub fn check_low_stock(conn: &Connection, item_id: &str) -> rusqlite::Result<Option<String>> {
    let row: Option<(String, f64, String, Option<f64>)> = conn
        .query_row(
            "SELECT name, quantity, unit, min_quantity FROM inventory_items WHERE id = ?1",
            [item_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((name, qty, unit, Some(min))) = row else {
        conn.execute("DELETE FROM inventory_stock_alerts WHERE item_id = ?1", [item_id])?;
        return Ok(None);
    };
    if qty > min {
        conn.execute("DELETE FROM inventory_stock_alerts WHERE item_id = ?1", [item_id])?;
        return Ok(None);
    }
    let fresh = conn.execute(
        "INSERT OR IGNORE INTO inventory_stock_alerts (item_id, alerted_at) VALUES (?1, ?2)",
        params![item_id, now_str()],
    )? > 0;
    Ok(fresh.then(|| {
        format!("📦 Queda poco: *{}* — {} {} (minimo {}). Hay que reponer.", md(&name), fmt_qty(qty), md(&unit), fmt_qty(min))
    }))
}

/// Tras crear o editar un item: aviso de stock bajo si corresponde
pub async fn after_item_change(state: &AppState, item_id: &str) {
    let id = item_id.to_string();
    let res = db_op(&state.db, move |conn| {
        let text = check_low_stock(conn, &id).map_err(|e| e.to_string())?;
        let users = if text.is_some() { staff(conn).map_err(|e| e.to_string())? } else { Vec::new() };
        Ok((text, users))
    })
    .await;
    if let Ok((Some(text), users)) = res {
        crate::handlers::notifications::notify_users(state, &users, &text).await;
        state.log_activity("Inventario", &text.replace('*', ""), "sistema").await;
    }
}

/// Items bajo su minimo (para el dashboard y la pagina)
#[derive(Debug, Serialize)]
pub struct LowStock {
    pub id: String,
    pub name: String,
    pub quantity: f64,
    pub unit: String,
    pub min_quantity: f64,
}

// ─── Vencimientos (para Plazos) ───

/// Items con fecha de vencimiento que no estan retirados: (id, nombre, fecha)
pub fn expiring(conn: &Connection) -> rusqlite::Result<Vec<(String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, expires_on FROM inventory_items WHERE expires_on IS NOT NULL AND expires_on != '' AND status != 'retirado'",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    rows.collect()
}

// ─── Prestamos ───

#[derive(Debug, Clone, Serialize)]
pub struct Loan {
    pub id: String,
    pub item_id: String,
    pub item_name: String,
    pub borrower: String,
    pub quantity: f64,
    pub lent_by: String,
    pub lent_at: String,
    /// `YYYY-MM-DD`
    pub due_on: String,
    pub note: String,
    pub returned_at: Option<String>,
    pub returned_to: Option<String>,
    /// Calculado: dias que faltan (negativo = atrasado)
    pub days_left: i64,
}

const LOAN_SELECT: &str = "SELECT l.id, l.item_id, COALESCE(i.name, '(item borrado)'), l.borrower, l.quantity, l.lent_by, l.lent_at,
    l.due_on, l.note, l.returned_at, l.returned_to FROM inventory_loans l LEFT JOIN inventory_items i ON i.id = l.item_id";

fn row_to_loan(r: &rusqlite::Row) -> rusqlite::Result<Loan> {
    let due_on: String = r.get(7)?;
    let days_left = NaiveDate::parse_from_str(&due_on, "%Y-%m-%d").map(|d| (d - today()).num_days()).unwrap_or(0);
    Ok(Loan {
        id: r.get(0)?,
        item_id: r.get(1)?,
        item_name: r.get(2)?,
        borrower: r.get(3)?,
        quantity: r.get(4)?,
        lent_by: r.get(5)?,
        lent_at: r.get(6)?,
        due_on,
        note: r.get(8)?,
        returned_at: r.get(9)?,
        returned_to: r.get(10)?,
        days_left,
    })
}

/// Cantidad prestada (sin devolver) de un item
pub fn on_loan(conn: &Connection, item_id: &str) -> rusqlite::Result<f64> {
    conn.query_row(
        "SELECT COALESCE(SUM(quantity), 0) FROM inventory_loans WHERE item_id = ?1 AND returned_at IS NULL",
        [item_id],
        |r| r.get(0),
    )
}

#[derive(Deserialize)]
pub struct LoanQuery {
    #[serde(default)]
    pub open: bool,
    #[serde(default)]
    pub mine: bool,
    pub item: Option<String>,
}

/// GET /api/inventory/loans?open=&mine=&item= — sin gestionar, solo los propios
pub async fn list_loans(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<LoanQuery>,
) -> Result<Json<Vec<Loan>>, ApiError> {
    let mine = q.mine || !staff_of(&session);
    let user = session.username.clone();
    db_op(&state.db, move |conn| {
        let mut sql = format!("{} WHERE 1 = 1", LOAN_SELECT);
        let mut args: Vec<String> = Vec::new();
        if q.open {
            sql.push_str(" AND l.returned_at IS NULL");
        }
        if mine {
            args.push(user);
            sql.push_str(&format!(" AND l.borrower = ?{}", args.len()));
        }
        if let Some(item) = q.item {
            args.push(item);
            sql.push_str(&format!(" AND l.item_id = ?{}", args.len()));
        }
        sql.push_str(" ORDER BY l.returned_at IS NOT NULL, l.due_on LIMIT 300");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_loan).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct LoanReq {
    /// A quien (por defecto, quien lo pide). Prestar a otra persona requiere gestionar
    pub borrower: Option<String>,
    #[serde(default = "one")]
    pub quantity: f64,
    /// `YYYY-MM-DD`
    pub due_on: String,
    #[serde(default)]
    pub note: String,
}

fn one() -> f64 {
    1.0
}

/// POST /api/inventory/items/{id}/loans — presta (o se lleva) un item
pub async fn create_loan(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(item_id): Path<String>,
    Json(req): Json<LoanReq>,
) -> Result<(StatusCode, Json<Loan>), ApiError> {
    let staff = staff_of(&session);
    let user = session.username.clone();
    let borrower = req.borrower.clone().map(|b| b.trim().to_string()).filter(|b| !b.is_empty()).unwrap_or_else(|| user.clone());
    if borrower != user && !staff {
        return Err((StatusCode::FORBIDDEN, "Solo quien gestiona Inventario presta a otra persona".into()));
    }
    if !(req.quantity > 0.0 && req.quantity <= 100000.0) {
        return Err(bad("Cantidad invalida"));
    }
    let due = NaiveDate::parse_from_str(req.due_on.trim(), "%Y-%m-%d").map_err(|_| bad("Fecha de devolucion invalida"))?;
    if due < today() || (due - today()).num_days() > 365 {
        return Err(bad("La devolucion tiene que ser entre hoy y un año mas"));
    }
    let note = req.note.trim().to_string();
    if note.chars().count() > 500 {
        return Err(bad("Nota demasiado larga"));
    }
    let (loan, msg) = db_op_status(&state.db, move |conn| {
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate).map_err(internal)?;
        let item: Option<(String, f64, String, bool, String)> = tx
            .query_row(
                "SELECT name, quantity, unit, loanable, status FROM inventory_items WHERE id = ?1",
                [&item_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0, r.get(4)?)),
            )
            .optional()
            .map_err(internal)?;
        let (name, qty, unit, loanable, status) = item.ok_or((StatusCode::NOT_FOUND, "Item no encontrado".to_string()))?;
        if !loanable && !staff {
            return Err((StatusCode::FORBIDDEN, format!("{} no esta marcado como prestable: pidelo a quien gestiona Inventario", name)));
        }
        if status == "retirado" || status == "mantenimiento" {
            return Err((StatusCode::CONFLICT, format!("{} esta en {}", name, status)));
        }
        let exists: bool = tx
            .query_row("SELECT 1 FROM web_users WHERE username = ?1 AND role != 'pendiente'", [&borrower], |_| Ok(true))
            .optional()
            .map_err(internal)?
            .unwrap_or(false);
        if !exists {
            return Err(bad("Esa cuenta no existe o esta pendiente"));
        }
        let available = qty - on_loan(&tx, &item_id).map_err(internal)?;
        if req.quantity > available + 1e-9 {
            return Err((StatusCode::CONFLICT, format!("Solo quedan {} {} disponibles de {}", fmt_qty(available.max(0.0)), unit, name)));
        }
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        tx.execute(
            "INSERT INTO inventory_loans (id, item_id, borrower, quantity, lent_by, lent_at, due_on, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, item_id, borrower, req.quantity, user, now_str(), due.format("%Y-%m-%d").to_string(), note],
        )
        .map_err(internal)?;
        let loan = tx.query_row(&format!("{} WHERE l.id = ?1", LOAN_SELECT), [&id], row_to_loan).map_err(internal)?;
        tx.commit().map_err(internal)?;
        let msg = format!("Presto {} {} de {} a {} hasta {}", fmt_qty(loan.quantity), unit, name, loan.borrower, loan.due_on);
        Ok((loan, msg))
    })
    .await?;
    state.log_activity("Inventario", &msg, &session.username).await;
    if loan.borrower != session.username {
        let text = format!("🔧 Te prestaron *{}* hasta el {}. Devuelvelo a tiempo.", md(&loan.item_name), loan.due_on);
        crate::handlers::notifications::notify_users(&state, std::slice::from_ref(&loan.borrower), &text).await;
    }
    publish(&state);
    Ok((StatusCode::CREATED, Json(loan)))
}

#[derive(Deserialize, Default)]
pub struct ReturnReq {
    #[serde(default)]
    pub note: String,
}

/// POST /api/inventory/loans/{id}/return — lo devuelve quien lo tiene o lo recibe el staff
pub async fn return_loan(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<ReturnReq>,
) -> Result<Json<Loan>, ApiError> {
    let staff = staff_of(&session);
    let user = session.username.clone();
    let note = req.note.trim().chars().take(500).collect::<String>();
    let loan = db_op_status(&state.db, move |conn| {
        let loan = conn
            .query_row(&format!("{} WHERE l.id = ?1", LOAN_SELECT), [&id], row_to_loan)
            .optional()
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Prestamo no encontrado".to_string()))?;
        if loan.returned_at.is_some() {
            return Err((StatusCode::CONFLICT, "Ya estaba devuelto".into()));
        }
        if loan.borrower != user && !staff {
            return Err((StatusCode::FORBIDDEN, "Lo devuelve quien lo tiene o lo recibe quien gestiona Inventario".into()));
        }
        let note = if note.is_empty() { loan.note.clone() } else if loan.note.is_empty() { note } else { format!("{} · {}", loan.note, note) };
        conn.execute(
            "UPDATE inventory_loans SET returned_at = ?2, returned_to = ?3, note = ?4 WHERE id = ?1",
            params![id, now_str(), user, note],
        )
        .map_err(internal)?;
        conn.query_row(&format!("{} WHERE l.id = ?1", LOAN_SELECT), [&id], row_to_loan).map_err(internal)
    })
    .await?;
    state
        .log_activity("Inventario", &format!("Devolucion de {} ({}), prestado a {}", loan.item_name, fmt_qty(loan.quantity), loan.borrower), &session.username)
        .await;
    publish(&state);
    Ok(Json(loan))
}

fn publish(state: &AppState) {
    state.events.publish("inventory.loans", (), crate::events::Audience::All, Some("inventory"));
}

/// Cada hora: aviso diario por prestamo atrasado a quien lo tiene y al staff
pub async fn overdue_loop(state: AppState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        tick.tick().await;
        let today_s = today().format("%Y-%m-%d").to_string();
        let due = db_op(&state.db, move |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "{} WHERE l.returned_at IS NULL AND l.due_on < ?1 AND (l.reminded_on IS NULL OR l.reminded_on < ?1)",
                    LOAN_SELECT
                ))
                .map_err(|e| e.to_string())?;
            let list: Vec<Loan> = stmt.query_map([&today_s], row_to_loan).map_err(|e| e.to_string())?.filter_map(|r| r.ok()).collect();
            for l in &list {
                conn.execute("UPDATE inventory_loans SET reminded_on = ?2 WHERE id = ?1", params![l.id, today_s]).map_err(|e| e.to_string())?;
            }
            let staff = staff(conn).map_err(|e| e.to_string())?;
            Ok((list, staff))
        })
        .await;
        let Ok((list, staff)) = due else { continue };
        for l in list {
            let late = -l.days_left;
            let mine = format!("⏰ Tienes *{}* prestado desde el {} y debias devolverlo el {} ({} dia{} de atraso).", md(&l.item_name), &l.lent_at[..10], l.due_on, late, if late == 1 { "" } else { "s" });
            crate::handlers::notifications::notify_users(&state, std::slice::from_ref(&l.borrower), &mine).await;
            let others: Vec<String> = staff.iter().filter(|s| **s != l.borrower).cloned().collect();
            let text = format!("⏰ Prestamo atrasado: *{}* lo tiene {} desde el {} (debia volver el {}).", md(&l.item_name), md(&l.borrower), &l.lent_at[..10], l.due_on);
            crate::handlers::notifications::notify_users(&state, &others, &text).await;
        }
    }
}

// ─── QR ───

#[derive(Deserialize)]
pub struct QrQuery {
    pub base: String,
}

#[derive(Serialize)]
pub struct QrResp {
    pub url: String,
    pub svg: String,
}

/// GET /api/inventory/items/{id}/qr?base=<origen> — QR que abre el item en Inventario
pub async fn item_qr(State(state): State<AppState>, Path(id): Path<String>, Query(q): Query<QrQuery>) -> Result<Json<QrResp>, ApiError> {
    let origin = crate::handlers::machines::clean_origin(&q.base)?;
    let id2 = id.clone();
    let exists = db_op(&state.db, move |conn| {
        conn.query_row("SELECT 1 FROM inventory_items WHERE id = ?1", [&id2], |_| Ok(())).optional().map_err(|e| e.to_string())
    })
    .await?;
    if exists.is_none() {
        return Err((StatusCode::NOT_FOUND, "Item no encontrado".into()));
    }
    let url = format!("{}/inventory?item={}", origin, urlencoding::encode(&id));
    let svg = crate::totp::qr_svg(&url).map_err(internal)?;
    Ok(Json(QrResp { url, svg }))
}

/// GET /api/inventory/low-stock — items en o bajo su minimo
pub async fn low_stock(State(state): State<AppState>) -> Result<Json<Vec<LowStock>>, ApiError> {
    db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, quantity, unit, min_quantity FROM inventory_items
                 WHERE min_quantity IS NOT NULL AND quantity <= min_quantity AND status != 'retirado' ORDER BY name",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok(LowStock { id: r.get(0)?, name: r.get(1)?, quantity: r.get(2)?, unit: r.get(3)?, min_quantity: r.get(4)? }))
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())
    })
    .await
    .map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cantidades_legibles() {
        assert_eq!(fmt_qty(3.0), "3");
        assert_eq!(fmt_qty(2.5), "2.50");
    }
}
