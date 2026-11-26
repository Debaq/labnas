//! Transparencia (ideas 70 y 71): portal publico generado desde los datos reales, secciones,
//! informes publicados y solicitudes de informacion con folio, codigo y plazo.

use reqwest::Method;
use serde_json::{json, Value};

use super::harness::Server;

const PDF_B64: &str = "JVBERi0xLjQgaW5mb3JtZSBkZSBwcnVlYmE="; // "%PDF-1.4 informe de prueba"

fn sql(s: &Server, q: &str) {
    s.state.db.get().unwrap().execute_batch(q).unwrap();
}

fn count(s: &Server, q: &str) -> i64 {
    s.state.db.get().unwrap().query_row(q, [], |r| r.get(0)).unwrap()
}

async fn public(s: &Server, method: Method, path: &str, body: Option<Value>) -> super::harness::Resp {
    s.req(method, path, None, body).await
}

/// Un proyecto con presupuesto, un bien y una sesion de uso (con nombres que NO deben salir)
fn seed(s: &Server) {
    sql(
        s,
        "INSERT INTO portfolio_entries (id, title, institution, funding_source, start_date, status, created_at)
             VALUES ('p1', 'Proyecto Alfa', 'UACh', 'FONDEF', '2026-03-01', 'active', '2026-01-01T00:00:00Z');
         INSERT INTO fin_projects (project_id, currency, created_by, created_at) VALUES ('p1', 'CLP', 'admin', '2026-01-01T00:00:00Z');
         INSERT INTO fin_budget_items (id, project_id, category, name, approved, created_by, created_at) VALUES ('b1', 'p1', 'equipos', 'Laser', 1000000, 'admin', '2026-01-01T00:00:00Z');
         INSERT INTO fin_commitments (source, source_id, project_id, budget_item_id, label, committed, spent, updated_at) VALUES ('compra', 'c1', 'p1', 'b1', 'x', 300000, 200000, '2026-01-01T00:00:00Z');
         INSERT INTO inventory_items (id, category_id, name, cost, serial_number, project_id, inventory_number, responsible, created_at)
             VALUES ('i1', 'cat', 'Cortadora laser', 900000, 'SERIE-SECRETA-123', 'p1', 'INV-001', 'Responsable Pepe', '2026-01-01');
         INSERT INTO machines (id, name, created_by, created_at) VALUES ('m1', 'Laser CO2', 'admin', '2026-01-01T00:00:00Z');
         INSERT INTO machine_usage (id, machine_id, username, started_at, ended_at) VALUES ('u1', 'm1', 'usuaria_privada', '2026-10-05T10:00:00', '2026-10-05T12:30:00');",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn portal_apagado_da_404_y_encendido_muestra_solo_lo_activo() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let adm = s.user(&admin, "adm", "administrativo").await;
    let est = s.user(&admin, "est", "estudiante").await;
    seed(&s);

    // Apagado: no existe (ni siquiera responde 403)
    for p in ["/api/transparency/public/portal", "/api/transparency/public/informes/x"] {
        assert_eq!(public(&s, Method::GET, p, None).await.status, 404, "{}", p);
    }
    let sol = json!({ "name": "Ana", "body": "Quiero saber en que se gasto el fondo" });
    assert_eq!(public(&s, Method::POST, "/api/transparency/public/solicitudes", Some(sol)).await.status, 404);

    assert_eq!(s.put("/api/modules/transparencia", &admin, json!({ "enabled": true })).await.status, 200);
    // Activarlo quedo en la bitacora
    assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger_entries WHERE kind = 'transparencia' AND subject = 'modulo'"), 1);

    // Encendido pero sin secciones elegidas: portal vacio
    let p = public(&s, Method::GET, "/api/transparency/public/portal", None).await;
    assert_eq!(p.status, 200);
    let v = p.json();
    assert_eq!(v["sections"], json!([]));
    assert!(v.get("proyectos").is_none() && v.get("bienes").is_none());

    // Configurar: solo quien gestiona
    let cfg = json!({ "sections": { "proyectos": true, "montos": true, "bienes": true, "uso": true }, "deadline_days": 15 });
    assert_eq!(s.put("/api/transparency/config", &est, cfg.clone()).await.status, 403);
    assert_eq!(s.post("/api/transparency/preview", &est, cfg.clone()).await.status, 403);
    assert_eq!(s.put("/api/transparency/config", &adm, json!({ "sections": { "secreto": true } })).await.status, 400);
    assert_eq!(s.put("/api/transparency/config", &adm, json!({ "sections": {}, "deadline_days": 0 })).await.status, 400);

    // La vista previa no guarda nada
    let prev = s.post("/api/transparency/preview", &adm, cfg.clone()).await;
    assert_eq!(prev.status, 200);
    assert_eq!(prev.json()["sections"], json!(["proyectos", "montos", "bienes", "uso"]));
    assert_eq!(public(&s, Method::GET, "/api/transparency/public/portal", None).await.json()["sections"], json!([]));

    assert_eq!(s.put("/api/transparency/config", &adm, cfg).await.status, 200);
    let p = public(&s, Method::GET, "/api/transparency/public/portal", None).await;
    let v = p.json();
    // Identico a la vista previa (salvo la hora de generacion)
    let (mut a, mut b) = (v.clone(), prev.json());
    a["generated_at"] = json!("");
    b["generated_at"] = json!("");
    assert_eq!(a, b);
    assert_eq!(v["proyectos"][0]["title"], "Proyecto Alfa");
    assert_eq!(v["montos"][0]["approved"], 1000000.0);
    assert_eq!(v["montos"][0]["spent"], 200000.0);
    assert_eq!(v["bienes"][0]["inventory_number"], "INV-001");
    assert_eq!(v["uso"][0]["machine"], "Laser CO2");
    assert_eq!(v["uso"][0]["month"], "2026-10");
    assert_eq!(v["uso"][0]["hours"], 2.5);
    assert!(v.get("informes").is_none(), "informes no esta activa");
    assert_eq!(v["deadline_days"], 15);
    // Nada de datos personales
    for secreto in ["SERIE-SECRETA-123", "Responsable Pepe", "usuaria_privada", "admin", "adm"] {
        assert!(!p.text.contains(secreto), "el portal filtra {}", secreto);
    }
    // El cambio de secciones quedo en la bitacora
    assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger_entries WHERE kind = 'transparencia' AND subject = 'secciones'"), 1);

    // Informes: subir, publicar (seccion activa), descargar sin sesion, retirar
    let bad = json!({ "title": "Informe", "file": { "name": "a.pdf", "data": "aGVsbG8=" } });
    assert_eq!(s.post("/api/transparency/reports", &adm, bad).await.status, 400, "no es un archivo admitido");
    let rep = json!({ "title": "Informe anual 2026", "date": "2026-11-01", "project_id": "p1", "file": { "name": "anual.pdf", "data": PDF_B64 } });
    assert_eq!(s.post("/api/transparency/reports", &est, rep.clone()).await.status, 403);
    let r = s.post("/api/transparency/reports", &adm, rep).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let rid = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.get("/api/transparency/reports", &est).await.status, 403);
    assert_eq!(s.get("/api/transparency/reports", &adm).await.json().as_array().unwrap().len(), 1);

    // La seccion informes sigue apagada: no aparece, pero la descarga publicada responde
    assert!(public(&s, Method::GET, "/api/transparency/public/portal", None).await.json().get("informes").is_none());
    s.put("/api/transparency/config", &adm, json!({ "sections": { "proyectos": true, "informes": true } })).await;
    let v = public(&s, Method::GET, "/api/transparency/public/portal", None).await.json();
    assert_eq!(v["informes"][0]["title"], "Informe anual 2026");
    assert_eq!(v["informes"][0]["project"], "Proyecto Alfa");
    let url = format!("/api/transparency/public/informes/{}", rid);
    let d = public(&s, Method::GET, &url, None).await;
    assert_eq!((d.status, d.text.starts_with("%PDF")), (200, true));
    assert_eq!(d.header("x-content-type-options"), "nosniff");
    // Lo interno de Finanzas sigue cerrado
    let fid = count(&s, "SELECT COUNT(*) FROM fin_files");
    assert_eq!(fid, 1);
    let file_id: String = s.state.db.get().unwrap().query_row("SELECT id FROM fin_files", [], |r| r.get(0)).unwrap();
    assert_eq!(public(&s, Method::GET, &format!("/api/finance/files/{}", file_id), None).await.status, 401);

    // Retirar: ya no sale ni se descarga; el archivo se conserva para quien gestiona
    assert_eq!(s.post(&format!("/api/transparency/reports/{}/withdraw", rid), &est, json!({})).await.status, 403);
    assert_eq!(s.post(&format!("/api/transparency/reports/{}/withdraw", rid), &adm, json!({})).await.status, 200);
    assert_eq!(s.post(&format!("/api/transparency/reports/{}/withdraw", rid), &adm, json!({})).await.status, 409);
    assert_eq!(public(&s, Method::GET, &url, None).await.status, 404);
    assert_eq!(public(&s, Method::GET, "/api/transparency/public/portal", None).await.json()["informes"], json!([]));
    assert_eq!(s.get(&format!("/api/transparency/reports/{}/file", rid), &adm).await.status, 200);
    assert_eq!(s.post(&format!("/api/transparency/reports/{}/publish", rid), &adm, json!({})).await.status, 200);
    assert_eq!(public(&s, Method::GET, &url, None).await.status, 200);
    // subido, retirado y publicado de nuevo: tres registros de informe
    assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger_entries WHERE kind = 'transparencia' AND subject LIKE 'informe:%'"), 3);

    // Apagar el modulo vuelve a esconderlo todo
    assert_eq!(s.put("/api/modules/transparencia", &admin, json!({ "enabled": false })).await.status, 200);
    assert_eq!(public(&s, Method::GET, "/api/transparency/public/portal", None).await.status, 404);
    assert_eq!(public(&s, Method::GET, &url, None).await.status, 404);
    assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger_entries WHERE kind = 'transparencia' AND subject = 'modulo'"), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn solicitudes_con_folio_codigo_y_plazo() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let adm = s.user(&admin, "adm", "administrativo").await;
    let est = s.user(&admin, "est", "estudiante").await;
    assert_eq!(s.put("/api/modules/transparencia", &admin, json!({ "enabled": true })).await.status, 200);

    let url = "/api/transparency/public/solicitudes";
    // Validaciones del formulario publico
    assert_eq!(public(&s, Method::POST, url, Some(json!({ "name": "", "body": "Una solicitud valida" }))).await.status, 400);
    assert_eq!(public(&s, Method::POST, url, Some(json!({ "name": "Ana", "body": "corta" }))).await.status, 400);
    assert_eq!(public(&s, Method::POST, url, Some(json!({ "name": "Ana", "body": "x".repeat(4001) }))).await.status, 400);
    assert_eq!(public(&s, Method::POST, url, Some(json!({ "name": "Ana", "email": "sin arroba", "body": "Una solicitud valida" }))).await.status, 400);

    let r = public(&s, Method::POST, url, Some(json!({ "name": "Ana Perez", "email": "ana@example.cl", "body": "Quiero saber que compras se hicieron en 2026" }))).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let (folio, code) = (r.json()["folio"].as_str().unwrap().to_string(), r.json()["code"].as_str().unwrap().to_string());
    assert!(folio.starts_with("SI-") && folio.ends_with("-0001"), "{}", folio);
    assert_eq!(code.len(), 10);
    let year = &folio[3..7];
    assert_eq!(r.json()["due_date"].as_str().unwrap().len(), 10);
    let second = public(&s, Method::POST, url, Some(json!({ "name": "Beto", "body": "Otra solicitud de informacion" }))).await.json();
    assert_eq!(second["folio"], format!("SI-{}-0002", year));

    // Consulta con folio + codigo (se acepta en minusculas y con guiones)
    let look = "/api/transparency/public/solicitudes/consulta";
    let ok = public(&s, Method::POST, look, Some(json!({ "folio": folio.to_lowercase(), "code": format!("{}-{}", &code[..5], &code[5..]).to_lowercase() }))).await;
    assert_eq!(ok.status, 200, "{}", ok.text);
    assert_eq!(ok.json()["status"], "recibida");
    assert_eq!(ok.json()["days_left"], 20);
    // Codigo malo o folio inexistente: el mismo 404
    assert_eq!(public(&s, Method::POST, look, Some(json!({ "folio": folio, "code": "AAAAAAAAAA" }))).await.status, 404);
    assert_eq!(public(&s, Method::POST, look, Some(json!({ "folio": "SI-1999-0001", "code": code }))).await.status, 404);
    // Esas respuestas nunca traen datos de otras personas
    assert!(!ok.text.contains("Beto") && !ok.text.contains("ana@example.cl"));

    // Gestion: nombre y correo solo para quien gestiona
    assert_eq!(s.get("/api/transparency/requests", &est).await.status, 403);
    let list = s.get("/api/transparency/requests", &adm).await.json();
    assert_eq!(list.as_array().unwrap().len(), 2);
    let id = list.as_array().unwrap().iter().find(|r| r["folio"] == folio.as_str()).unwrap()["id"].as_str().unwrap().to_string();
    let put = |body: Value| {
        let (s, adm, id) = (&s, adm.clone(), id.clone());
        async move { s.put(&format!("/api/transparency/requests/{}", id), &adm, body).await }
    };
    assert_eq!(s.put(&format!("/api/transparency/requests/{}", id), &est, json!({ "status": "en_curso" })).await.status, 403);
    assert_eq!(put(json!({ "assigned_to": "est" })).await.status, 400, "responsable sin gestion");
    assert_eq!(put(json!({ "status": "respondida" })).await.status, 400, "falta la respuesta");
    assert_eq!(put(json!({ "status": "rechazada" })).await.status, 400, "falta el motivo");
    assert_eq!(put(json!({ "status": "derivada" })).await.status, 400, "falta el destino");
    assert_eq!(put(json!({ "status": "inventado" })).await.status, 400);
    let r = put(json!({ "status": "en_curso", "assigned_to": "adm", "note": "Lo reviso yo" })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["assigned_to"], "adm");
    // La respuesta aun no se ve
    let ok = public(&s, Method::POST, look, Some(json!({ "folio": folio, "code": code }))).await.json();
    assert_eq!((ok["status"].as_str(), ok["response"].as_str()), (Some("en_curso"), Some("")));

    let r = put(json!({ "status": "respondida", "response": "Se hicieron 12 compras; el detalle esta en el portal." })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let ok = public(&s, Method::POST, look, Some(json!({ "folio": folio, "code": code }))).await.json();
    assert_eq!(ok["status"], "respondida");
    assert!(ok["response"].as_str().unwrap().contains("12 compras"));
    assert!(ok["days_left"].is_null());
    assert_eq!(ok["history"].as_array().unwrap().len(), 3, "recibida, en curso, respondida");
    assert_eq!(put(json!({ "status": "en_curso" })).await.status, 409, "ya cerrada");
    // La otra, rechazada con motivo
    let id2 = list.as_array().unwrap().iter().find(|r| r["folio"] != folio.as_str()).unwrap()["id"].as_str().unwrap().to_string();
    let rej = s.put(&format!("/api/transparency/requests/{}", id2), &adm, json!({ "status": "rechazada", "reason": "No es informacion publica" })).await;
    assert_eq!(rej.status, 200, "{}", rej.text);
    let det = s.get(&format!("/api/transparency/requests/{}", id), &adm).await.json();
    assert_eq!(det["events"].as_array().unwrap().len(), 3);
    assert!(det["events"][2]["ledger_seq"].is_number());

    // Todo cambio de estado quedo en la bitacora (2 recibidas + en curso + respondida + rechazada)
    assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger_entries WHERE kind = 'transparencia' AND subject LIKE 'solicitud:%'"), 5);
    // Y sin datos personales de quien pregunta
    assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger_entries WHERE data LIKE '%Ana%' OR data LIKE '%example.cl%'"), 0);

    // Fuerza bruta: tras 5 intentos fallidos el folio se bloquea (incluso con el codigo bueno)
    for _ in 0..5 {
        assert_eq!(public(&s, Method::POST, look, Some(json!({ "folio": "SI-2026-9999", "code": "AAAAAAAAAA" }))).await.status, 404);
    }
    assert_eq!(public(&s, Method::POST, look, Some(json!({ "folio": "SI-2026-9999", "code": "AAAAAAAAAA" }))).await.status, 429);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aviso_de_plazo_una_vez_por_umbral() {
    let s = Server::start().await;
    let admin = s.admin().await;
    assert_eq!(s.put("/api/modules/transparencia", &admin, json!({ "enabled": true })).await.status, 200);
    let url = "/api/transparency/public/solicitudes";
    public(&s, Method::POST, url, Some(json!({ "name": "Ana", "body": "Solicitud numero uno de prueba" }))).await;

    // Lejos del plazo: nada
    assert_eq!(crate::handlers::transparency::requests::check_deadlines(&s.state).await, 0);
    // A 2 dias habiles: un aviso, y no se repite
    let soon = crate::handlers::transparency::requests::add_business_days(chrono::Local::now().date_naive(), 2).format("%Y-%m-%d").to_string();
    sql(&s, &format!("UPDATE transp_requests SET due_date = '{}'", soon));
    assert_eq!(crate::handlers::transparency::requests::check_deadlines(&s.state).await, 1);
    assert_eq!(crate::handlers::transparency::requests::check_deadlines(&s.state).await, 0);
    // Vencida: otro aviso (umbral distinto), tambien una sola vez
    sql(&s, "UPDATE transp_requests SET due_date = '2020-01-01'");
    assert_eq!(crate::handlers::transparency::requests::check_deadlines(&s.state).await, 1);
    assert_eq!(crate::handlers::transparency::requests::check_deadlines(&s.state).await, 0);
}
