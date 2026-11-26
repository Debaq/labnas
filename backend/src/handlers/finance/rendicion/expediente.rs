//! Expediente por proyecto (idea 66): un ZIP con indice (HTML y CSV), todos los respaldos
//! con nombres ordenados, las huellas SHA-256 de cada archivo, los registros de la bitacora
//! que tocan al proyecto y las instrucciones para verificar sin LabNAS.

use axum::{
    extract::{Query, State},
    response::Response,
    Extension,
};
use rusqlite::{params, types::ValueRef, Connection};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;

use super::xlsx::{csv, t, Cell};
use super::{attachment, bad, db, esc_html, ext_for, not_found, project_name, safe_name, zip_files};
use crate::db::db_op_status;
use crate::error::{internal, ApiError};
use crate::state::{AppState, SessionInfo};

#[derive(Debug, Deserialize)]
pub struct Q {
    project_id: String,
}

type Table = (Vec<String>, Vec<Vec<String>>);

/// Corre una consulta con el proyecto como ?1 y devuelve encabezados y filas como texto
fn dump(conn: &Connection, sql: &str, project: &str) -> Result<Table, ApiError> {
    let mut stmt = conn.prepare(sql).map_err(db)?;
    let names: Vec<String> = stmt.column_names().map(|s| s.to_string()).collect();
    let n = names.len();
    let rows = stmt
        .query_map(params![project], |r| {
            (0..n)
                .map(|i| {
                    Ok(match r.get_ref(i)? {
                        ValueRef::Null => String::new(),
                        ValueRef::Integer(v) => v.to_string(),
                        ValueRef::Real(v) => format!("{}", v),
                        ValueRef::Text(v) => String::from_utf8_lossy(v).into_owned(),
                        ValueRef::Blob(_) => "(binario)".into(),
                    })
                })
                .collect::<rusqlite::Result<Vec<String>>>()
        })
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)?;
    Ok((names, rows))
}

const DATASETS: &[(&str, &str, &str)] = &[
    (
        "presupuesto",
        "Presupuesto aprobado",
        "SELECT id, category AS categoria, name AS item, approved AS aprobado, notes AS notas, created_by AS creado_por, created_at AS creado
         FROM fin_budget_items WHERE project_id = ?1 ORDER BY category, name",
    ),
    (
        "compromisos",
        "Comprometido y gastado",
        "SELECT source AS origen, source_id AS id_origen, budget_item_id AS item, label AS detalle, committed AS comprometido, spent AS gastado, updated_at AS actualizado
         FROM fin_commitments WHERE project_id = ?1 ORDER BY source, updated_at",
    ),
    (
        "compras",
        "Compras",
        "SELECT id, folio, budget_item_id AS item, title AS titulo, justification AS justificacion, estimated_amount AS monto_estimado,
                final_amount AS monto_final, status AS estado, requested_by AS pidio, approved_by AS aprobo, received_by AS recibio,
                chosen_quote_id AS cotizacion_elegida, choice_reason AS razon_eleccion, created_at AS creada, updated_at AS actualizada
         FROM fin_purchases WHERE project_id = ?1 ORDER BY folio",
    ),
    (
        "pasos",
        "Pasos de cada compra (quien y cuando)",
        "SELECT p.folio AS compra, s.step AS paso, s.username AS persona, s.at AS fecha, s.note AS nota, s.passkey AS con_passkey, s.ledger_seq AS registro_bitacora
         FROM fin_purchase_steps s JOIN fin_purchases p ON p.id = s.purchase_id WHERE p.project_id = ?1 ORDER BY p.folio, s.at",
    ),
    (
        "cotizaciones",
        "Cotizaciones",
        "SELECT p.folio AS compra, q.id, q.supplier AS proveedor, q.supplier_rut AS rut, q.amount AS monto, q.valid_until AS vigente_hasta,
                q.file_id AS archivo, q.notes AS notas, q.created_by AS creada_por, q.created_at AS creada
         FROM fin_quotes q JOIN fin_purchases p ON p.id = q.purchase_id WHERE p.project_id = ?1 ORDER BY p.folio, q.created_at",
    ),
    (
        "documentos",
        "Documentos (facturas, boletas)",
        "SELECT d.id, d.kind AS tipo, d.folio, d.issuer_rut AS rut_emisor, d.issuer_name AS emisor, d.issued_on AS fecha, d.net_amount AS neto,
                d.tax_amount AS iva, d.total_amount AS total, p.folio AS compra, d.file_id AS archivo, d.extracted_by AS lectura,
                d.confirmed_by AS confirmado_por, d.created_by AS creado_por, d.created_at AS creado
         FROM fin_documents d LEFT JOIN fin_purchases p ON p.id = d.purchase_id WHERE d.project_id = ?1 ORDER BY d.issued_on, d.folio",
    ),
    (
        "recepciones",
        "Recepciones",
        "SELECT p.folio AS compra, r.id, r.received_by AS recibio, r.received_at AS fecha, r.conforming AS conforme, r.notes AS notas, r.photo_file_id AS foto
         FROM fin_receptions r JOIN fin_purchases p ON p.id = r.purchase_id WHERE p.project_id = ?1 ORDER BY p.folio, r.received_at",
    ),
    (
        "horas",
        "Horas y honorarios",
        "SELECT id, username AS persona, month AS mes, hours AS horas, amount AS monto, budget_item_id AS item, status AS estado, notes AS notas,
                file_id AS archivo, created_by AS registro, decided_by AS decidio, decided_at AS fecha_decision, decision_note AS motivo
         FROM fin_hours WHERE project_id = ?1 ORDER BY month, username",
    ),
];

/// Respaldos del proyecto: (id del archivo, etiqueta de donde viene)
const FILE_SOURCES: &[&str] = &[
    "SELECT file_id, 'documento-' || CASE WHEN folio = '' THEN id ELSE folio END FROM fin_documents WHERE project_id = ?1 AND file_id IS NOT NULL",
    "SELECT q.file_id, 'cotizacion-C' || p.folio || '-' || q.supplier FROM fin_quotes q JOIN fin_purchases p ON p.id = q.purchase_id WHERE p.project_id = ?1 AND q.file_id IS NOT NULL",
    "SELECT r.photo_file_id, 'recepcion-C' || p.folio FROM fin_receptions r JOIN fin_purchases p ON p.id = r.purchase_id WHERE p.project_id = ?1 AND r.photo_file_id IS NOT NULL",
    "SELECT file_id, 'horas-' || username || '-' || month FROM fin_hours WHERE project_id = ?1 AND file_id IS NOT NULL",
];

const VERIFY_PY: &str = r#"#!/usr/bin/env python3
"""Verifica las huellas de bitacora.jsonl (registros del expediente):
   python3 verificar_bitacora.py bitacora.jsonl
Recalcula la huella de cada registro. Como el expediente trae solo los registros del
proyecto, el encadenado completo se comprueba contra la exportacion completa de la bitacora
(vea docs/BITACORA.md): el campo "hash" de cada registro aqui debe ser igual al de alla."""
import hashlib, json, sys

n = 0
for line in open(sys.argv[1], encoding="utf-8"):
    if not line.strip():
        continue
    e = json.loads(line)
    pre = json.dumps(
        [e["seq"], e["timestamp"], e["username"], e["kind"], e["subject"],
         e["annuls"], e["data"], e["prev_hash"]],
        ensure_ascii=False, separators=(",", ":"),
    )
    if hashlib.sha256(pre.encode("utf-8")).hexdigest() != e["hash"]:
        sys.exit(f"Registro {e['seq']}: el contenido no corresponde a su huella")
    n += 1
print(f"OK: {n} registros con huella correcta")
"#;

fn html_table(title: &str, (cols, rows): &Table) -> String {
    let mut h = format!("<h2>{} ({})</h2>", esc_html(title), rows.len());
    if rows.is_empty() {
        h.push_str("<p><i>Sin registros.</i></p>");
        return h;
    }
    h.push_str("<table><tr>");
    for c in cols {
        h.push_str(&format!("<th>{}</th>", esc_html(c)));
    }
    h.push_str("</tr>");
    for r in rows {
        h.push_str("<tr>");
        for v in r {
            h.push_str(&format!("<td>{}</td>", esc_html(v)));
        }
        h.push_str("</tr>");
    }
    h.push_str("</table>");
    h
}

fn table_csv((cols, rows): &Table) -> String {
    let mut all: Vec<Vec<Cell>> = vec![cols.iter().map(t).collect()];
    all.extend(rows.iter().map(|r| r.iter().map(t).collect()));
    csv(&all)
}

struct Built {
    files: Vec<(String, Vec<u8>)>,
    name: String,
    huellas_sha: String,
    n_files: usize,
    n_ledger: usize,
    alerts: Vec<String>,
}

fn build(conn: &Connection, project_id: &str) -> Result<Built, ApiError> {
    let title = project_name(conn, project_id)?.ok_or_else(|| not_found("Proyecto no encontrado"))?;
    let (responsible, closed): (String, Option<String>) = conn
        .query_row("SELECT responsible, closed_at FROM fin_projects WHERE project_id = ?1", [project_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or_default();
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut alerts = Vec::new();

    // Respaldos
    let mut seen: HashSet<String> = HashSet::new();
    let mut idx_files: Vec<(String, String, String, i64)> = Vec::new(); // ruta, nombre original, sha, tamano
    for src in FILE_SOURCES {
        let (_, rows) = dump(conn, src, project_id)?;
        for r in rows {
            let (fid, label) = (&r[0], &r[1]);
            if !seen.insert(fid.clone()) {
                continue;
            }
            let found: Option<(String, String, Vec<u8>, String)> = conn
                .query_row("SELECT name, mime, data, sha256 FROM fin_files WHERE id = ?1", [fid], |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?, x.get(3)?)))
                .ok();
            let Some((name, mime, bytes, stored)) = found else { continue };
            let real = crate::ledger::sha256_hex(&bytes);
            if real != stored {
                alerts.push(format!("ALERTA: el archivo {} ({}) no coincide con la huella guardada al subirlo", fid, name));
            }
            let stem = safe_name(name.rsplit_once('.').map(|(a, _)| a).unwrap_or(&name));
            let path = format!("respaldos/{:03}_{}_{}.{}", idx_files.len() + 1, safe_name(label), stem, ext_for(&mime, &name));
            idx_files.push((path.clone(), name, real, bytes.len() as i64));
            out.push((path, bytes));
        }
    }

    // Tablas
    let mut html = String::new();
    let mut ids: Vec<String> = vec![project_id.to_string()];
    for (base, label, sql) in DATASETS {
        let table = dump(conn, sql, project_id)?;
        if let Some(c) = table.0.iter().position(|c| c == "id") {
            ids.extend(table.1.iter().map(|r| r[c].clone()).filter(|s| s.len() >= 8));
        }
        html.push_str(&html_table(label, &table));
        out.push((format!("datos/{}.csv", base), table_csv(&table).into_bytes()));
    }

    // Bitacora relacionada
    let mut stmt = conn
        .prepare(&format!("SELECT {} FROM ledger_entries ORDER BY seq ASC", crate::ledger::COLS))
        .map_err(db)?;
    let mut ledger = String::new();
    let mut n_ledger = 0;
    for e in stmt.query_map([], crate::ledger::row_to_entry).map_err(db)? {
        let e = e.map_err(db)?;
        if ids.iter().any(|id| e.subject.contains(id.as_str()) || e.data.contains(id.as_str())) {
            ledger.push_str(&serde_json::to_string(&e).map_err(|e| internal(e.to_string()))?);
            ledger.push('\n');
            n_ledger += 1;
        }
    }
    out.push(("bitacora.jsonl".into(), ledger.into_bytes()));
    out.push(("verificar_bitacora.py".into(), VERIFY_PY.as_bytes().to_vec()));

    // Indice
    let mut idx = format!(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\"><title>Expediente</title><style>\
         body{{font:13px/1.45 system-ui,sans-serif;margin:24px;color:#111}}table{{border-collapse:collapse;margin-bottom:8px}}\
         th,td{{border:1px solid #999;padding:3px 6px;text-align:left;vertical-align:top}}th{{background:#eee}}.sha{{font:11px monospace}}</style></head><body>\
         <h1>Expediente: {}</h1><p>Responsable: {}. Estado: {}. Generado el {}. Bitacora: {} registros relacionados.</p>",
        esc_html(&title),
        esc_html(if responsible.is_empty() { "(sin indicar)" } else { &responsible }),
        if closed.is_some() { "cerrado" } else { "abierto" },
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
        n_ledger
    );
    for a in &alerts {
        idx.push_str(&format!("<p style=\"color:#b00;font-weight:700\">{}</p>", esc_html(a)));
    }
    idx.push_str(&html);
    idx.push_str(&format!("<h2>Respaldos ({})</h2>", idx_files.len()));
    idx.push_str("<table><tr><th>Archivo en el expediente</th><th>Nombre original</th><th>Bytes</th><th>SHA-256</th></tr>");
    for (p, n, s, z) in &idx_files {
        idx.push_str(&format!("<tr><td><a href=\"{p}\">{p}</a></td><td>{}</td><td>{z}</td><td class=sha>{s}</td></tr>", esc_html(n)));
    }
    idx.push_str("</table><p>Las huellas de todos los archivos estan en <b>huellas.txt</b>. Como verificar: <b>LEEME.txt</b>.</p></body></html>");
    out.insert(0, ("indice.html".into(), idx.into_bytes()));

    let leeme = format!(
        "EXPEDIENTE DE {title}\n\n\
         indice.html            indice legible (abrir en el navegador)\n\
         datos/*.csv            las mismas tablas para Excel (separador ';', UTF-8)\n\
         respaldos/             facturas, cotizaciones, actas y contratos, tal como se subieron\n\
         bitacora.jsonl         registros de la bitacora inalterable que mencionan el proyecto\n\
         huellas.txt            SHA-256 de cada archivo de este expediente\n\n\
         COMO VERIFICAR (sin LabNAS)\n\
         1. Huellas de los archivos: en esta carpeta, `sha256sum -c huellas.txt` (Linux/Mac)\n\
            o `Get-FileHash -Algorithm SHA256 <archivo>` (Windows) y comparar con huellas.txt.\n\
         2. Bitacora: `python3 verificar_bitacora.py bitacora.jsonl` recalcula la huella de cada\n\
            registro. Para comprobar el encadenado completo, pida a Finanzas la exportacion completa\n\
            de la bitacora (docs/BITACORA.md trae el script) y compare que cada \"hash\" coincida.\n\
         3. Cada respaldo nombrado en una factura o compra tiene su huella en la bitacora cuando\n\
            se subio: debe ser igual a la de huellas.txt.\n"
    );
    out.push(("LEEME.txt".into(), leeme.into_bytes()));

    // Huellas de todo lo anterior
    let mut huellas = String::new();
    for (p, b) in &out {
        huellas.push_str(&format!("{}  {}\n", crate::ledger::sha256_hex(b), p));
    }
    for a in &alerts {
        huellas.push_str(&format!("# {}\n", a));
    }
    let huellas_sha = crate::ledger::sha256_hex(huellas.as_bytes());
    let n_files = idx_files.len();
    out.push(("huellas.txt".into(), huellas.into_bytes()));
    Ok(Built { files: out, name: safe_name(&title), huellas_sha, n_files, n_ledger, alerts })
}

/// GET /api/finance/expediente?project_id= — ZIP del proyecto (nivel Ver: tambien el auditor)
pub async fn download(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<Q>,
) -> Result<Response, ApiError> {
    let pid = q.project_id.trim().to_string();
    if pid.is_empty() {
        return Err(bad("Elija un proyecto"));
    }
    let pid2 = pid.clone();
    let b = db_op_status(&state.db, move |conn| build(conn, &pid2)).await?;
    let zip = zip_files(&b.files).map_err(internal)?;
    // Queda constancia de quien descargo el expediente y con que huellas
    let _ = crate::finance::ledger(
        &state,
        &session.username,
        "expediente",
        &pid,
        json!({ "archivos": b.n_files, "registros_bitacora": b.n_ledger, "huellas_sha256": b.huellas_sha, "alertas": b.alerts.len() }),
    )
    .await;
    state.log_activity("Finanzas", &format!("Expediente descargado: {}", b.name), &session.username).await;
    attachment(zip, "application/zip", &format!("expediente-{}-{}.zip", b.name, chrono::Local::now().format("%Y%m%d")))
}
