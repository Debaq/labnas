//! Caja chica con arqueo e ingresos por servicios (idea 64). Un fondo por proyecto con un
//! responsable que lo custodia; los egresos llevan respaldo con huella y cuentan como gastado
//! contra un item del presupuesto (`fin_commitments`, fuente "caja"). El arqueo (conteo fisico
//! contra el saldo calculado) lo hace otra persona, no quien custodia. Nada se borra: se anula
//! con motivo, y todo queda en la bitacora inalterable.

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Extension, Json, Router,
};
use chrono::Duration;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{bad, clean, conflict, forbidden, is_staff, ledger_annul, md, money_ok, not_found, open_config, parse_day, publish, today, user_exists};
use crate::db::db_op_status;
use crate::error::{internal, ApiError};
use crate::finance::{self, FileRef};
use crate::state::{AppState, SessionInfo};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/finance/petty/funds", get(list_funds).post(create_fund))
        .route("/api/finance/petty/funds/{id}", get(fund_detail).put(update_fund))
        .route("/api/finance/petty/funds/{id}/moves", post(create_move))
        .route("/api/finance/petty/funds/{id}/counts", post(create_count))
        .route("/api/finance/petty/moves/{id}/annul", post(annul_move))
        .route("/api/finance/incomes", get(list_incomes).post(create_income))
        .route("/api/finance/incomes/{id}/annul", post(annul_income))
}

fn subject(title: &str) -> String {
    format!("caja chica: {}", title)
}

// ─── Fondos ───

#[derive(Debug, Clone, Serialize)]
pub struct Fund {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub name: String,
    pub responsible: String,
    pub arqueo_days: i64,
    pub balance: f64,
    pub last_count_at: Option<String>,
    pub last_difference: Option<f64>,
    /// Fecha limite del proximo arqueo (AAAA-MM-DD)
    pub next_arqueo: String,
    pub arqueo_overdue: bool,
    pub closed: bool,
}

const FUND_SQL: &str = "SELECT f.id, f.project_id, COALESCE(e.title, f.project_id), f.name, f.responsible, f.arqueo_days, f.created_at,
        COALESCE((SELECT SUM(CASE m.kind WHEN 'ingreso' THEN m.amount ELSE -m.amount END) FROM fin_petty_moves m
                  WHERE m.fund_id = f.id AND m.annulled_at IS NULL), 0),
        (SELECT c.counted_at FROM fin_petty_counts c WHERE c.fund_id = f.id ORDER BY c.counted_at DESC LIMIT 1),
        (SELECT c.difference FROM fin_petty_counts c WHERE c.fund_id = f.id ORDER BY c.counted_at DESC LIMIT 1),
        p.closed_at IS NOT NULL
    FROM fin_petty_funds f
    LEFT JOIN portfolio_entries e ON e.id = f.project_id
    LEFT JOIN fin_projects p ON p.project_id = f.project_id";

fn day_of(s: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

fn fund_row(r: &rusqlite::Row) -> rusqlite::Result<Fund> {
    let created: String = r.get(6)?;
    let last_count_at: Option<String> = r.get(8)?;
    let days: i64 = r.get(5)?;
    let base = last_count_at.as_deref().and_then(day_of).or_else(|| day_of(&created)).unwrap_or_else(today);
    let due = base + Duration::days(days);
    Ok(Fund {
        id: r.get(0)?,
        project_id: r.get(1)?,
        title: r.get(2)?,
        name: r.get(3)?,
        responsible: r.get(4)?,
        arqueo_days: days,
        balance: r.get(7)?,
        last_count_at,
        last_difference: r.get(9)?,
        next_arqueo: due.format("%Y-%m-%d").to_string(),
        arqueo_overdue: due < today(),
        closed: r.get(10)?,
    })
}

fn load_fund(conn: &Connection, id: &str) -> Result<Fund, ApiError> {
    conn.query_row(&format!("{} WHERE f.id = ?1", FUND_SQL), [id], fund_row)
        .optional()
        .map_err(internal)?
        .ok_or_else(|| not_found("Fondo de caja chica no encontrado"))
}

/// Arqueos que vencen o ya vencieron en fondos de proyectos abiertos (para Plazos)
pub struct PendingArqueo {
    pub fund_id: String,
    pub title: String,
    pub responsible: String,
    pub due: String,
}

pub fn pending_arqueos(conn: &Connection) -> rusqlite::Result<Vec<PendingArqueo>> {
    let mut stmt = conn.prepare(&format!("{} WHERE p.closed_at IS NULL", FUND_SQL))?;
    let funds = stmt.query_map([], fund_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(funds
        .into_iter()
        .map(|f| PendingArqueo { fund_id: f.id, title: f.title, responsible: f.responsible, due: f.next_arqueo })
        .collect())
}

#[derive(Deserialize)]
struct ProjectQuery {
    project_id: Option<String>,
}

/// GET /api/finance/petty/funds?project_id=
async fn list_funds(State(state): State<AppState>, Query(q): Query<ProjectQuery>) -> Result<Json<Vec<Fund>>, ApiError> {
    db_op_status(&state.db, move |conn| {
        let pid = q.project_id.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        let mut stmt = conn
            .prepare(&format!("{} WHERE (?1 IS NULL OR f.project_id = ?1) ORDER BY 3", FUND_SQL))
            .map_err(internal)?;
        let rows = stmt.query_map([pid], fund_row).map_err(internal)?.collect::<rusqlite::Result<Vec<_>>>().map_err(internal)?;
        Ok(rows)
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
struct FundReq {
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    name: String,
    responsible: String,
    arqueo_days: Option<i64>,
}

fn check_fund(req: &FundReq) -> Result<(String, String, i64), ApiError> {
    let days = req.arqueo_days.unwrap_or(30);
    if !(7..=365).contains(&days) {
        return Err(bad("El arqueo se hace cada 7 a 365 dias"));
    }
    Ok((clean(&req.name, 80, "El nombre", false)?, clean(&req.responsible, 60, "El responsable", true)?, days))
}

/// POST /api/finance/petty/funds — abre el fondo de un proyecto (uno por proyecto)
async fn create_fund(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<FundReq>,
) -> Result<(axum::http::StatusCode, Json<Fund>), ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas abre una caja chica"));
    }
    let (name, responsible, days) = check_fund(&req)?;
    let user = session.username.clone();
    let fund = db_op_status(&state.db, move |conn| {
        let cfg = open_config(conn, req.project_id.trim())?;
        if !user_exists(conn, &responsible).map_err(internal)? {
            return Err(bad("El responsable debe ser una cuenta de LabNAS"));
        }
        let exists: bool = conn
            .query_row("SELECT EXISTS(SELECT 1 FROM fin_petty_funds WHERE project_id = ?1)", [&cfg.project_id], |r| r.get(0))
            .map_err(internal)?;
        if exists {
            return Err(conflict("El proyecto ya tiene una caja chica"));
        }
        super::ensure_project(conn, &cfg.project_id, &user).map_err(internal)?;
        let id = finance::new_id();
        conn.execute(
            "INSERT INTO fin_petty_funds (id, project_id, name, responsible, arqueo_days, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, cfg.project_id, name, responsible, days, user, finance::now()],
        )
        .map_err(internal)?;
        load_fund(conn, &id)
    })
    .await?;
    finance::ledger(
        &state,
        &session.username,
        "caja",
        &subject(&fund.title),
        json!({ "accion": "fondo_abierto", "fondo": fund.id, "responsable": fund.responsible, "arqueo_cada_dias": fund.arqueo_days }),
    )
    .await?;
    state.log_activity("Finanzas", &format!("Caja chica abierta: {}", fund.title), &session.username).await;
    publish(&state);
    Ok((axum::http::StatusCode::CREATED, Json(fund)))
}

/// PUT /api/finance/petty/funds/{id} — nombre, responsable y frecuencia del arqueo
async fn update_fund(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<FundReq>,
) -> Result<Json<Fund>, ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas cambia la caja chica"));
    }
    let (name, responsible, days) = check_fund(&req)?;
    let (before, after) = db_op_status(&state.db, move |conn| {
        let before = load_fund(conn, &id)?;
        open_config(conn, &before.project_id)?;
        if !user_exists(conn, &responsible).map_err(internal)? {
            return Err(bad("El responsable debe ser una cuenta de LabNAS"));
        }
        conn.execute(
            "UPDATE fin_petty_funds SET name = ?2, responsible = ?3, arqueo_days = ?4 WHERE id = ?1",
            params![id, name, responsible, days],
        )
        .map_err(internal)?;
        Ok((before, load_fund(conn, &id)?))
    })
    .await?;
    finance::ledger(
        &state,
        &session.username,
        "caja",
        &subject(&after.title),
        json!({
            "accion": "fondo_modificado", "fondo": after.id,
            "antes": { "responsable": before.responsible, "arqueo_cada_dias": before.arqueo_days },
            "despues": { "responsable": after.responsible, "arqueo_cada_dias": after.arqueo_days },
        }),
    )
    .await?;
    publish(&state);
    Ok(Json(after))
}

// ─── Movimientos y arqueos ───

#[derive(Serialize)]
struct Move {
    id: String,
    kind: String,
    amount: f64,
    concept: String,
    moved_on: String,
    budget_item_id: Option<String>,
    item_name: Option<String>,
    file: Option<FileRef>,
    created_by: String,
    created_at: String,
    ledger_seq: Option<i64>,
    annulled_at: Option<String>,
    annulled_by: Option<String>,
    annul_reason: Option<String>,
}

#[derive(Serialize)]
struct Count {
    id: String,
    counted: f64,
    expected: f64,
    difference: f64,
    notes: String,
    counted_by: String,
    counted_at: String,
    ledger_seq: Option<i64>,
}

#[derive(Serialize)]
struct FundDetail {
    fund: Fund,
    moves: Vec<Move>,
    counts: Vec<Count>,
}

/// GET /api/finance/petty/funds/{id}
async fn fund_detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<FundDetail>, ApiError> {
    db_op_status(&state.db, move |conn| {
        let fund = load_fund(conn, &id)?;
        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.kind, m.amount, m.concept, m.moved_on, m.budget_item_id, i.name, m.file_id, m.created_by, m.created_at,
                        m.ledger_seq, m.annulled_at, m.annulled_by, m.annul_reason
                 FROM fin_petty_moves m LEFT JOIN fin_budget_items i ON i.id = m.budget_item_id
                 WHERE m.fund_id = ?1 ORDER BY m.moved_on DESC, m.created_at DESC LIMIT 500",
            )
            .map_err(internal)?;
        let raw = stmt
            .query_map([&id], |r| {
                Ok((
                    Move {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        amount: r.get(2)?,
                        concept: r.get(3)?,
                        moved_on: r.get(4)?,
                        budget_item_id: r.get(5)?,
                        item_name: r.get(6)?,
                        file: None,
                        created_by: r.get(8)?,
                        created_at: r.get(9)?,
                        ledger_seq: r.get(10)?,
                        annulled_at: r.get(11)?,
                        annulled_by: r.get(12)?,
                        annul_reason: r.get(13)?,
                    },
                    r.get::<_, Option<String>>(7)?,
                ))
            })
            .map_err(internal)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(internal)?;
        let mut moves = Vec::new();
        for (mut m, file_id) in raw {
            m.file = match file_id {
                Some(f) => finance::file_ref(conn, &f).map_err(internal)?,
                None => None,
            };
            moves.push(m);
        }
        let mut stmt = conn
            .prepare(
                "SELECT id, counted, expected, difference, notes, counted_by, counted_at, ledger_seq
                 FROM fin_petty_counts WHERE fund_id = ?1 ORDER BY counted_at DESC LIMIT 100",
            )
            .map_err(internal)?;
        let counts = stmt
            .query_map([&id], |r| {
                Ok(Count {
                    id: r.get(0)?,
                    counted: r.get(1)?,
                    expected: r.get(2)?,
                    difference: r.get(3)?,
                    notes: r.get(4)?,
                    counted_by: r.get(5)?,
                    counted_at: r.get(6)?,
                    ledger_seq: r.get(7)?,
                })
            })
            .map_err(internal)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(internal)?;
        Ok(FundDetail { fund, moves, counts })
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
struct MoveReq {
    /// ingreso (reposicion del fondo) | egreso (gasto)
    kind: String,
    amount: f64,
    concept: String,
    /// AAAA-MM-DD (hoy por defecto)
    moved_on: Option<String>,
    /// Item de presupuesto al que se imputa el gasto (obligatorio en egresos)
    budget_item_id: Option<String>,
    /// Respaldo (boleta, factura, foto) como data URL o base64 (obligatorio en egresos)
    file: Option<String>,
    #[serde(default)]
    file_name: String,
}

/// POST /api/finance/petty/funds/{id}/moves — el ingreso al fondo lo hace quien gestiona
/// Finanzas; el egreso, tambien quien custodia el fondo
async fn create_move(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<MoveReq>,
) -> Result<(axum::http::StatusCode, Json<Move>), ApiError> {
    let egreso = match req.kind.as_str() {
        "ingreso" => false,
        "egreso" => true,
        _ => return Err(bad("El tipo es ingreso o egreso")),
    };
    if !(req.amount.is_finite() && req.amount > 0.0 && money_ok(req.amount)) {
        return Err(bad("El monto debe ser mayor que cero"));
    }
    let concept = clean(&req.concept, 200, "El concepto", true)?;
    let moved_on = match req.moved_on.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => parse_day(d, "Fecha")?,
        None => today(),
    };
    if moved_on > today() {
        return Err(bad("La fecha no puede ser futura"));
    }
    let upload = if egreso {
        if req.budget_item_id.as_deref().map(str::trim).unwrap_or("").is_empty() {
            return Err(bad("El egreso se imputa a un item del presupuesto"));
        }
        let data = req.file.as_deref().ok_or_else(|| bad("El egreso necesita su respaldo (boleta, factura o foto)"))?;
        Some(finance::decode_upload(data)?)
    } else {
        None
    };
    let (staff, user) = (is_staff(&session), session.username.clone());
    let file_name = req.file_name.clone();
    let move_id = finance::new_id();
    let mid = move_id.clone();
    let (kind, amount, item_req) = (req.kind.clone(), req.amount, req.budget_item_id.clone());
    let concept_c = concept.clone();
    let (fund, file, item_name) = db_op_status(&state.db, move |conn| {
        let fund = load_fund(conn, &id)?;
        let cfg = open_config(conn, &fund.project_id)?;
        let allowed = if egreso { staff || fund.responsible == user } else { staff };
        if !allowed {
            return Err(forbidden(if egreso {
                "Solo el responsable de la caja o quien gestiona Finanzas registra gastos"
            } else {
                "Solo quien gestiona Finanzas repone el fondo"
            }));
        }
        if egreso && amount > fund.balance + 0.005 {
            return Err(conflict(format!("Saldo insuficiente en la caja (disponible {:.0})", fund.balance)));
        }
        let mut item_id: Option<String> = None;
        let mut item_name: Option<String> = None;
        if egreso {
            let want = item_req.as_deref().unwrap_or("").trim().to_string();
            let found: Option<String> = conn
                .query_row(
                    "SELECT name FROM fin_budget_items WHERE id = ?1 AND project_id = ?2",
                    params![want, cfg.project_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(internal)?;
            item_name = Some(found.ok_or_else(|| bad("El item no existe en el presupuesto del proyecto"))?);
            item_id = Some(want);
        }
        let file = match &upload {
            Some((bytes, mime)) => Some(finance::store_file(conn, &file_name, bytes, mime, &user).map_err(internal)?),
            None => None,
        };
        conn.execute(
            "INSERT INTO fin_petty_moves (id, fund_id, kind, amount, concept, moved_on, budget_item_id, file_id, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                mid, fund.id, kind, amount, concept_c, moved_on.format("%Y-%m-%d").to_string(),
                item_id, file.as_ref().map(|f| f.id.clone()), user, finance::now()
            ],
        )
        .map_err(internal)?;
        if egreso {
            finance::set_commitment(conn, "caja", &mid, &cfg.project_id, item_id.as_deref(), &concept_c, 0.0, amount).map_err(internal)?;
        }
        Ok((fund, file, item_name))
    })
    .await?;
    let seq = finance::ledger(
        &state,
        &session.username,
        "caja",
        &subject(&fund.title),
        json!({
            "accion": if egreso { "egreso" } else { "ingreso" },
            "fondo": fund.id, "movimiento": move_id, "monto": req.amount, "concepto": concept,
            "item": item_name, "respaldo": file.as_ref().map(|f| json!({ "id": f.id, "nombre": f.name, "sha256": f.sha256 })),
        }),
    )
    .await?;
    let created_by = session.username.clone();
    let mv = db_op_status(&state.db, move |conn| {
        conn.execute("UPDATE fin_petty_moves SET ledger_seq = ?2 WHERE id = ?1", params![move_id, seq]).map_err(internal)?;
        Ok(Move {
            id: move_id,
            kind: req.kind,
            amount: req.amount,
            concept,
            moved_on: moved_on.format("%Y-%m-%d").to_string(),
            budget_item_id: req.budget_item_id.filter(|_| egreso),
            item_name,
            file,
            created_by,
            created_at: finance::now(),
            ledger_seq: Some(seq),
            annulled_at: None,
            annulled_by: None,
            annul_reason: None,
        })
    })
    .await?;
    state.log_activity("Finanzas", &format!("Caja chica {} {:.0}: {}", mv.kind, mv.amount, fund.title), &session.username).await;
    if egreso {
        super::run_alerts(&state).await;
    }
    publish(&state);
    Ok((axum::http::StatusCode::CREATED, Json(mv)))
}

#[derive(Deserialize)]
struct AnnulReq {
    reason: String,
}

/// POST /api/finance/petty/moves/{id}/annul — anula con motivo (el egreso deja de contar)
async fn annul_move(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<AnnulReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas anula movimientos"));
    }
    let reason = clean(&req.reason, 300, "El motivo", true)?;
    let user = session.username.clone();
    let (mid, r2) = (id.clone(), reason.clone());
    let (fund, orig_seq, kind, amount) = db_op_status(&state.db, move |conn| {
        type MoveRow = (String, String, f64, String, Option<String>, Option<String>, Option<i64>, Option<String>);
        let row: Option<MoveRow> = conn
            .query_row(
                "SELECT fund_id, kind, amount, concept, budget_item_id, annulled_at, ledger_seq, file_id FROM fin_petty_moves WHERE id = ?1",
                [&mid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
            )
            .optional()
            .map_err(internal)?;
        let (fund_id, kind, amount, concept, item, annulled, seq, _) = row.ok_or_else(|| not_found("Movimiento no encontrado"))?;
        if annulled.is_some() {
            return Err(conflict("El movimiento ya esta anulado"));
        }
        let fund = load_fund(conn, &fund_id)?;
        let cfg = open_config(conn, &fund.project_id)?;
        if kind == "ingreso" && fund.balance - amount < -0.005 {
            return Err(conflict("Anular este ingreso dejaria el saldo de la caja en negativo"));
        }
        conn.execute(
            "UPDATE fin_petty_moves SET annulled_at = ?2, annulled_by = ?3, annul_reason = ?4 WHERE id = ?1",
            params![mid, finance::now(), user, r2],
        )
        .map_err(internal)?;
        if kind == "egreso" {
            finance::set_commitment(conn, "caja", &mid, &cfg.project_id, item.as_deref(), &concept, 0.0, 0.0).map_err(internal)?;
        }
        Ok((fund, seq, kind, amount))
    })
    .await?;
    let data = json!({ "accion": "movimiento_anulado", "fondo": fund.id, "movimiento": id, "tipo": kind, "monto": amount, "motivo": reason });
    let seq = match orig_seq {
        Some(s) => ledger_annul(&state, &session.username, "caja", &subject(&fund.title), s, data).await?,
        None => finance::ledger(&state, &session.username, "caja", &subject(&fund.title), data).await?,
    };
    let mid = id.clone();
    db_op_status(&state.db, move |conn| {
        conn.execute("UPDATE fin_petty_moves SET annul_seq = ?2 WHERE id = ?1", params![mid, seq]).map_err(internal)
    })
    .await?;
    state.log_activity("Finanzas", &format!("Movimiento de caja chica anulado ({}): {}", fund.title, reason), &session.username).await;
    super::run_alerts(&state).await;
    publish(&state);
    Ok(Json(json!({ "ok": true, "annul_seq": seq })))
}

#[derive(Deserialize)]
struct CountReq {
    /// Dinero contado fisicamente
    counted: f64,
    #[serde(default)]
    notes: String,
}

/// POST /api/finance/petty/funds/{id}/counts — arqueo: conteo fisico contra saldo calculado.
/// Quien custodia el fondo no puede arquearlo.
async fn create_count(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<CountReq>,
) -> Result<(axum::http::StatusCode, Json<Count>), ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas hace el arqueo"));
    }
    if !money_ok(req.counted) {
        return Err(bad("Monto contado invalido"));
    }
    let notes = clean(&req.notes, 500, "Las observaciones", false)?;
    let user = session.username.clone();
    let count_id = finance::new_id();
    let (cid, n2) = (count_id.clone(), notes.clone());
    let (fund, count) = db_op_status(&state.db, move |conn| {
        let fund = load_fund(conn, &id)?;
        open_config(conn, &fund.project_id)?;
        if fund.responsible == user {
            return Err(forbidden("Quien custodia la caja no puede hacer su propio arqueo: lo hace otra persona"));
        }
        let difference = ((req.counted - fund.balance) * 100.0).round() / 100.0;
        if difference != 0.0 && n2.is_empty() {
            return Err(bad("Hay diferencia: explicala en las observaciones"));
        }
        let at = finance::now();
        conn.execute(
            "INSERT INTO fin_petty_counts (id, fund_id, counted, expected, difference, notes, counted_by, counted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![cid, fund.id, req.counted, fund.balance, difference, n2, user, at],
        )
        .map_err(internal)?;
        let count = Count {
            id: cid,
            counted: req.counted,
            expected: fund.balance,
            difference,
            notes: n2,
            counted_by: user,
            counted_at: at,
            ledger_seq: None,
        };
        Ok((fund, count))
    })
    .await?;
    let seq = finance::ledger(
        &state,
        &session.username,
        "caja",
        &subject(&fund.title),
        json!({
            "accion": "arqueo", "fondo": fund.id, "responsable": fund.responsible, "arqueo": count.id,
            "contado": count.counted, "saldo_calculado": count.expected, "diferencia": count.difference, "observaciones": count.notes,
        }),
    )
    .await?;
    let cid = count_id.clone();
    db_op_status(&state.db, move |conn| {
        conn.execute("UPDATE fin_petty_counts SET ledger_seq = ?2 WHERE id = ?1", params![cid, seq]).map_err(internal)
    })
    .await?;
    state.log_activity("Finanzas", &format!("Arqueo de caja chica {} (diferencia {:.0})", fund.title, count.difference), &session.username).await;
    if count.difference != 0.0 {
        let staff = crate::db::db_op(&state.db, |conn| {
            crate::roles::users_with_level(conn, "finanzas", crate::roles::Level::Gestionar).map_err(|e| e.to_string())
        })
        .await
        .unwrap_or_default();
        let mut users = staff;
        users.push(fund.responsible.clone());
        users.sort();
        users.dedup();
        let text = format!(
            "⚠️ *Arqueo con diferencia*\nCaja chica de {}: contado ${:.0}, saldo calculado ${:.0}, diferencia ${:.0} (arqueo de {}).",
            md(&fund.title), count.counted, count.expected, count.difference, md(&count.counted_by)
        );
        crate::handlers::notifications::notify_users(&state, &users, &text).await;
    }
    publish(&state);
    Ok((axum::http::StatusCode::CREATED, Json(Count { ledger_seq: Some(seq), ..count })))
}

// ─── Ingresos por servicios ───

#[derive(Serialize)]
struct Income {
    id: String,
    project_id: String,
    amount: f64,
    origin: String,
    destination: String,
    received_on: String,
    notes: String,
    file: Option<FileRef>,
    created_by: String,
    created_at: String,
    ledger_seq: Option<i64>,
    annulled_at: Option<String>,
    annulled_by: Option<String>,
    annul_reason: Option<String>,
}

const INCOME_SQL: &str = "SELECT id, project_id, amount, origin, destination, received_on, notes, file_id, created_by, created_at, ledger_seq,
        annulled_at, annulled_by, annul_reason FROM fin_incomes";

fn income_rows(conn: &Connection, sql: &str, arg: Option<String>) -> Result<Vec<Income>, ApiError> {
    let mut stmt = conn.prepare(sql).map_err(internal)?;
    let raw = stmt
        .query_map([arg], |r| {
            Ok((
                Income {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    amount: r.get(2)?,
                    origin: r.get(3)?,
                    destination: r.get(4)?,
                    received_on: r.get(5)?,
                    notes: r.get(6)?,
                    file: None,
                    created_by: r.get(8)?,
                    created_at: r.get(9)?,
                    ledger_seq: r.get(10)?,
                    annulled_at: r.get(11)?,
                    annulled_by: r.get(12)?,
                    annul_reason: r.get(13)?,
                },
                r.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(internal)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(internal)?;
    let mut out = Vec::new();
    for (mut i, f) in raw {
        i.file = match f {
            Some(f) => finance::file_ref(conn, &f).map_err(internal)?,
            None => None,
        };
        out.push(i);
    }
    Ok(out)
}

/// GET /api/finance/incomes?project_id=
async fn list_incomes(State(state): State<AppState>, Query(q): Query<ProjectQuery>) -> Result<Json<Vec<Income>>, ApiError> {
    db_op_status(&state.db, move |conn| {
        let pid = q.project_id.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        income_rows(conn, &format!("{} WHERE (?1 IS NULL OR project_id = ?1) ORDER BY received_on DESC, created_at DESC LIMIT 500", INCOME_SQL), pid)
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
struct IncomeReq {
    project_id: String,
    amount: f64,
    origin: String,
    #[serde(default)]
    destination: String,
    received_on: Option<String>,
    #[serde(default)]
    notes: String,
    /// Respaldo opcional (factura emitida, comprobante)
    file: Option<String>,
    #[serde(default)]
    file_name: String,
}

/// POST /api/finance/incomes — ingreso por servicios, registrado por proyecto
async fn create_income(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<IncomeReq>,
) -> Result<(axum::http::StatusCode, Json<Income>), ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas registra ingresos"));
    }
    if !(req.amount.is_finite() && req.amount > 0.0 && money_ok(req.amount)) {
        return Err(bad("El monto debe ser mayor que cero"));
    }
    let origin = clean(&req.origin, 120, "El origen", true)?;
    let destination = clean(&req.destination, 120, "El destino", false)?;
    let notes = clean(&req.notes, 300, "Las notas", false)?;
    let received_on = match req.received_on.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => parse_day(d, "Fecha")?,
        None => today(),
    };
    if received_on > today() {
        return Err(bad("La fecha no puede ser futura"));
    }
    let upload = req.file.as_deref().map(finance::decode_upload).transpose()?;
    let user = session.username.clone();
    let income_id = finance::new_id();
    let iid = income_id.clone();
    let (origin_c, destination_c, notes_c) = (origin.clone(), destination.clone(), notes.clone());
    let (cfg, file) = db_op_status(&state.db, move |conn| {
        let cfg = open_config(conn, req.project_id.trim())?;
        super::ensure_project(conn, &cfg.project_id, &user).map_err(internal)?;
        let file = match &upload {
            Some((bytes, mime)) => Some(finance::store_file(conn, &req.file_name, bytes, mime, &user).map_err(internal)?),
            None => None,
        };
        conn.execute(
            "INSERT INTO fin_incomes (id, project_id, amount, origin, destination, received_on, notes, file_id, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                iid, cfg.project_id, req.amount, origin_c, destination_c, received_on.format("%Y-%m-%d").to_string(), notes_c,
                file.as_ref().map(|f| f.id.clone()), user, finance::now()
            ],
        )
        .map_err(internal)?;
        Ok((cfg, file))
    })
    .await?;
    let seq = finance::ledger(
        &state,
        &session.username,
        "caja",
        &format!("ingreso: {}", cfg.title),
        json!({
            "accion": "ingreso_servicio", "ingreso": income_id, "monto": req.amount, "origen": origin, "destino": destination,
            "fecha": received_on.format("%Y-%m-%d").to_string(),
            "respaldo": file.as_ref().map(|f| json!({ "id": f.id, "nombre": f.name, "sha256": f.sha256 })),
        }),
    )
    .await?;
    let income = db_op_status(&state.db, move |conn| {
        conn.execute("UPDATE fin_incomes SET ledger_seq = ?2 WHERE id = ?1", params![income_id, seq]).map_err(internal)?;
        let mut v = income_rows(conn, &format!("{} WHERE id = ?1", INCOME_SQL), Some(income_id))?;
        v.pop().ok_or_else(|| internal("ingreso"))
    })
    .await?;
    state.log_activity("Finanzas", &format!("Ingreso por servicios {:.0} ({}): {}", income.amount, cfg.title, income.origin), &session.username).await;
    publish(&state);
    Ok((axum::http::StatusCode::CREATED, Json(income)))
}

/// POST /api/finance/incomes/{id}/annul
async fn annul_income(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<AnnulReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !is_staff(&session) {
        return Err(forbidden("Solo quien gestiona Finanzas anula ingresos"));
    }
    let reason = clean(&req.reason, 300, "El motivo", true)?;
    let user = session.username.clone();
    let (iid, r2) = (id.clone(), reason.clone());
    let (cfg, orig_seq, amount) = db_op_status(&state.db, move |conn| {
        let row: Option<(String, f64, Option<String>, Option<i64>)> = conn
            .query_row("SELECT project_id, amount, annulled_at, ledger_seq FROM fin_incomes WHERE id = ?1", [&iid], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()
            .map_err(internal)?;
        let (pid, amount, annulled, seq) = row.ok_or_else(|| not_found("Ingreso no encontrado"))?;
        if annulled.is_some() {
            return Err(conflict("El ingreso ya esta anulado"));
        }
        let cfg = open_config(conn, &pid)?;
        conn.execute(
            "UPDATE fin_incomes SET annulled_at = ?2, annulled_by = ?3, annul_reason = ?4 WHERE id = ?1",
            params![iid, finance::now(), user, r2],
        )
        .map_err(internal)?;
        Ok((cfg, seq, amount))
    })
    .await?;
    let data = json!({ "accion": "ingreso_anulado", "ingreso": id, "monto": amount, "motivo": reason });
    let subj = format!("ingreso: {}", cfg.title);
    let seq = match orig_seq {
        Some(s) => ledger_annul(&state, &session.username, "caja", &subj, s, data).await?,
        None => finance::ledger(&state, &session.username, "caja", &subj, data).await?,
    };
    let iid = id.clone();
    db_op_status(&state.db, move |conn| {
        conn.execute("UPDATE fin_incomes SET annul_seq = ?2 WHERE id = ?1", params![iid, seq]).map_err(internal)
    })
    .await?;
    state.log_activity("Finanzas", &format!("Ingreso anulado ({}): {}", cfg.title, reason), &session.username).await;
    publish(&state);
    Ok(Json(json!({ "ok": true, "annul_seq": seq })))
}
