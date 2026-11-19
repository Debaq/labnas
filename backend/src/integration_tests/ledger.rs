//! Bitacora inalterable por HTTP: notas con huella de archivos, anulaciones,
//! verificacion, exportacion y deteccion de alteraciones hechas directo en la base.

use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bitacora_de_punta_a_punta() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;

    // Solo admin
    assert_eq!(s.get("/api/ledger", &op).await.status, 403);
    assert_eq!(s.post("/api/ledger/notes", &op, json!({ "title": "x" })).await.status, 403);

    // Vacia: cuadra y la huella final es la de origen
    let v = s.get("/api/ledger/verify", &admin).await.json();
    assert_eq!((v["ok"].as_bool(), v["count"].as_i64()), (Some(true), Some(0)));
    assert_eq!(v["head_hash"], crate::ledger::GENESIS);

    // Validaciones
    assert_eq!(s.post("/api/ledger/notes", &admin, json!({ "title": "  " })).await.status, 400);
    let afuera = json!({ "title": "x", "files": ["/etc/passwd"] });
    assert_eq!(s.post("/api/ledger/notes", &admin, afuera).await.status, 403);

    // Nota con la huella de una factura del NAS
    std::fs::write(s.home.join("factura.pdf"), b"%PDF factura 123").unwrap();
    let nota = json!({
        "title": "Compra CNC", "text": "Aprobada en acta 4", "subject": "proyecto FONDEF",
        "files": [s.path("factura.pdf")],
    });
    let r = s.post("/api/ledger/notes", &admin, nota).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let e1 = r.json();
    assert_eq!((e1["seq"].as_i64(), e1["kind"].as_str()), (Some(1), Some("nota")));
    let data: serde_json::Value = serde_json::from_str(e1["data"].as_str().unwrap()).unwrap();
    assert_eq!(data["files"][0]["sha256"], crate::ledger::sha256_hex(b"%PDF factura 123"));

    let files = s.get("/api/ledger/1/files", &admin).await.json();
    assert_eq!(files[0]["status"], "igual");
    std::fs::write(s.home.join("factura.pdf"), b"%PDF factura 999").unwrap();
    assert_eq!(s.get("/api/ledger/1/files", &admin).await.json()[0]["status"], "cambiado");
    std::fs::remove_file(s.home.join("factura.pdf")).unwrap();
    assert_eq!(s.get("/api/ledger/1/files", &admin).await.json()[0]["status"], "no_existe");

    // Anular: nuevo registro; no se anula dos veces ni se anula una anulacion
    assert_eq!(s.post("/api/ledger/1/annul", &admin, json!({ "reason": "" })).await.status, 400);
    assert_eq!(s.post("/api/ledger/9/annul", &admin, json!({ "reason": "x" })).await.status, 404);
    let r = s.post("/api/ledger/1/annul", &admin, json!({ "reason": "monto mal escrito" })).await;
    assert_eq!(r.status, 201, "{}", r.text);
    assert_eq!(r.json()["annuls"], 1);
    assert_eq!(s.post("/api/ledger/1/annul", &admin, json!({ "reason": "otra vez" })).await.status, 409);
    assert_eq!(s.post("/api/ledger/2/annul", &admin, json!({ "reason": "x" })).await.status, 400);

    let list = s.get("/api/ledger", &admin).await.json();
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!((list[0]["seq"].as_i64(), list[1]["annulled_by"].as_i64()), (Some(2), Some(2)));
    assert_eq!(s.get("/api/ledger?q=CNC", &admin).await.json().as_array().unwrap().len(), 1);

    // Exportacion: una linea por registro, verificable con la receta publica
    let r = s.get("/api/ledger/export", &admin).await;
    assert!(r.header("content-disposition").contains("bitacora-labnas-"));
    let lines: Vec<serde_json::Value> = r.text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2);
    let l = &lines[1];
    let h = crate::ledger::compute_hash(
        l["seq"].as_i64().unwrap(),
        l["timestamp"].as_str().unwrap(),
        l["username"].as_str().unwrap(),
        l["kind"].as_str().unwrap(),
        l["subject"].as_str().unwrap(),
        l["annuls"].as_i64(),
        l["data"].as_str().unwrap(),
        l["prev_hash"].as_str().unwrap(),
    );
    assert_eq!(l["hash"], h.as_str());
    assert_eq!(l["prev_hash"], lines[0]["hash"]);

    // La revision diaria sin Telegram no finge un ancla: la huella no salio del servidor
    crate::handlers::ledger::check_once(&s.state).await;
    assert!(s.get("/api/ledger/verify", &admin).await.json()["last_anchor"].is_null());
    // A mano si: la persona recibe el texto para guardarlo afuera
    let r = s.post("/api/ledger/anchor", &admin, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["telegram_sent"], 0);
    let v = s.get("/api/ledger/verify", &admin).await.json();
    assert_eq!(v["last_anchor"]["via"], "texto");
    assert_eq!(v["last_anchor"]["seq"], 2);
    assert_eq!(v["last_anchor"]["hash"], l["hash"]);

    // Alguien con acceso al archivo de la base cambia un registro: la cadena lo delata
    let conn = s.state.db.get().unwrap();
    conn.execute_batch("DROP TRIGGER ledger_no_update;").unwrap();
    conn.execute("UPDATE ledger_entries SET data = replace(data, 'CNC', 'Notebook') WHERE seq = 1", []).unwrap();
    drop(conn);
    let v = s.get("/api/ledger/verify", &admin).await.json();
    assert_eq!((v["ok"].as_bool(), v["broken_at"].as_i64()), (Some(false), Some(1)));
    assert_eq!(s.post("/api/ledger/anchor", &admin, json!({})).await.status, 409);

    // La revision diaria lo registra en la auditoria
    crate::handlers::ledger::check_once(&s.state).await;
    let audit = s.get("/api/audit?q=ALTERADA", &admin).await.json();
    assert_eq!(audit.as_array().unwrap().len(), 1);
}
