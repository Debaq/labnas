//! Detector de anomalias (idea 68): solo lectura. Revisa los datos y devuelve un informe de
//! cosas que merecen una mirada humana. No bloquea nada ni concluye que algo es un fraude.

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::NaiveDate;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use super::{db, opt_text};
use crate::db::db_op_status;
use crate::error::ApiError;
use crate::state::AppState;

const DEFAULT_THRESHOLD: f64 = 1_000_000.0;
const NEW_SUPPLIER_DAYS: i64 = 60;
const SPLIT_DAYS: i64 = 30;
const DUP_DAYS: i64 = 7;
const MAX_PER_KIND: usize = 100;

#[derive(Debug, Serialize)]
pub struct Finding {
    /// duplicado, proveedor_nuevo, fraccionamiento, sin_factura, funciones, sobre_presupuesto
    pub kind: &'static str,
    /// alta, media o info
    pub severity: &'static str,
    pub title: String,
    pub detail: String,
    pub project_id: Option<String>,
    pub purchase_id: Option<String>,
    pub document_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub generated_at: String,
    /// Monto desde el que una compra pide cotizaciones
    pub threshold: f64,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Deserialize)]
pub struct Q {
    project_id: Option<String>,
}

/// Umbral de cotizaciones: lo guarda el flujo de compras en su tabla de ajustes (el nombre
/// exacto puede variar), asi que se busca sin fallar si no existe
fn threshold(conn: &Connection) -> f64 {
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'fin\\_%setting%' ESCAPE '\\'")
        .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
        .unwrap_or_default();
    const KEYS: &str = "'quote_threshold','quotes_threshold','quote_min_amount','umbral_cotizaciones','quotes_required_over','min_quotes_amount'";
    for t in tables {
        let t = t.replace('"', "");
        let v: Option<f64> = conn
            .query_row(&format!("SELECT CAST(value AS REAL) FROM \"{}\" WHERE key IN ({}) LIMIT 1", t, KEYS), [], |r| r.get(0))
            .ok()
            .or_else(|| conn.query_row(&format!("SELECT CAST(quote_threshold AS REAL) FROM \"{}\" LIMIT 1", t), [], |r| r.get(0)).ok());
        if let Some(v) = v.filter(|v| *v > 0.0) {
            return v;
        }
    }
    DEFAULT_THRESHOLD
}

fn norm_rut(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

fn day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

struct Doc {
    id: String,
    project_id: String,
    purchase_id: Option<String>,
    purchase_folio: Option<i64>,
    kind: String,
    folio: String,
    rut: String,
    name: String,
    issued_on: Option<String>,
    total: f64,
    created_at: String,
}

impl Doc {
    fn issuer(&self) -> String {
        if self.rut.is_empty() {
            self.name.trim().to_lowercase()
        } else {
            norm_rut(&self.rut)
        }
    }
    fn label(&self) -> String {
        format!("{} {}{}", if self.kind.is_empty() { "documento" } else { &self.kind }, if self.folio.is_empty() { "s/n" } else { &self.folio }, if self.name.is_empty() { String::new() } else { format!(" de {}", self.name) })
    }
    fn purchase_text(&self) -> String {
        self.purchase_folio.map(|f| format!(" (compra #{})", f)).unwrap_or_default()
    }
}

struct Purchase {
    id: String,
    folio: i64,
    project_id: String,
    item: Option<String>,
    title: String,
    amount: f64,
    status: String,
    requested_by: String,
    approved_by: Option<String>,
    received_by: Option<String>,
    created: String,
    supplier: String,
    supplier_name: String,
}

fn analyze(conn: &Connection, project: Option<String>) -> Result<Report, ApiError> {
    let threshold = threshold(conn);
    let mut f: Vec<Finding> = Vec::new();

    // Documentos
    let mut stmt = conn
        .prepare(
            "SELECT d.id, d.project_id, d.purchase_id, p.folio, d.kind, d.folio, d.issuer_rut, d.issuer_name, d.issued_on,
                    COALESCE(d.total_amount, 0), d.created_at
             FROM fin_documents d LEFT JOIN fin_purchases p ON p.id = d.purchase_id
             WHERE (?1 IS NULL OR d.project_id = ?1) ORDER BY d.created_at, d.id",
        )
        .map_err(db)?;
    let docs: Vec<Doc> = stmt
        .query_map(params![project], |r| {
            Ok(Doc {
                id: r.get(0)?,
                project_id: r.get(1)?,
                purchase_id: r.get(2)?,
                purchase_folio: r.get(3)?,
                kind: r.get(4)?,
                folio: r.get::<_, String>(5)?.trim().to_string(),
                rut: r.get::<_, String>(6)?.trim().to_string(),
                name: r.get::<_, String>(7)?.trim().to_string(),
                issued_on: r.get(8)?,
                total: r.get(9)?,
                created_at: r.get(10)?,
            })
        })
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)?;

    // 1. Facturas duplicadas: mismo RUT y folio
    let mut by_folio: BTreeMap<(String, String, String), Vec<&Doc>> = BTreeMap::new();
    for d in docs.iter().filter(|d| !d.rut.is_empty() && !d.folio.is_empty()) {
        by_folio.entry((norm_rut(&d.rut), d.kind.to_lowercase(), d.folio.to_lowercase())).or_default().push(d);
    }
    let mut dup_pairs: HashSet<(String, String)> = HashSet::new();
    for group in by_folio.values().filter(|g| g.len() > 1) {
        for a in group {
            for b in group {
                dup_pairs.insert((a.id.clone(), b.id.clone()));
            }
        }
        let first = group[0];
        let compras: Vec<String> = group.iter().filter_map(|d| d.purchase_folio.map(|f| format!("#{}", f))).collect();
        f.push(Finding {
            kind: "duplicado",
            severity: "alta",
            title: format!("{} registrado {} veces", first.label(), group.len()),
            detail: format!("Mismo RUT ({}) y mismo folio en {} documentos{}. Revise que no se haya pagado o rendido dos veces.", first.rut, group.len(), if compras.is_empty() { String::new() } else { format!(", compras {}", compras.join(", ")) }),
            project_id: Some(first.project_id.clone()),
            purchase_id: group.iter().find_map(|d| d.purchase_id.clone()),
            document_id: Some(first.id.clone()),
        });
    }
    // 1b. Mismo emisor y mismo monto en pocos dias
    let mut n_pairs = 0;
    'outer: for (i, a) in docs.iter().enumerate() {
        for b in docs.iter().skip(i + 1) {
            if a.total <= 0.0 || (a.total - b.total).abs() > 0.5 || a.issuer().is_empty() || a.issuer() != b.issuer() {
                continue;
            }
            if dup_pairs.contains(&(a.id.clone(), b.id.clone())) {
                continue;
            }
            let (Some(da), Some(db_)) = (a.issued_on.as_deref().and_then(day), b.issued_on.as_deref().and_then(day)) else { continue };
            if (da - db_).num_days().abs() > DUP_DAYS {
                continue;
            }
            f.push(Finding {
                kind: "duplicado",
                severity: "media",
                title: format!("Mismo emisor y monto en {} dias o menos: {}", DUP_DAYS, money(a.total)),
                detail: format!("{} ({}){} y {} ({}){} del mismo emisor por el mismo monto. Puede ser una factura cargada dos veces con otro folio.", a.label(), a.issued_on.as_deref().unwrap_or("?"), a.purchase_text(), b.label(), b.issued_on.as_deref().unwrap_or("?"), b.purchase_text()),
                project_id: Some(a.project_id.clone()),
                purchase_id: a.purchase_id.clone().or_else(|| b.purchase_id.clone()),
                document_id: Some(a.id.clone()),
            });
            n_pairs += 1;
            if n_pairs >= MAX_PER_KIND {
                break 'outer;
            }
        }
    }

    // 2. Proveedores nuevos: primera vez que aparece un RUT
    let today = chrono::Local::now().date_naive();
    let mut first_seen: BTreeMap<String, &Doc> = BTreeMap::new();
    for d in docs.iter().filter(|d| !d.rut.is_empty()) {
        first_seen.entry(norm_rut(&d.rut)).or_insert(d);
    }
    for d in first_seen.values() {
        if day(&d.created_at).is_some_and(|c| (today - c).num_days() <= NEW_SUPPLIER_DAYS) {
            f.push(Finding {
                kind: "proveedor_nuevo",
                severity: "info",
                title: format!("Proveedor nuevo: {}", if d.name.is_empty() { &d.rut } else { &d.name }),
                detail: format!("Primera vez que aparece el RUT {} (desde {}): {}{}. Confirme que existe y que es un proveedor legitimo.", d.rut, &d.created_at[..d.created_at.len().min(10)], d.label(), d.purchase_text()),
                project_id: Some(d.project_id.clone()),
                purchase_id: d.purchase_id.clone(),
                document_id: Some(d.id.clone()),
            });
        }
    }

    // Compras
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.folio, p.project_id, p.budget_item_id, p.title, COALESCE(p.final_amount, p.estimated_amount, 0), p.status,
                    p.requested_by, p.approved_by, p.received_by, p.created_at,
                    COALESCE(NULLIF(q.supplier_rut, ''), (SELECT d.issuer_rut FROM fin_documents d WHERE d.purchase_id = p.id AND d.issuer_rut != '' LIMIT 1), ''),
                    COALESCE(NULLIF(q.supplier, ''), (SELECT d.issuer_name FROM fin_documents d WHERE d.purchase_id = p.id AND d.issuer_name != '' LIMIT 1), '')
             FROM fin_purchases p LEFT JOIN fin_quotes q ON q.id = p.chosen_quote_id
             WHERE (?1 IS NULL OR p.project_id = ?1) ORDER BY p.created_at, p.folio",
        )
        .map_err(db)?;
    let purchases: Vec<Purchase> = stmt
        .query_map(params![project], |r| {
            Ok(Purchase {
                id: r.get(0)?,
                folio: r.get(1)?,
                project_id: r.get(2)?,
                item: r.get(3)?,
                title: r.get(4)?,
                amount: r.get(5)?,
                status: r.get(6)?,
                requested_by: r.get(7)?,
                approved_by: r.get(8)?,
                received_by: r.get(9)?,
                created: r.get(10)?,
                supplier: r.get(11)?,
                supplier_name: r.get(12)?,
            })
        })
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)?;
    let dead = |s: &str| matches!(s, "rechazada" | "anulada" | "cancelada" | "rechazado" | "anulado");

    // 3. Posible fraccionamiento: varias compras chicas que juntas pasan el umbral
    let mut groups: BTreeMap<(String, String), Vec<&Purchase>> = BTreeMap::new();
    for p in purchases.iter().filter(|p| !dead(&p.status) && p.amount > 0.0 && p.amount < threshold && day(&p.created).is_some()) {
        let key = if p.supplier.is_empty() { p.supplier_name.trim().to_lowercase() } else { norm_rut(&p.supplier) };
        if !key.is_empty() {
            groups.entry(("proveedor".into(), format!("{}|{}", p.project_id, key))).or_default().push(p);
        }
        if let Some(item) = &p.item {
            groups.entry(("item".into(), item.clone())).or_default().push(p);
        }
    }
    let mut reported: HashSet<Vec<String>> = HashSet::new();
    for ((how, _), list) in &groups {
        let n = list.len();
        for i in 0..n {
            let start = day(&list[i].created).unwrap();
            let window: Vec<&&Purchase> = list[i..].iter().take_while(|p| (day(&p.created).unwrap() - start).num_days() <= SPLIT_DAYS).collect();
            let sum: f64 = window.iter().map(|p| p.amount).sum();
            if window.len() < 2 || sum <= threshold {
                continue;
            }
            let mut ids: Vec<String> = window.iter().map(|p| p.id.clone()).collect();
            ids.sort();
            if !reported.insert(ids) {
                break;
            }
            let folios: Vec<String> = window.iter().map(|p| format!("#{} ({})", p.folio, money(p.amount))).collect();
            let who = if how == "proveedor" {
                let p = window[0];
                format!("al proveedor {}", if p.supplier_name.is_empty() { &p.supplier } else { &p.supplier_name })
            } else {
                "al mismo item de presupuesto".to_string()
            };
            f.push(Finding {
                kind: "fraccionamiento",
                severity: "media",
                title: format!("Posible fraccionamiento: {} compras {} en {} dias", window.len(), who, SPLIT_DAYS),
                detail: format!("Compras {} suman {}, sobre el umbral de cotizaciones de {}, pero cada una queda debajo. Revise que no se hayan dividido para evitar cotizar.", folios.join(", "), money(sum), money(threshold)),
                project_id: Some(window[0].project_id.clone()),
                purchase_id: Some(window[0].id.clone()),
                document_id: None,
            });
            break;
        }
    }

    // 4. Compras pagadas sin factura confirmada
    let mut stmt = conn
        .prepare(
            "SELECT p.id FROM fin_purchases p
             WHERE (?1 IS NULL OR p.project_id = ?1)
               AND (p.status LIKE 'pagad%' OR p.status LIKE 'rendid%' OR p.status LIKE 'cerrad%'
                    OR EXISTS (SELECT 1 FROM fin_purchase_steps s WHERE s.purchase_id = p.id AND s.step LIKE 'pag%'))
               AND NOT EXISTS (SELECT 1 FROM fin_documents d WHERE d.purchase_id = p.id AND d.confirmed_by IS NOT NULL)",
        )
        .map_err(db)?;
    let unpaid: HashSet<String> = stmt.query_map(params![project], |r| r.get(0)).map_err(db)?.collect::<Result<_, _>>().map_err(db)?;
    for p in purchases.iter().filter(|p| unpaid.contains(&p.id) && !dead(&p.status)) {
        f.push(Finding {
            kind: "sin_factura",
            severity: "alta",
            title: format!("Compra #{} pagada sin factura confirmada", p.folio),
            detail: format!("\"{}\" ({}) figura pagada, pero no tiene un documento confirmado por una persona. No se puede rendir sin respaldo.", p.title, money(p.amount)),
            project_id: Some(p.project_id.clone()),
            purchase_id: Some(p.id.clone()),
            document_id: None,
        });
    }

    // 5. Separacion de funciones rota
    for p in &purchases {
        let approver = p.approved_by.as_deref().filter(|s| !s.is_empty());
        if approver == Some(p.requested_by.as_str()) {
            f.push(Finding {
                kind: "funciones",
                severity: "alta",
                title: format!("Compra #{} aprobada por quien la pidio", p.folio),
                detail: format!("\"{}\": {} pidio y aprobo la compra. El sistema no deberia permitirlo; revise como ocurrio.", p.title, p.requested_by),
                project_id: Some(p.project_id.clone()),
                purchase_id: Some(p.id.clone()),
                document_id: None,
            });
        }
        if let (Some(a), Some(r)) = (approver, p.received_by.as_deref().filter(|s| !s.is_empty())) {
            if a == r {
                f.push(Finding {
                    kind: "funciones",
                    severity: "alta",
                    title: format!("Compra #{} recibida por quien la aprobo", p.folio),
                    detail: format!("\"{}\": {} aprobo y recibio la compra. Quien aprueba no deberia recibir.", p.title, a),
                    project_id: Some(p.project_id.clone()),
                    purchase_id: Some(p.id.clone()),
                    document_id: None,
                });
            }
        }
    }

    // 6. Gastos sobre lo aprobado
    let mut stmt = conn
        .prepare(
            "SELECT bi.project_id, bi.name, bi.approved,
                    COALESCE((SELECT SUM(committed) FROM fin_commitments c WHERE c.budget_item_id = bi.id), 0),
                    COALESCE((SELECT SUM(spent) FROM fin_commitments c WHERE c.budget_item_id = bi.id), 0)
             FROM fin_budget_items bi WHERE (?1 IS NULL OR bi.project_id = ?1) ORDER BY bi.project_id, bi.name",
        )
        .map_err(db)?;
    let rows: Vec<(String, String, f64, f64, f64)> =
        stmt.query_map(params![project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).map_err(db)?.collect::<Result<_, _>>().map_err(db)?;
    for (pid, name, approved, committed, spent) in rows {
        if spent > approved + 0.5 {
            f.push(Finding {
                kind: "sobre_presupuesto",
                severity: "alta",
                title: format!("Gasto sobre lo aprobado en \"{}\"", name),
                detail: format!("Aprobado {}, gastado {} (excede en {}).", money(approved), money(spent), money(spent - approved)),
                project_id: Some(pid),
                purchase_id: None,
                document_id: None,
            });
        } else if committed > approved + 0.5 {
            f.push(Finding {
                kind: "sobre_presupuesto",
                severity: "media",
                title: format!("Comprometido sobre lo aprobado en \"{}\"", name),
                detail: format!("Aprobado {}, comprometido {} (excede en {}).", money(approved), money(committed), money(committed - approved)),
                project_id: Some(pid),
                purchase_id: None,
                document_id: None,
            });
        }
    }

    let rank = |s: &str| match s {
        "alta" => 0,
        "media" => 1,
        _ => 2,
    };
    f.sort_by_key(|x| rank(x.severity));
    Ok(Report { generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(), threshold, findings: f })
}

fn money(n: f64) -> String {
    super::money_fmt(n)
}

/// GET /api/finance/anomalies?project_id= — informe de cosas a revisar (solo lectura)
pub async fn list(State(state): State<AppState>, Query(q): Query<Q>) -> Result<Json<Report>, ApiError> {
    let project = opt_text(&q.project_id);
    Ok(Json(db_op_status(&state.db, move |conn| analyze(conn, project)).await?))
}
