use serde_json::json;

use super::harness::{events, Server};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cola_compartida_de_impresion() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "observador").await;
    let eva = s.user(&admin, "eva", "observador").await;
    let op = s.user(&admin, "op", "operador").await;
    let printer = s.post("/api/printers3d", &admin, json!({ "name": "Prusa", "ip": "10.0.0.9", "port": 7125, "printer_type": "Moonraker" })).await.json();
    let printer_id = printer["id"].as_str().unwrap().to_string();

    // cualquiera pide; titulo obligatorio
    assert_eq!(s.post("/api/printers3d/queue", &ana, json!({ "title": "" })).await.status, 400);
    let a = s.post("/api/printers3d/queue", &ana, json!({ "title": "Soporte", "file_name": "soporte.gcode", "grams": 12.5, "seconds": 3600 })).await;
    assert_eq!(a.status, 201);
    let a = a.json()["id"].as_str().unwrap().to_string();
    let b = s.post("/api/printers3d/queue", &eva, json!({ "title": "Engranaje", "printer_id": printer_id })).await.json()["id"].as_str().unwrap().to_string();

    let list = s.get("/api/printers3d/queue", &ana).await.json();
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["id"], a.as_str(), "orden de llegada");

    // el solicitante solo cancela lo suyo pendiente; no cambia estado ni impresora
    assert_eq!(s.put(&format!("/api/printers3d/queue/{}", b), &ana, json!({ "status": "cancelado" })).await.status, 403);
    assert_eq!(s.put(&format!("/api/printers3d/queue/{}", a), &ana, json!({ "status": "imprimiendo" })).await.status, 403);
    assert_eq!(s.post("/api/printers3d/queue/reorder", &ana, json!({ "ids": [b, a] })).await.status, 403);

    // el operador ordena, asigna y cambia estado; el solicitante recibe aviso
    assert_eq!(s.post("/api/printers3d/queue/reorder", &op, json!({ "ids": [b.clone(), a.clone()] })).await.status, 200);
    assert_eq!(s.get("/api/printers3d/queue", &ana).await.json()[0]["id"], b.as_str());
    let mut ws_ana = s.live(&ana).await;
    let r = s.put(&format!("/api/printers3d/queue/{}", a), &op, json!({ "status": "imprimiendo", "printer_id": printer_id })).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["printer_id"], printer_id.as_str());
    let ev = events(&mut ws_ana, 800).await;
    let aviso = ev.iter().find(|e| e["kind"] == "notify").expect("aviso al solicitante");
    assert_eq!(aviso["data"]["title"], "Tu pedido se esta imprimiendo");
    assert!(ev.iter().any(|e| e["kind"] == "printers3d.queue"));

    // null quita la impresora; estado invalido
    let r = s.put(&format!("/api/printers3d/queue/{}", a), &op, json!({ "printer_id": null })).await;
    assert_eq!(r.json()["printer_id"], serde_json::Value::Null);
    assert_eq!(s.put(&format!("/api/printers3d/queue/{}", a), &op, json!({ "status": "volando" })).await.status, 400);

    // ya no esta pendiente: ana no puede borrarlo; el operador si
    assert_eq!(s.delete(&format!("/api/printers3d/queue/{}", a), &ana).await.status, 403);
    assert_eq!(s.delete(&format!("/api/printers3d/queue/{}", b), &eva).await.status, 204, "lo propio pendiente");
    assert_eq!(s.delete(&format!("/api/printers3d/queue/{}", a), &op).await.status, 204);
}
