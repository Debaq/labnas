//! Finanzas: rendicion, expediente, anomalias, manual con firmas y horas por proyecto
//! (handlers::finance::rendicion). Los datos de compras y documentos se insertan por SQL
//! directo en las tablas de la migracion 37.

use std::io::Read;

use base64::Engine;
use serde_json::json;

use super::harness::Server;

const PDF1: &[u8] = b"%PDF-1.4 factura uno";
const PDF2: &[u8] = b"%PDF-1.4 contrato honorarios";

async fn bytes(s: &Server, path: &str, token: &str) -> (u16, Vec<u8>) {
    let r = s.client().get(format!("{}{}", s.base, path)).bearer_auth(token).send().await.unwrap();
    (r.status().as_u16(), r.bytes().await.unwrap().to_vec())
}

fn unzip(data: Vec<u8>) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).expect("zip");
    let mut out = std::collections::BTreeMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        out.insert(f.name().unwrap().to_string(), b);
    }
    out
}

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

async fn sql<F, T>(s: &Server, f: F) -> T
where
    F: FnOnce(&rusqlite::Connection) -> T + Send + 'static,
    T: Send + 'static,
{
    crate::db::db_op(&s.state.db, move |c| Ok(f(c))).await.unwrap()
}

/// Proyecto "p1" con presupuesto, compras, documentos y respaldo insertados a mano
async fn seed(s: &Server) {
    sql(s, |c| {
        let now = chrono::Utc::now().to_rfc3339();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        c.execute("INSERT INTO portfolio_entries (id, title, created_at) VALUES ('p1', 'FONDEF Fablab', ?1)", [&now]).unwrap();
        c.execute("INSERT INTO fin_projects (project_id, responsible, created_by, created_at) VALUES ('p1', 'doc', 'adm', ?1)", [&now]).unwrap();
        c.execute("INSERT INTO fin_budget_items (id, project_id, category, name, approved, created_by, created_at) VALUES ('bi1', 'p1', 'equipos', 'Equipos CNC', 500000, 'adm', ?1)", [&now]).unwrap();
        c.execute("INSERT INTO fin_commitments (source, source_id, project_id, budget_item_id, label, committed, spent, updated_at) VALUES ('compra', 'c1', 'p1', 'bi1', 'Fresas', 600000, 600000, ?1)", [&now]).unwrap();
        let f1 = crate::finance::store_file(c, "factura100.pdf", PDF1, "application/pdf", "adm").unwrap();
        // Compra 1: normal, con factura 100 (y su duplicada)
        let purchase = |id: &str, folio: i64, status: &str, req: &str, appr: Option<&str>, rec: Option<&str>, amount: f64, quote: Option<&str>| {
            c.execute(
                "INSERT INTO fin_purchases (id, folio, project_id, budget_item_id, title, estimated_amount, status, requested_by, approved_by, received_by, chosen_quote_id, created_at, updated_at)
                 VALUES (?1, ?2, 'p1', 'bi1', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                rusqlite::params![id, folio, format!("Compra {}", folio), amount, status, req, appr, rec, quote, format!("{}T10:00:00Z", today)],
            )
            .unwrap();
        };
        purchase("c1", 1, "pagada", "doc", Some("adm"), Some("op"), 119000.0, None);
        // Tres compras chicas al mismo proveedor que juntas pasan el millon
        for (i, q) in [("c2", "q2"), ("c3", "q3"), ("c4", "q4")].iter().enumerate() {
            purchase(q.0, 2 + i as i64, "aprobada", "doc", Some("adm"), None, 400000.0, Some(q.1));
            c.execute("INSERT INTO fin_quotes (id, purchase_id, supplier, supplier_rut, amount, created_by, created_at) VALUES (?1, ?2, 'Maderas Sur', '77.000.111-2', 400000, 'doc', ?3)", rusqlite::params![q.1, q.0, now]).unwrap();
        }
        // Pagada sin factura, y una aprobada por quien la pidio
        purchase("c5", 5, "pagada", "doc", Some("adm"), Some("op"), 50000.0, None);
        purchase("c6", 6, "aprobada", "adm", Some("adm"), None, 10000.0, None);
        let doc = |id: &str, folio: &str, rut: &str, name: &str, date: &str, total: f64, purchase: Option<&str>, file: Option<&str>, confirmed: Option<&str>| {
            c.execute(
                "INSERT INTO fin_documents (id, project_id, purchase_id, kind, folio, issuer_rut, issuer_name, issued_on, net_amount, tax_amount, total_amount, file_id, confirmed_by, created_by, created_at)
                 VALUES (?1, 'p1', ?2, 'factura', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'adm', ?12)",
                rusqlite::params![id, purchase, folio, rut, name, date, (total / 1.19).round(), total - (total / 1.19).round(), total, file, confirmed, now],
            )
            .unwrap();
        };
        doc("d1", "100", "76.123.456-7", "Ferreteria Central", "2026-03-10", 119000.0, Some("c1"), Some(&f1.id), Some("adm"));
        doc("d2", "100", "76123456-7", "Ferreteria Central", "2026-03-12", 119000.0, None, None, Some("adm"));
        doc("d3", "200", "11.111.111-1", "Libreria Uno", "2026-03-01", 50000.0, None, None, Some("adm"));
        doc("d4", "201", "11.111.111-1", "Libreria Uno", "2026-03-05", 50000.0, None, None, Some("adm"));
        doc("d5", "300", "22.222.222-2", "Sin confirmar", "2026-03-06", 9999.0, None, None, None);
        doc("d6", "400", "33.333.333-3", "Fuera de periodo", "2026-05-06", 1000.0, None, None, Some("adm"));
        f1.sha256
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rendicion_expediente_y_anomalias() {
    let s = Server::start().await;
    let admin = s.admin().await;
    s.user(&admin, "adm", "administrativo").await;
    let doc = s.user(&admin, "doc", "docente").await;
    let aud = s.user(&admin, "aud", "auditor").await;
    let est = s.user(&admin, "est", "estudiante").await;
    seed(&s).await;
    let sha_pdf = crate::ledger::sha256_hex(PDF1);

    // Sin acceso a Finanzas no se ve nada; el auditor (Ver) si
    let q = "project_id=p1&from=2026-03-01&to=2026-03-31";
    assert_eq!(s.get(&format!("/api/finance/rendicion?{}", q), &est).await.status, 403);
    assert_eq!(s.get("/api/finance/expediente?project_id=p1", &est).await.status, 403);
    let r = s.get(&format!("/api/finance/rendicion?{}", q), &aud).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let rend = r.json();
    assert_eq!(rend["documents"].as_array().unwrap().len(), 4, "solo confirmados y dentro del periodo");
    assert_eq!(rend["total"], 119000.0 * 2.0 + 100000.0);
    assert_eq!(rend["documents"][2]["file_sha256"], sha_pdf.as_str(), "el d1 (10 de marzo) lleva su respaldo");
    let item = &rend["items"][0];
    assert_eq!((item["approved"].as_f64(), item["spent"].as_f64(), item["over"].as_bool()), (Some(500000.0), Some(600000.0), Some(true)));
    assert_eq!(s.get("/api/finance/rendicion?project_id=nope", &aud).await.status, 404);
    assert_eq!(s.get("/api/finance/rendicion?from=hoy", &aud).await.status, 400);
    assert_eq!(s.get("/api/finance/rendicion?from=2026-04-01&to=2026-03-01", &aud).await.status, 400);

    // Excel: un zip con las hojas
    let (st, x) = bytes(&s, &format!("/api/finance/rendicion/export?{}", q), &aud).await;
    assert_eq!(st, 200);
    let files = unzip(x);
    let sheet = String::from_utf8(files["xl/worksheets/sheet1.xml"].clone()).unwrap();
    assert!(sheet.contains("76.123.456-7") && sheet.contains("Ferreteria Central") && sheet.contains(&sha_pdf));
    assert!(String::from_utf8(files["xl/worksheets/sheet2.xml"].clone()).unwrap().contains("Equipos CNC"));
    // CSV con BOM y ';'
    let (_, c) = bytes(&s, &format!("/api/finance/rendicion/export?{}&format=csv", q), &aud).await;
    assert!(c.starts_with(&[0xEF, 0xBB, 0xBF]));
    let c = String::from_utf8(c).unwrap();
    assert!(c.contains("76.123.456-7;Ferreteria Central;100000;19000;119000"));
    // ZIP con respaldos anexos
    let (_, z) = bytes(&s, &format!("/api/finance/rendicion/export?{}&format=zip", q), &aud).await;
    let files = unzip(z);
    assert!(files.keys().any(|k| k.ends_with(".xlsx")));
    let (_, pdf) = files.iter().find(|(k, _)| k.starts_with("respaldos/")).expect("respaldo anexo");
    assert_eq!(pdf, PDF1);
    assert_eq!(s.get(&format!("/api/finance/rendicion/export?{}&format=exe", q), &aud).await.status, 400);
    // Vista imprimible
    let p = s.get(&format!("/api/finance/rendicion/print?{}", q), &aud).await;
    assert!(p.header("content-type").starts_with("text/html") && p.text.contains("Ferreteria Central") && p.text.contains("$119.000"));

    // Expediente: huellas correctas para cada archivo; lo ve y descarga el auditor
    assert_eq!(s.get("/api/finance/expediente", &aud).await.status, 400);
    let (st, z) = bytes(&s, "/api/finance/expediente?project_id=p1", &aud).await;
    assert_eq!(st, 200);
    let files = unzip(z);
    for k in ["indice.html", "huellas.txt", "LEEME.txt", "bitacora.jsonl", "datos/compras.csv", "datos/documentos.csv", "datos/pasos.csv"] {
        assert!(files.contains_key(k), "falta {}", k);
    }
    let huellas = String::from_utf8(files["huellas.txt"].clone()).unwrap();
    let mut checked = 0;
    for line in huellas.lines().filter(|l| !l.starts_with('#')) {
        let (hash, path) = line.split_once("  ").unwrap();
        assert_eq!(hash, crate::ledger::sha256_hex(&files[path]), "huella de {}", path);
        checked += 1;
    }
    assert_eq!(checked, files.len() - 1, "todos menos huellas.txt");
    assert!(huellas.contains(&sha_pdf), "la huella del respaldo");
    assert!(files.keys().any(|k| k.starts_with("respaldos/001_documento-100")));
    // Quien lo descargo queda en la bitacora
    let n: i64 = sql(&s, |c| c.query_row("SELECT COUNT(*) FROM ledger_entries WHERE kind = 'expediente'", [], |r| r.get(0)).unwrap()).await;
    assert_eq!(n, 1);
    assert_eq!(s.get("/api/finance/expediente?project_id=nope", &aud).await.status, 404);

    // Anomalias
    let a = s.get("/api/finance/anomalies?project_id=p1", &aud).await.json();
    assert_eq!(a["threshold"], 1000000.0);
    let find = |kind: &str| a["findings"].as_array().unwrap().iter().filter(|f| f["kind"] == kind).cloned().collect::<Vec<_>>();
    let dup = find("duplicado");
    assert!(dup.iter().any(|f| f["severity"] == "alta" && f["detail"].as_str().unwrap().contains("mismo folio")), "RUT+folio");
    assert!(dup.iter().any(|f| f["severity"] == "media" && f["title"].as_str().unwrap().contains("Mismo emisor")), "mismo emisor y monto");
    let frac = find("fraccionamiento");
    assert!(frac.iter().any(|f| f["title"].as_str().unwrap().contains("proveedor Maderas Sur") && f["detail"].as_str().unwrap().contains("$1.200.000") && f["purchase_id"].is_string()));
    assert!(find("sin_factura").iter().any(|f| f["purchase_id"] == "c5"));
    assert!(!find("sin_factura").iter().any(|f| f["purchase_id"] == "c1"), "c1 tiene factura confirmada");
    assert!(find("funciones").iter().any(|f| f["purchase_id"] == "c6"));
    assert!(find("sobre_presupuesto").iter().any(|f| f["title"].as_str().unwrap().contains("Equipos CNC")));
    assert!(!find("proveedor_nuevo").is_empty());
    // Todo lo anterior es solo lectura: sin acceso no hay informe
    assert_eq!(s.get("/api/finance/anomalies", &est).await.status, 403);
    // Una persona con "usar" tambien lo ve
    assert_eq!(s.get("/api/finance/anomalies", &doc).await.status, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_con_versiones_y_firmas() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let adm = s.user(&admin, "adm", "administrativo").await;
    let doc = s.user(&admin, "doc", "docente").await;
    let aud = s.user(&admin, "aud", "auditor").await;
    let est = s.user(&admin, "est", "estudiante").await;

    let vacio = s.get("/api/finance/manual", &aud).await.json();
    assert!(vacio["current"].is_null());
    assert_eq!(s.post("/api/finance/manual/sign", &aud, json!({})).await.status, 404);

    // Publicar: solo quien gestiona
    let texto = "# Manual\n\n1. Toda compra se pide, se aprueba y se recibe por personas distintas.";
    assert_eq!(s.post("/api/finance/manual/versions", &doc, json!({ "body": texto })).await.status, 403);
    assert_eq!(s.post("/api/finance/manual/versions", &adm, json!({ "body": "corto" })).await.status, 400);
    let r = s.post("/api/finance/manual/versions", &adm, json!({ "body": texto })).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let v1 = r.json();
    assert_eq!((v1["version"].as_i64(), v1["sha256"].as_str()), (Some(1), Some(crate::ledger::sha256_hex(texto.as_bytes()).as_str())));
    assert!(v1["ledger_seq"].as_i64().is_some());
    // Mismo texto: no es una version nueva
    assert_eq!(s.post("/api/finance/manual/versions", &adm, json!({ "body": texto, "summary": "x" })).await.status, 409);

    // Firmar: sin acceso no; el auditor (Ver) si, una sola vez
    assert_eq!(s.post("/api/finance/manual/sign", &est, json!({})).await.status, 403);
    let r = s.post("/api/finance/manual/sign", &aud, json!({ "version_id": v1["id"] })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(s.post("/api/finance/manual/sign", &aud, json!({})).await.status, 409);
    let st = s.get("/api/finance/manual/signatures", &aud).await.json();
    let pending: Vec<&str> = st["pending"].as_array().unwrap().iter().map(|u| u.as_str().unwrap()).collect();
    assert!(!pending.contains(&"aud") && pending.contains(&"doc") && pending.contains(&"adm") && pending.contains(&"admin"));
    assert!(!pending.contains(&"est"), "sin acceso a Finanzas no debe firmar");
    assert_eq!(st["signed"][0]["username"], "aud");
    assert!(s.get("/api/finance/manual", &aud).await.json()["my_signed_at"].is_string());
    assert!(s.get("/api/finance/manual", &doc).await.json()["my_signed_at"].is_null());

    // Version 2: pide resumen; las firmas anteriores no valen y la v1 sigue intacta
    assert_eq!(s.post("/api/finance/manual/versions", &adm, json!({ "body": format!("{}\n2. Se anulan con motivo.", texto) })).await.status, 400);
    let v2 = s.post("/api/finance/manual/versions", &adm, json!({ "body": format!("{}\n2. Se anulan con motivo.", texto), "summary": "Agrega anulaciones" })).await.json();
    assert_eq!(v2["version"], 2);
    assert_eq!(s.post("/api/finance/manual/sign", &doc, json!({ "version_id": v1["id"] })).await.status, 409, "firma de una version vieja");
    let st = s.get("/api/finance/manual/signatures", &aud).await.json();
    assert_eq!(st["version"]["version"], 2);
    assert!(st["pending"].as_array().unwrap().iter().any(|u| u == "aud"));
    let hist = s.get("/api/finance/manual", &aud).await.json();
    assert_eq!(hist["versions"].as_array().unwrap().len(), 2);
    let old = s.get(&format!("/api/finance/manual/versions/{}", v1["id"].as_str().unwrap()), &aud).await.json();
    assert_eq!(old["body"], texto);
    assert_eq!(s.post("/api/finance/manual/sign", &doc, json!({})).await.status, 200);

    // Inmutable: ni cambiar el texto ni borrar
    let (upd, del) = sql(&s, |c| {
        (
            c.execute("UPDATE fin_manual_versions SET body = 'otro texto' WHERE version = 1", []).is_err(),
            c.execute("DELETE FROM fin_manual_versions WHERE version = 1", []).is_err(),
        )
    })
    .await;
    assert!(upd && del);

    // Bitacora: dos publicaciones (con la huella del texto) y dos firmas
    let (pubs, firmas, con_huella): (i64, i64, i64) = sql(&s, |c| {
        (
            c.query_row("SELECT COUNT(*) FROM ledger_entries WHERE kind = 'manual'", [], |r| r.get(0)).unwrap(),
            c.query_row("SELECT COUNT(*) FROM ledger_entries WHERE kind = 'manual.firma'", [], |r| r.get(0)).unwrap(),
            c.query_row("SELECT COUNT(*) FROM ledger_entries WHERE kind = 'manual' AND data LIKE '%' || ?1 || '%'", [crate::ledger::sha256_hex(texto.as_bytes())], |r| r.get(0)).unwrap(),
        )
    })
    .await;
    assert_eq!((pubs, firmas, con_huella), (2, 2, 1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn horas_con_separacion_de_funciones() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let adm = s.user(&admin, "adm", "administrativo").await;
    let doc = s.user(&admin, "doc", "docente").await; // responsable del proyecto, solo "usar"
    let aud = s.user(&admin, "aud", "auditor").await;
    seed(&s).await;

    let mut body = json!({ "project_id": "p1", "month": "2026-03", "hours": 40, "amount": 300000, "budget_item_id": "bi1", "notes": "Diseno" });
    // Con monto hay que adjuntar contrato o boleta
    assert_eq!(s.post("/api/finance/hours", &doc, body.clone()).await.status, 400);
    body["file"] = json!({ "name": "contrato.pdf", "data": b64(PDF2) });
    // El auditor (Ver) no registra; nadie registra por otra persona salvo quien gestiona
    assert_eq!(s.post("/api/finance/hours", &aud, body.clone()).await.status, 403);
    let mut ajeno = body.clone();
    ajeno["username"] = json!("adm");
    assert_eq!(s.post("/api/finance/hours", &doc, ajeno).await.status, 403);
    let mut mal = body.clone();
    mal["month"] = json!("2026-13");
    assert_eq!(s.post("/api/finance/hours", &doc, mal).await.status, 400);
    let r = s.post("/api/finance/hours", &doc, body.clone()).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let h = r.json();
    let id = h["id"].as_str().unwrap().to_string();
    assert_eq!((h["status"].as_str(), h["file"]["sha256"].as_str()), (Some("pendiente"), Some(crate::ledger::sha256_hex(PDF2).as_str())));
    assert_eq!(s.post("/api/finance/hours", &doc, body.clone()).await.status, 409, "mismo mes y proyecto");

    // No se aprueba a si mismo, aunque sea el responsable del proyecto
    let r = s.post(&format!("/api/finance/hours/{}/approve", id), &doc, json!({})).await;
    assert_eq!(r.status, 403);
    assert!(r.text.contains("Separacion de funciones"));
    // Sin cambiar nada hasta aqui
    let n: i64 = sql(&s, |c| c.query_row("SELECT COUNT(*) FROM fin_commitments WHERE source = 'honorario'", [], |r| r.get(0)).unwrap()).await;
    assert_eq!(n, 0);
    // El auditor tampoco decide
    assert_eq!(s.post(&format!("/api/finance/hours/{}/approve", id), &aud, json!({})).await.status, 403);
    // Quien gestiona si: cuenta como gastado en el item
    let r = s.post(&format!("/api/finance/hours/{}/approve", id), &adm, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!((r.json()["status"].as_str(), r.json()["decided_by"].as_str()), (Some("aprobada"), Some("adm")));
    let (spent, item): (f64, String) = sql(&s, |c| c.query_row("SELECT spent, budget_item_id FROM fin_commitments WHERE source = 'honorario'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()).await;
    assert_eq!((spent, item.as_str()), (300000.0, "bi1"));
    assert_eq!(s.post(&format!("/api/finance/hours/{}/approve", id), &adm, json!({})).await.status, 409, "ya aprobada");

    // Horas de quien gestiona: las aprueba el responsable del proyecto, no ella misma
    let mut propias = json!({ "project_id": "p1", "month": "2026-03", "hours": 10.5 });
    let h2 = s.post("/api/finance/hours", &adm, propias.clone()).await.json();
    let id2 = h2["id"].as_str().unwrap().to_string();
    let r = s.post(&format!("/api/finance/hours/{}/approve", id2), &adm, json!({})).await;
    assert_eq!(r.status, 403, "{}", r.text);
    assert_eq!(s.post(&format!("/api/finance/hours/{}/reject", id2), &doc, json!({})).await.status, 400, "rechazar pide motivo");
    let r = s.post(&format!("/api/finance/hours/{}/approve", id2), &doc, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text);
    // Una persona con solo "ver" que no es responsable no decide
    propias["month"] = json!("2026-04");
    let h3 = s.post("/api/finance/hours", &adm, propias).await.json();
    assert_eq!(s.post(&format!("/api/finance/hours/{}/reject", h3["id"].as_str().unwrap()), &aud, json!({ "note": "x" })).await.status, 403);

    // Resumen por proyecto y persona
    let sum = s.get("/api/finance/hours/summary?project_id=p1", &aud).await.json();
    let row = |u: &str| sum.as_array().unwrap().iter().find(|r| r["username"] == u).cloned().unwrap();
    assert_eq!((row("doc")["approved_hours"].as_f64(), row("doc")["approved_amount"].as_f64()), (Some(40.0), Some(300000.0)));
    assert_eq!((row("adm")["approved_hours"].as_f64(), row("adm")["pending_hours"].as_f64()), (Some(10.5), Some(10.5)));
    assert_eq!(s.get("/api/finance/hours?project_id=p1&status=aprobada", &aud).await.json().as_array().unwrap().len(), 2);

    // Anular (motivo obligatorio): deja de contar como gastado
    assert_eq!(s.post(&format!("/api/finance/hours/{}/annul", id), &doc, json!({ "note": "error" })).await.status, 403);
    assert_eq!(s.post(&format!("/api/finance/hours/{}/annul", id), &adm, json!({})).await.status, 400);
    assert_eq!(s.post(&format!("/api/finance/hours/{}/annul", id), &adm, json!({ "note": "monto mal" })).await.status, 200);
    let spent: f64 = sql(&s, |c| c.query_row("SELECT spent FROM fin_commitments WHERE source = 'honorario' AND source_id IN (SELECT id FROM fin_hours WHERE month = '2026-03' AND username = 'doc')", [], |r| r.get(0)).unwrap()).await;
    assert_eq!(spent, 0.0);

    // Todo quedo en la bitacora con la huella del contrato
    let n: i64 = sql(&s, |c| c.query_row("SELECT COUNT(*) FROM ledger_entries WHERE kind = 'horas' AND data LIKE '%' || ?1 || '%'", [crate::ledger::sha256_hex(PDF2)], |r| r.get(0)).unwrap()).await;
    assert!(n >= 1);
}
