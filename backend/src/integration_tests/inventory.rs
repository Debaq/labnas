//! Inventario ampliado: stock minimo, vencimientos, prestamos y QR (handlers::inventory_loans)

use chrono::{Duration, Local};
use serde_json::json;

use super::harness::Server;

fn day(d: i64) -> String {
    (Local::now().date_naive() + Duration::days(d)).format("%Y-%m-%d").to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stock_vencimientos_y_prestamos() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "observador").await;
    let _bob = s.user(&admin, "bob", "observador").await;

    let cat = s.post("/api/inventory/categories", &admin, json!({ "id": "", "name": "Taller" })).await.json()["id"].as_str().unwrap().to_string();
    let item = |name: &str, extra: serde_json::Value| {
        let mut v = json!({ "id": "", "category_id": cat, "name": name, "unit": "unidad" });
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        v
    };

    // Validaciones
    assert_eq!(s.post("/api/inventory/items", &admin, item("X", json!({ "expires_on": "mañana" }))).await.status, 400);

    // Consumible con minimo y vencimiento
    let resina = s.post("/api/inventory/items", &admin, item("Resina gris", json!({ "quantity": 5, "min_quantity": 2, "expires_on": day(10) }))).await;
    assert_eq!(resina.status, 200);
    let resina = resina.json();
    let rid = resina["id"].as_str().unwrap().to_string();
    assert!(s.get("/api/deadlines", &admin).await.text.contains("Vence: Resina gris"));

    // Baja del minimo: un aviso; reponer lo rearma
    let mut low = resina.clone();
    low["quantity"] = json!(1);
    assert_eq!(s.put(&format!("/api/inventory/items/{}", rid), &admin, low.clone()).await.status, 200);
    let ls = s.get("/api/inventory/low-stock", &ana).await.json();
    assert_eq!(ls[0]["name"], "Resina gris");
    s.put(&format!("/api/inventory/items/{}", rid), &admin, low.clone()).await;
    let log = s.get("/api/audit?limit=100", &admin).await.text;
    assert_eq!(log.matches("Queda poco").count(), 1, "una vez por vez que baja");
    let mut ok = resina.clone();
    ok["quantity"] = json!(8);
    s.put(&format!("/api/inventory/items/{}", rid), &admin, ok).await;
    assert_eq!(s.get("/api/inventory/low-stock", &ana).await.json().as_array().unwrap().len(), 0);

    // Herramienta prestable (2 unidades)
    let taladro = s.post("/api/inventory/items", &admin, item("Taladro", json!({ "quantity": 2, "loanable": true }))).await.json();
    let tid = taladro["id"].as_str().unwrap().to_string();
    let url = format!("/api/inventory/items/{}/loans", tid);
    assert_eq!(s.post(&url, &ana, json!({ "due_on": day(-1) })).await.status, 400, "devolucion en el pasado");
    assert_eq!(s.post(&url, &ana, json!({ "due_on": day(2), "borrower": "bob" })).await.status, 403, "prestar a otro es del staff");
    let l = s.post(&url, &ana, json!({ "due_on": day(2) })).await;
    assert_eq!(l.status, 201);
    let loan = l.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.post(&url, &ana, json!({ "due_on": day(2), "quantity": 2 })).await.status, 409, "solo queda 1");
    let items = s.get("/api/inventory/items", &ana).await.json();
    let t = items.as_array().unwrap().iter().find(|i| i["id"] == tid.as_str()).unwrap().clone();
    assert_eq!(t["on_loan"], 1.0);

    // No prestable: solo el staff lo presta
    assert_eq!(s.post(&format!("/api/inventory/items/{}/loans", rid), &ana, json!({ "due_on": day(1) })).await.status, 403);
    assert_eq!(s.post(&format!("/api/inventory/items/{}/loans", rid), &admin, json!({ "due_on": day(1), "borrower": "bob" })).await.status, 201);

    // Lista: sin gestionar, solo lo propio
    let mine = s.get("/api/inventory/loans?open=true", &ana).await.json();
    assert_eq!(mine.as_array().unwrap().len(), 1);
    assert_eq!(s.get("/api/inventory/loans?open=true", &admin).await.json().as_array().unwrap().len(), 2);

    // Devolver: quien lo tiene (otro no)
    let bob = s.user(&admin, "cata", "observador").await;
    assert_eq!(s.post(&format!("/api/inventory/loans/{}/return", loan), &bob, json!({})).await.status, 403);
    let r = s.post(&format!("/api/inventory/loans/{}/return", loan), &ana, json!({ "note": "sin broca" })).await;
    assert_eq!(r.status, 200);
    assert!(r.json()["returned_at"].is_string());
    assert_eq!(s.post(&format!("/api/inventory/loans/{}/return", loan), &ana, json!({})).await.status, 409);

    // QR del item
    let qr = s.get(&format!("/api/inventory/items/{}/qr?base=http://lab.local:3000", tid), &ana).await.json();
    assert_eq!(qr["url"], format!("http://lab.local:3000/inventory?item={}", tid));
    assert!(qr["svg"].as_str().unwrap().contains("<svg"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retazos() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "observador").await;
    let bob = s.user(&admin, "bob", "observador").await;

    assert_eq!(s.post("/api/inventory/offcuts", &ana, json!({ "material": "MDF", "width_mm": 0, "height_mm": 10 })).await.status, 400);
    let grande = s.post("/api/inventory/offcuts", &ana, json!({ "material": "MDF", "thickness_mm": 3, "width_mm": 600, "height_mm": 400 })).await;
    assert_eq!(grande.status, 201);
    let chico = s.post("/api/inventory/offcuts", &ana, json!({ "material": "MDF", "thickness_mm": 3, "width_mm": 300, "height_mm": 120 })).await.json();
    s.post("/api/inventory/offcuts", &bob, json!({ "material": "Acrilico", "thickness_mm": 3, "width_mm": 500, "height_mm": 500 })).await;

    // Pieza de 100x250 mm en MDF 3 mm: sirven los dos (el chico girado), el chico primero
    let r = s.get("/api/inventory/offcuts?material=mdf&thickness=3&min_w=100&min_h=250", &bob).await.json();
    assert_eq!(r.as_array().unwrap().len(), 2);
    assert_eq!(r[0]["id"], chico["id"]);
    // 450x450: ninguno de MDF
    assert_eq!(s.get("/api/inventory/offcuts?material=mdf&min_w=450&min_h=450", &bob).await.json().as_array().unwrap().len(), 0);

    // Tomarlo lo saca de la lista
    let id = chico["id"].as_str().unwrap();
    assert_eq!(s.post(&format!("/api/inventory/offcuts/{}/take", id), &bob, json!({})).await.status, 200);
    assert_eq!(s.post(&format!("/api/inventory/offcuts/{}/take", id), &bob, json!({})).await.status, 409);
    assert_eq!(s.get("/api/inventory/offcuts?material=mdf", &bob).await.json().as_array().unwrap().len(), 1);

    // Borrar: quien lo subio o el staff
    let g = grande.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.delete(&format!("/api/inventory/offcuts/{}", g), &bob).await.status, 403);
    assert_eq!(s.delete(&format!("/api/inventory/offcuts/{}", g), &ana).await.status, 204);
}
