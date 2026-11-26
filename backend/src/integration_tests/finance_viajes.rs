//! Viajes y viaticos (idea 63): solicitud, aprobacion sin quien pide ni quien viaja,
//! comprobantes, informe obligatorio para cerrar y compromisos en la ejecucion.

use serde_json::{json, Value};

use super::harness::Server;

const PDF_B64: &str = "JVBERi0xLjQgaW5mb3JtZSBkZSBwcnVlYmE="; // "%PDF-1.4 informe de prueba"

fn sql(s: &Server, q: &str) {
    s.state.db.get().unwrap().execute_batch(q).unwrap();
}

fn commitment(s: &Server, id: &str) -> Option<(f64, f64)> {
    s.state
        .db
        .get()
        .unwrap()
        .query_row("SELECT committed, spent FROM fin_commitments WHERE source = 'viaje' AND source_id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .ok()
}

fn trip(extra: Value) -> Value {
    let mut t = json!({
        "project_id": "p1", "budget_item_id": "b1", "traveler": "Pedro Soto", "destination": "Santiago",
        "start_date": "2026-12-01", "end_date": "2026-12-03", "purpose": "Congreso de fabricacion digital",
        "est_transport": 120000, "est_lodging": 90000, "est_per_diem": 60000
    });
    for (k, v) in extra.as_object().unwrap() {
        t[k] = v.clone();
    }
    t
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn viaje_de_la_solicitud_al_cierre() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let pedro = s.user(&admin, "pedro", "docente").await; // Usar
    let adm = s.user(&admin, "adm", "administrativo").await; // Gestionar
    let inv = s.user(&admin, "inv", "investigador").await; // Gestionar
    let est = s.user(&admin, "est", "estudiante").await; // sin acceso
    sql(
        &s,
        "INSERT INTO portfolio_entries (id, title, created_at) VALUES ('p1', 'Proyecto Alfa', '2026-01-01T00:00:00Z');
         INSERT INTO fin_projects (project_id, created_by, created_at) VALUES ('p1', 'admin', '2026-01-01T00:00:00Z');
         INSERT INTO fin_budget_items (id, project_id, category, name, approved, created_by, created_at) VALUES ('b1', 'p1', 'viajes', 'Pasajes', 500000, 'admin', '2026-01-01T00:00:00Z');
         INSERT INTO portfolio_entries (id, title, created_at) VALUES ('p2', 'Otro', '2026-01-01T00:00:00Z');",
    );

    // Pedir: Usar; sin acceso no
    assert_eq!(s.post("/api/finance/trips", &est, trip(json!({}))).await.status, 403);
    assert_eq!(s.post("/api/finance/trips", &pedro, trip(json!({ "end_date": "2026-11-30" }))).await.status, 400, "regreso antes de la salida");
    assert_eq!(s.post("/api/finance/trips", &pedro, trip(json!({ "est_transport": 0, "est_lodging": 0, "est_per_diem": 0 }))).await.status, 400);
    assert_eq!(s.post("/api/finance/trips", &pedro, trip(json!({ "project_id": "nope" }))).await.status, 400);
    assert_eq!(s.post("/api/finance/trips", &pedro, trip(json!({ "project_id": "p2" }))).await.status, 400, "item de otro proyecto");
    assert_eq!(s.post("/api/finance/trips", &pedro, trip(json!({ "est_transport": -5 }))).await.status, 400);
    let r = s.post("/api/finance/trips", &pedro, trip(json!({ "traveler_user": "pedro" }))).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let t = r.json();
    let id = t["id"].as_str().unwrap().to_string();
    assert_eq!((t["status"].as_str(), t["est_total"].as_f64(), t["folio"].as_i64()), (Some("solicitud"), Some(270000.0), Some(1)));
    assert!(commitment(&s, &id).is_none(), "pedir no compromete nada");

    // Quien pidio (y viaja) no aprueba ni rechaza: ni por nivel ni por separacion de funciones
    assert_eq!(s.post(&format!("/api/finance/trips/{}/approve", id), &pedro, json!({})).await.status, 403);
    // Un gestor que viaja en su propio viaje tampoco
    let own = s.post("/api/finance/trips", &adm, trip(json!({ "traveler": "Adm", "traveler_user": "adm" }))).await.json();
    let own_id = own["id"].as_str().unwrap().to_string();
    let r = s.post(&format!("/api/finance/trips/{}/approve", own_id), &adm, json!({})).await;
    assert_eq!(r.status, 403);
    assert!(r.text.contains("otra persona"), "{}", r.text);
    assert_eq!(s.post(&format!("/api/finance/trips/{}/reject", own_id), &adm, json!({ "note": "no" })).await.status, 403);
    // ... ni quien viaja sin haber pedido
    let other = s.post("/api/finance/trips", &pedro, trip(json!({ "traveler": "Inv", "traveler_user": "inv" }))).await.json();
    let other_id = other["id"].as_str().unwrap().to_string();
    assert_eq!(s.post(&format!("/api/finance/trips/{}/approve", other_id), &inv, json!({})).await.status, 403);
    // Otro gestor si
    assert_eq!(s.post(&format!("/api/finance/trips/{}/approve", own_id), &inv, json!({})).await.status, 200);

    // Rechazar pide motivo y no compromete nada
    assert_eq!(s.post(&format!("/api/finance/trips/{}/reject", other_id), &adm, json!({})).await.status, 400);
    assert_eq!(s.post(&format!("/api/finance/trips/{}/reject", other_id), &adm, json!({ "note": "Sin fondos este mes" })).await.status, 200);
    assert!(commitment(&s, &other_id).is_none());
    assert_eq!(s.post(&format!("/api/finance/trips/{}/approve", other_id), &adm, json!({})).await.status, 409, "ya decidido");

    // Aprobar compromete el estimado
    let r = s.post(&format!("/api/finance/trips/{}/approve", id), &adm, json!({ "note": "ok" })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["status"], "aprobado");
    assert_eq!(commitment(&s, &id), Some((270000.0, 0.0)));
    assert_eq!(s.put(&format!("/api/finance/trips/{}", id), &pedro, trip(json!({}))).await.status, 409, "ya no se edita");

    // Comprobantes: monto, tipo y respaldo
    let rc = format!("/api/finance/trips/{}/receipts", id);
    assert_eq!(s.post(&rc, &pedro, json!({ "kind": "pasaje", "amount": 110000 })).await.status, 400, "sin respaldo");
    assert_eq!(s.post(&rc, &pedro, json!({ "kind": "taxi", "amount": 1000, "file": { "name": "a.pdf", "data": PDF_B64 } })).await.status, 400);
    assert_eq!(s.post(&rc, &pedro, json!({ "kind": "pasaje", "amount": 0, "file": { "name": "a.pdf", "data": PDF_B64 } })).await.status, 400);
    assert_eq!(s.post(&rc, &est, json!({ "kind": "viatico", "amount": 1000 })).await.status, 403);
    let r = s.post(&rc, &pedro, json!({ "kind": "pasaje", "description": "Bus ida y vuelta", "amount": 110000, "file": { "name": "pasaje.pdf", "data": PDF_B64 } })).await;
    assert_eq!(r.status, 201, "{}", r.text);
    assert_eq!(r.json()["receipts"][0]["file"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(s.post(&rc, &pedro, json!({ "kind": "viatico", "amount": 60000 })).await.status, 201, "el viatico va sin boleta");
    let rid = s.post(&rc, &pedro, json!({ "kind": "otro", "amount": 5000, "file": { "name": "x.pdf", "data": PDF_B64 } })).await.json()["receipts"][2]["id"].as_str().unwrap().to_string();
    assert_eq!(s.delete(&format!("{}/{}", rc, rid), &pedro).await.status, 200);

    // Cerrar exige el informe de comision
    let close = format!("/api/finance/trips/{}/close", id);
    let r = s.post(&close, &pedro, json!({})).await;
    assert_eq!(r.status, 400, "{}", r.text);
    assert!(r.text.contains("informe"));
    assert_eq!(s.put(&format!("/api/finance/trips/{}/report", id), &pedro, json!({ "text": "corto" })).await.status, 200);
    assert_eq!(s.post(&close, &pedro, json!({})).await.status, 400, "informe demasiado corto");
    let r = s.put(&format!("/api/finance/trips/{}/report", id), &pedro, json!({ "text": "Se asistio al congreso, se presento el proyecto y se hicieron tres contactos." })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    // Adjunto del informe (sin monto)
    assert_eq!(s.post(&rc, &pedro, json!({ "kind": "informe", "description": "Presentacion", "file": { "name": "pres.pdf", "data": PDF_B64 } })).await.status, 201);
    assert_eq!(s.post(&close, &est, json!({})).await.status, 403);
    let r = s.post(&close, &pedro, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let c = r.json();
    assert_eq!((c["status"].as_str(), c["spent"].as_f64()), (Some("cerrado"), Some(170000.0)));
    // Cerrado: comprometido 0, gastado = suma de comprobantes (sin el adjunto del informe)
    assert_eq!(commitment(&s, &id), Some((0.0, 170000.0)));
    assert_eq!(s.post(&rc, &pedro, json!({ "kind": "viatico", "amount": 1 })).await.status, 409);
    assert_eq!(s.post(&close, &pedro, json!({})).await.status, 409);

    // Anular un viaje aprobado devuelve lo comprometido
    assert_eq!(commitment(&s, &own_id), Some((270000.0, 0.0)));
    assert_eq!(s.post(&format!("/api/finance/trips/{}/cancel", own_id), &adm, json!({})).await.status, 400, "pide motivo");
    assert_eq!(s.post(&format!("/api/finance/trips/{}/cancel", own_id), &adm, json!({ "reason": "Se suspendio el congreso" })).await.status, 200);
    assert_eq!(commitment(&s, &own_id), Some((0.0, 0.0)));

    // Aprobaciones, rechazos, cierres y anulaciones en la bitacora: 2 aprobados + 1 rechazo + 1 cierre + 1 anulacion
    let n: i64 = s.state.db.get().unwrap().query_row("SELECT COUNT(*) FROM ledger_entries WHERE kind = 'viaje'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 5);

    // Lista y ficha
    let all = s.get("/api/finance/trips?project_id=p1", &pedro).await.json();
    assert_eq!(all.as_array().unwrap().len(), 3);
    let d = s.get(&format!("/api/finance/trips/{}", id), &pedro).await.json();
    assert_eq!(d["steps"].as_array().unwrap().len(), 3);
    assert!(d["steps"][1]["ledger_seq"].is_number());
    assert_eq!(s.get("/api/finance/trips", &est).await.status, 403);
}
