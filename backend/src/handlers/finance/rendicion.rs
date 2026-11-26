//! Rendiciones, expediente exportable, manual de procedimientos, horas por proyecto y
//! detector de anomalias (ideas 61, 62, 66, 67, 68). Ver docs/FINANZAS.md.
//!
//! Todo lo de lectura (rendicion, expediente, anomalias, manual) es nivel Ver: asi un auditor
//! con cuenta que vence puede revisar y descargar sin poder cambiar nada.

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use chrono::NaiveDate;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::db_op_status;
use crate::error::{internal, ApiError};
use crate::state::{AppState, SessionInfo};

mod anomalias;
mod expediente;
mod horas;
mod manual;
pub mod xlsx;

use xlsx::{t, Cell, Sheet};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/finance/rendicion", get(report))
        .route("/api/finance/rendicion/export", get(export))
        .route("/api/finance/rendicion/print", get(print))
        .route("/api/finance/expediente", get(expediente::download))
        .route("/api/finance/anomalies", get(anomalias::list))
        .route("/api/finance/manual", get(manual::current))
        .route("/api/finance/manual/versions", post(manual::publish))
        .route("/api/finance/manual/versions/{id}", get(manual::version))
        .route("/api/finance/manual/sign", post(manual::sign))
        .route("/api/finance/manual/signatures", get(manual::signatures))
        .route("/api/finance/hours", get(horas::list).post(horas::create))
        .route("/api/finance/hours/summary", get(horas::summary))
        .route("/api/finance/hours/{id}/approve", post(horas::approve))
        .route("/api/finance/hours/{id}/reject", post(horas::reject))
        .route("/api/finance/hours/{id}/annul", post(horas::annul))
}

// ─── Ayudas compartidas ───

pub(super) fn bad(m: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, m.into())
}
pub(super) fn forbidden(m: impl Into<String>) -> ApiError {
    (StatusCode::FORBIDDEN, m.into())
}
pub(super) fn not_found(m: impl Into<String>) -> ApiError {
    (StatusCode::NOT_FOUND, m.into())
}
pub(super) fn conflict(m: impl Into<String>) -> ApiError {
    (StatusCode::CONFLICT, m.into())
}
pub(super) fn db(e: rusqlite::Error) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("Base de datos: {}", e))
}

/// Texto recortado con largo maximo
pub(super) fn clean(s: &str, max: usize, what: &str) -> Result<String, ApiError> {
    let s = s.trim().to_string();
    if s.chars().count() > max {
        return Err(bad(format!("{} demasiado largo (maximo {} caracteres)", what, max)));
    }
    Ok(s)
}

pub(super) fn opt_text(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

pub(super) fn parse_date(s: &str, what: &str) -> Result<String, ApiError> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map(|d| d.format("%Y-%m-%d").to_string())
        .map_err(|_| bad(format!("{}: fecha invalida (AAAA-MM-DD)", what)))
}

pub(super) fn opt_date(s: &Option<String>, what: &str) -> Result<Option<String>, ApiError> {
    opt_text(s).map(|d| parse_date(&d, what)).transpose()
}

pub(super) fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Escapa lo que Telegram (Markdown) toma como formato
pub(super) fn esc_md(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '_' | '*' | '`' | '[') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

pub(super) fn project_name(conn: &Connection, id: &str) -> Result<Option<String>, ApiError> {
    crate::finance::project_title(conn, id).map_err(db)
}

/// Nombre de archivo seguro para cabeceras y para dentro de un zip
pub(super) fn safe_name(s: &str) -> String {
    let n: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else if c == ' ' { '_' } else { '-' })
        .collect();
    let n = n.trim_matches(|c| c == '-' || c == '.').to_string();
    let n: String = n.chars().take(60).collect();
    if n.is_empty() {
        "archivo".into()
    } else {
        n
    }
}

pub(super) fn attachment(bytes: Vec<u8>, content_type: &str, filename: &str) -> Result<Response, ApiError> {
    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", safe_name_keep_ext(filename)))
        .header(header::CACHE_CONTROL, "private, no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(bytes))
        .map_err(internal)
}

fn safe_name_keep_ext(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect()
}

pub(super) fn zip_files(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            z.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
            z.write_all(data).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

/// Extension a partir del tipo MIME de un respaldo
pub(super) fn ext_for(mime: &str, name: &str) -> &'static str {
    match mime {
        "application/pdf" => "pdf",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "application/xml" => "xml",
        m if m.contains("spreadsheetml") => "xlsx",
        m if m.contains("wordprocessingml") => "docx",
        _ => {
            let _ = name;
            "bin"
        }
    }
}

// ─── Rendicion (idea 61) ───

#[derive(Debug, Deserialize)]
pub struct RendQuery {
    project_id: Option<String>,
    from: Option<String>,
    to: Option<String>,
    /// xlsx (por defecto), csv o zip (xlsx + respaldos)
    format: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocRow {
    pub id: String,
    pub project_id: String,
    pub project: String,
    pub kind: String,
    pub folio: String,
    pub issuer_rut: String,
    pub issuer_name: String,
    pub issued_on: Option<String>,
    pub net: Option<f64>,
    pub tax: Option<f64>,
    pub total: Option<f64>,
    pub budget_item_id: Option<String>,
    pub budget_item: String,
    pub purchase_id: Option<String>,
    pub purchase_folio: Option<i64>,
    pub purchase_title: String,
    pub file_id: Option<String>,
    pub file_name: Option<String>,
    pub file_mime: Option<String>,
    pub file_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemRow {
    pub id: Option<String>,
    pub project_id: String,
    pub project: String,
    pub category: String,
    pub name: String,
    pub approved: f64,
    pub committed: f64,
    pub spent: f64,
    /// Suma de los documentos confirmados del periodo contra este item
    pub documented: f64,
    pub available: f64,
    pub over: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rendicion {
    pub project_id: Option<String>,
    pub project: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub generated_at: String,
    pub documents: Vec<DocRow>,
    pub items: Vec<ItemRow>,
    pub total_net: f64,
    pub total_tax: f64,
    pub total: f64,
    pub total_approved: f64,
    pub total_spent: f64,
}

struct Filter {
    project_id: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

fn filter_of(q: &RendQuery) -> Result<Filter, ApiError> {
    let f = Filter { project_id: opt_text(&q.project_id), from: opt_date(&q.from, "Desde")?, to: opt_date(&q.to, "Hasta")? };
    if let (Some(a), Some(b)) = (&f.from, &f.to) {
        if a > b {
            return Err(bad("'Desde' no puede ser posterior a 'Hasta'"));
        }
    }
    Ok(f)
}

fn load(conn: &Connection, f: &Filter) -> Result<Rendicion, ApiError> {
    let project = match &f.project_id {
        Some(id) => project_name(conn, id)?.ok_or_else(|| not_found("Proyecto no encontrado"))?,
        None => "Todos los proyectos".to_string(),
    };
    let mut stmt = conn
        .prepare(
            "SELECT d.id, d.project_id, COALESCE(pe.title, ''), d.kind, d.folio, d.issuer_rut, d.issuer_name, d.issued_on,
                    d.net_amount, d.tax_amount, d.total_amount, p.budget_item_id, COALESCE(b.name, ''),
                    d.purchase_id, p.folio, COALESCE(p.title, ''), d.file_id, fl.name, fl.mime, fl.sha256
             FROM fin_documents d
             LEFT JOIN fin_purchases p ON p.id = d.purchase_id
             LEFT JOIN fin_budget_items b ON b.id = p.budget_item_id
             LEFT JOIN portfolio_entries pe ON pe.id = d.project_id
             LEFT JOIN fin_files fl ON fl.id = d.file_id
             WHERE d.confirmed_by IS NOT NULL
               AND (?1 IS NULL OR d.project_id = ?1)
               AND (?2 IS NULL OR d.issued_on >= ?2)
               AND (?3 IS NULL OR d.issued_on <= ?3)
             ORDER BY d.issued_on, d.folio, d.id",
        )
        .map_err(db)?;
    let documents: Vec<DocRow> = stmt
        .query_map(params![f.project_id, f.from, f.to], |r| {
            Ok(DocRow {
                id: r.get(0)?,
                project_id: r.get(1)?,
                project: r.get(2)?,
                kind: r.get(3)?,
                folio: r.get(4)?,
                issuer_rut: r.get(5)?,
                issuer_name: r.get(6)?,
                issued_on: r.get(7)?,
                net: r.get(8)?,
                tax: r.get(9)?,
                total: r.get(10)?,
                budget_item_id: r.get(11)?,
                budget_item: r.get(12)?,
                purchase_id: r.get(13)?,
                purchase_folio: r.get(14)?,
                purchase_title: r.get(15)?,
                file_id: r.get(16)?,
                file_name: r.get(17)?,
                file_mime: r.get(18)?,
                file_sha256: r.get(19)?,
            })
        })
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)?;

    let mut stmt = conn
        .prepare(
            "SELECT bi.id, bi.project_id, COALESCE(pe.title, ''), bi.category, bi.name, bi.approved,
                    COALESCE((SELECT SUM(committed) FROM fin_commitments c WHERE c.budget_item_id = bi.id), 0),
                    COALESCE((SELECT SUM(spent) FROM fin_commitments c WHERE c.budget_item_id = bi.id), 0)
             FROM fin_budget_items bi LEFT JOIN portfolio_entries pe ON pe.id = bi.project_id
             WHERE (?1 IS NULL OR bi.project_id = ?1)
             ORDER BY bi.project_id, bi.category, bi.name",
        )
        .map_err(db)?;
    let mut items: Vec<ItemRow> = stmt
        .query_map(params![f.project_id], |r| {
            Ok(ItemRow {
                id: r.get(0)?,
                project_id: r.get(1)?,
                project: r.get(2)?,
                category: r.get(3)?,
                name: r.get(4)?,
                approved: r.get(5)?,
                committed: r.get(6)?,
                spent: r.get(7)?,
                documented: 0.0,
                available: 0.0,
                over: false,
            })
        })
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)?;
    // Gastos que no se cargaron a ningun item
    let (c, s): (f64, f64) = conn
        .query_row(
            "SELECT COALESCE(SUM(committed), 0), COALESCE(SUM(spent), 0) FROM fin_commitments
             WHERE budget_item_id IS NULL AND (?1 IS NULL OR project_id = ?1)",
            params![f.project_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(db)?;
    if c != 0.0 || s != 0.0 {
        items.push(ItemRow {
            id: None,
            project_id: f.project_id.clone().unwrap_or_default(),
            project: project.clone(),
            category: "otro".into(),
            name: "Sin item de presupuesto".into(),
            approved: 0.0,
            committed: c,
            spent: s,
            documented: 0.0,
            available: 0.0,
            over: false,
        });
    }
    for it in &mut items {
        it.documented = documents.iter().filter(|d| d.budget_item_id.is_some() && d.budget_item_id == it.id).filter_map(|d| d.total).sum();
        it.available = it.approved - it.spent;
        it.over = it.id.is_some() && it.spent > it.approved + 0.5;
    }
    Ok(Rendicion {
        project_id: f.project_id.clone(),
        project,
        from: f.from.clone(),
        to: f.to.clone(),
        generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        total_net: documents.iter().filter_map(|d| d.net).sum(),
        total_tax: documents.iter().filter_map(|d| d.tax).sum(),
        total: documents.iter().filter_map(|d| d.total).sum(),
        total_approved: items.iter().map(|i| i.approved).sum(),
        total_spent: items.iter().map(|i| i.spent).sum(),
        documents,
        items,
    })
}

/// GET /api/finance/rendicion — gastos con documento y ejecucion por item
pub async fn report(State(state): State<AppState>, Query(q): Query<RendQuery>) -> Result<Json<Rendicion>, ApiError> {
    let f = filter_of(&q)?;
    Ok(Json(db_op_status(&state.db, move |conn| load(conn, &f)).await?))
}

fn num(v: Option<f64>) -> Cell {
    v.map(Cell::N).unwrap_or_else(|| t(""))
}

pub(super) fn doc_rows(r: &Rendicion) -> Vec<Vec<Cell>> {
    let mut rows = vec![vec![
        t("Fecha"), t("Tipo"), t("Folio"), t("RUT emisor"), t("Emisor"), t("Neto"), t("IVA"), t("Total"),
        t("Item de presupuesto"), t("Compra"), t("Proyecto"), t("Respaldo"), t("SHA-256 del respaldo"),
    ]];
    for d in &r.documents {
        let compra = match (d.purchase_folio, d.purchase_title.as_str()) {
            (Some(f), title) => format!("#{} {}", f, title),
            _ => String::new(),
        };
        rows.push(vec![
            t(d.issued_on.clone().unwrap_or_default()), t(&d.kind), t(&d.folio), t(&d.issuer_rut), t(&d.issuer_name),
            num(d.net), num(d.tax), num(d.total), t(&d.budget_item), t(compra), t(&d.project),
            t(d.file_name.clone().unwrap_or_default()), t(d.file_sha256.clone().unwrap_or_default()),
        ]);
    }
    rows.push(vec![t("TOTAL"), t(""), t(""), t(""), t(""), Cell::N(r.total_net), Cell::N(r.total_tax), Cell::N(r.total)]);
    rows
}

pub(super) fn item_rows(r: &Rendicion) -> Vec<Vec<Cell>> {
    let mut rows = vec![vec![t("Proyecto"), t("Categoria"), t("Item"), t("Aprobado"), t("Comprometido"), t("Gastado"), t("Disponible"), t("Documentado en el periodo"), t("Estado")]];
    for i in &r.items {
        rows.push(vec![
            t(&i.project), t(&i.category), t(&i.name), Cell::N(i.approved), Cell::N(i.committed), Cell::N(i.spent),
            Cell::N(i.available), Cell::N(i.documented), t(if i.over { "SOBRE LO APROBADO" } else { "" }),
        ]);
    }
    rows.push(vec![t("TOTAL"), t(""), t(""), Cell::N(r.total_approved), t(""), Cell::N(r.total_spent), Cell::N(r.total_approved - r.total_spent)]);
    rows
}

fn period_label(r: &Rendicion) -> String {
    match (&r.from, &r.to) {
        (Some(a), Some(b)) => format!("{} a {}", a, b),
        (Some(a), None) => format!("desde {}", a),
        (None, Some(b)) => format!("hasta {}", b),
        _ => "todo el periodo".into(),
    }
}

/// GET /api/finance/rendicion/export — .xlsx, .csv (con BOM, ';') o .zip (xlsx + respaldos)
pub async fn export(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<RendQuery>,
) -> Result<Response, ApiError> {
    let f = filter_of(&q)?;
    let format = q.format.clone().unwrap_or_else(|| "xlsx".into());
    if !matches!(format.as_str(), "xlsx" | "csv" | "zip") {
        return Err(bad("Formato invalido (xlsx, csv o zip)"));
    }
    let with_files = format == "zip";
    let (r, files) = db_op_status(&state.db, move |conn| {
        let r = load(conn, &f)?;
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        if with_files {
            for (i, d) in r.documents.iter().enumerate() {
                let Some(fid) = &d.file_id else { continue };
                let data: Option<(String, String, Vec<u8>)> = conn
                    .query_row("SELECT name, mime, data FROM fin_files WHERE id = ?1", [fid], |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?)))
                    .ok();
                if let Some((name, mime, bytes)) = data {
                    let ext = ext_for(&mime, &name);
                    let stem = safe_name(name.rsplit_once('.').map(|(a, _)| a).unwrap_or(&name));
                    files.push((format!("respaldos/{:03}_{}_{}.{}", i + 1, safe_name(&d.folio), stem, ext), bytes));
                }
            }
        }
        Ok((r, files))
    })
    .await?;
    let stem = format!("rendicion-{}-{}", safe_name(&r.project), chrono::Local::now().format("%Y%m%d"));
    state.log_activity("Finanzas", &format!("Rendicion exportada ({}): {}, {}", format, r.project, period_label(&r)), &session.username).await;
    let docs = doc_rows(&r);
    let items = item_rows(&r);
    match format.as_str() {
        "csv" => {
            let mut all = docs;
            all.push(vec![]);
            all.extend(items);
            attachment(xlsx::csv(&all).into_bytes(), "text/csv; charset=utf-8", &format!("{}.csv", stem))
        }
        _ => {
            let book = xlsx::build(&[
                Sheet { name: "Gastos con documento".into(), rows: docs },
                Sheet { name: "Ejecucion por item".into(), rows: items },
                Sheet {
                    name: "Datos".into(),
                    rows: vec![
                        vec![t("Proyecto"), t("Periodo"), t("Generado")],
                        vec![t(&r.project), t(period_label(&r)), t(&r.generated_at)],
                    ],
                },
            ])
            .map_err(internal)?;
            if with_files {
                let mut all = vec![(format!("{}.xlsx", stem), book)];
                all.extend(files);
                attachment(zip_files(&all).map_err(internal)?, "application/zip", &format!("{}.zip", stem))
            } else {
                attachment(book, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", &format!("{}.xlsx", stem))
            }
        }
    }
}

pub(super) fn money_fmt(n: f64) -> String {
    let s = format!("{}", n.round() as i64);
    let (neg, digits) = s.strip_prefix('-').map(|d| (true, d)).unwrap_or((false, &s));
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    format!("{}${}", if neg { "-" } else { "" }, out)
}

fn money_opt(n: Option<f64>) -> String {
    n.map(money_fmt).unwrap_or_default()
}

use money_fmt as money;

/// GET /api/finance/rendicion/print — vista imprimible (guardar como PDF desde el navegador)
pub async fn print(State(state): State<AppState>, Query(q): Query<RendQuery>) -> Result<Response, ApiError> {
    let f = filter_of(&q)?;
    let r = db_op_status(&state.db, move |conn| load(conn, &f)).await?;
    let mut h = String::from(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\"><title>Rendicion</title><style>\
         body{font:12px/1.4 system-ui,sans-serif;margin:24px;color:#111}h1{font-size:18px;margin:0 0 4px}h2{font-size:14px;margin:20px 0 6px}\
         table{border-collapse:collapse;width:100%}th,td{border:1px solid #999;padding:3px 6px;text-align:left;vertical-align:top}\
         th{background:#eee}td.n{text-align:right;white-space:nowrap}.over{color:#b00;font-weight:700}.sha{font:10px monospace;word-break:break-all}\
         @media print{button{display:none}}</style></head><body>",
    );
    h.push_str(&format!(
        "<button onclick=\"window.print()\">Imprimir / guardar como PDF</button><h1>Rendicion: {}</h1><p>Periodo: {}. Generada el {}.</p>",
        esc_html(&r.project),
        esc_html(&period_label(&r)),
        esc_html(&r.generated_at)
    ));
    h.push_str("<h2>Gastos con documento</h2><table><tr><th>Fecha</th><th>Tipo</th><th>Folio</th><th>RUT</th><th>Emisor</th><th>Neto</th><th>IVA</th><th>Total</th><th>Item</th><th>Compra</th><th>Respaldo (SHA-256)</th></tr>");
    for d in &r.documents {
        let compra = d.purchase_folio.map(|f| format!("#{} {}", f, d.purchase_title)).unwrap_or_default();
        h.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=n>{}</td><td class=n>{}</td><td class=n>{}</td><td>{}</td><td>{}</td><td class=sha>{}</td></tr>",
            esc_html(d.issued_on.as_deref().unwrap_or("")), esc_html(&d.kind), esc_html(&d.folio), esc_html(&d.issuer_rut),
            esc_html(&d.issuer_name), money_opt(d.net), money_opt(d.tax), money_opt(d.total), esc_html(&d.budget_item),
            esc_html(&compra), esc_html(d.file_sha256.as_deref().unwrap_or(""))
        ));
    }
    h.push_str(&format!(
        "<tr><th colspan=5>Total</th><th class=n>{}</th><th class=n>{}</th><th class=n>{}</th><th colspan=3></th></tr></table>",
        money(r.total_net), money(r.total_tax), money(r.total)
    ));
    h.push_str("<h2>Ejecucion por item</h2><table><tr><th>Proyecto</th><th>Categoria</th><th>Item</th><th>Aprobado</th><th>Comprometido</th><th>Gastado</th><th>Disponible</th><th>Documentado</th></tr>");
    for i in &r.items {
        h.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td class=n>{}</td><td class=n>{}</td><td class=\"n{}\">{}</td><td class=n>{}</td><td class=n>{}</td></tr>",
            esc_html(&i.project), esc_html(&i.category), esc_html(&i.name), money(i.approved), money(i.committed),
            if i.over { " over" } else { "" }, money(i.spent), money(i.available), money(i.documented)
        ));
    }
    h.push_str(&format!("<tr><th colspan=3>Total</th><th class=n>{}</th><th></th><th class=n>{}</th><th class=n>{}</th><th></th></tr></table></body></html>",
        money(r.total_approved), money(r.total_spent), money(r.total_approved - r.total_spent)));
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from(h))
        .map_err(internal)
}
