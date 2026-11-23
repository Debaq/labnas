use chrono::{Duration, Local, NaiveTime};
use serde_json::json;

use super::harness::Server;

/// `YYYY-MM-DDTHH:MM` de mañana a esa hora
fn tomorrow(h: u32, m: u32) -> String {
    let d = Local::now().date_naive() + Duration::days(1);
    d.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap()).format("%Y-%m-%dT%H:%M").to_string()
}

fn id(v: &serde_json::Value) -> String {
    v["id"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maquinas_reservas_y_fallas() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "observador").await;
    let eva = s.user(&admin, "eva", "observador").await;
    let op = s.user(&admin, "op", "operador").await;

    // solo staff crea; validacion de tipo
    let nueva = json!({ "name": "Laser CO2", "kind": "laser", "location": "Sala 2", "max_hours": 2 });
    assert_eq!(s.post("/api/machines", &ana, nueva.clone()).await.status, 403);
    assert_eq!(s.post("/api/machines", &op, json!({ "name": "X", "kind": "nave" })).await.status, 400);
    let r = s.post("/api/machines", &op, nueva).await;
    assert_eq!(r.status, 201);
    let laser = id(&r.json());
    assert_eq!(r.json()["status"], "disponible");

    // reservar: choques, maximo de horas, pasado
    let url = format!("/api/machines/{}/bookings", laser);
    let r = s.post(&url, &ana, json!({ "start": tomorrow(10, 0), "end": tomorrow(11, 30), "purpose": "Cortar MDF" })).await;
    assert_eq!(r.status, 201);
    assert_eq!(r.json()["status"], "confirmada");
    let reserva_ana = id(&r.json());
    let r = s.post(&url, &eva, json!({ "start": tomorrow(11, 0), "end": tomorrow(12, 0) })).await;
    assert_eq!(r.status, 409, "choca con la de ana");
    assert!(r.text.contains("ana"));
    assert_eq!(s.post(&url, &eva, json!({ "start": tomorrow(11, 30), "end": tomorrow(12, 30) })).await.status, 201, "justo despues si");
    assert_eq!(s.post(&url, &eva, json!({ "start": tomorrow(13, 0), "end": tomorrow(16, 0) })).await.status, 400, "mas de 2 h");
    assert_eq!(s.post(&url, &eva, json!({ "start": "2020-01-01T10:00", "end": "2020-01-01T11:00" })).await.status, 400);
    assert_eq!(s.post(&url, &eva, json!({ "start": tomorrow(14, 0), "end": tomorrow(13, 0) })).await.status, 400);
    // bloquear horario: solo staff
    assert_eq!(s.post(&url, &eva, json!({ "start": tomorrow(17, 0), "end": tomorrow(18, 0), "kind": "bloqueo" })).await.status, 403);
    assert_eq!(s.post(&url, &op, json!({ "start": tomorrow(17, 0), "end": tomorrow(18, 0), "kind": "bloqueo" })).await.status, 201);

    // cancelar: solo lo propio (o staff)
    assert_eq!(s.post(&format!("/api/machines/bookings/{}/cancel", reserva_ana), &eva, json!({})).await.status, 403);
    assert_eq!(s.post(&format!("/api/machines/bookings/{}/cancel", reserva_ana), &ana, json!({})).await.status, 200);
    assert_eq!(s.post(&format!("/api/machines/bookings/{}/cancel", reserva_ana), &ana, json!({})).await.status, 409);
    let mias = s.get("/api/machines/bookings?mine=true", &eva).await.json();
    assert_eq!(mias.as_array().unwrap().len(), 1);
    assert_eq!(mias[0]["machine_name"], "Laser CO2");

    // con aprobacion: queda pendiente hasta que un operador la apruebe
    let cnc = id(&s.post("/api/machines", &op, json!({ "name": "CNC", "kind": "cnc", "requires_approval": true })).await.json());
    let r = s.post(&format!("/api/machines/{}/bookings", cnc), &ana, json!({ "start": tomorrow(9, 0), "end": tomorrow(10, 0) })).await;
    assert_eq!(r.json()["status"], "pendiente");
    let pend = id(&r.json());
    assert_eq!(s.post(&format!("/api/machines/{}/bookings", cnc), &eva, json!({ "start": tomorrow(9, 30), "end": tomorrow(10, 30) })).await.status, 409, "la pendiente tambien ocupa");
    assert_eq!(s.post(&format!("/api/machines/bookings/{}/approve", pend), &ana, json!({})).await.status, 403);
    let pendientes = s.get("/api/machines/bookings?status=pendiente", &op).await.json();
    assert_eq!(pendientes.as_array().unwrap().len(), 1);
    let r = s.post(&format!("/api/machines/bookings/{}/approve", pend), &op, json!({})).await;
    assert_eq!(r.json()["status"], "confirmada");
    assert_eq!(r.json()["decided_by"], "op");

    // falla: bloquea nuevas reservas hasta que un operador la resuelve
    let r = s.post(&format!("/api/machines/{}/faults", laser), &ana, json!({ "description": "No enciende el tubo" })).await;
    assert_eq!(r.status, 201);
    let falla = id(&r.json());
    let ficha = s.get(&format!("/api/machines/{}", laser), &eva).await.json();
    assert_eq!(ficha["status"], "fuera_servicio");
    assert_eq!(ficha["open_faults"].as_array().unwrap().len(), 1);
    assert_eq!(s.post(&url, &eva, json!({ "start": tomorrow(19, 0), "end": tomorrow(20, 0) })).await.status, 409);
    assert_eq!(s.post(&format!("/api/machines/faults/{}/resolve", falla), &ana, json!({ "note": "listo" })).await.status, 403);
    let r = s.post(&format!("/api/machines/faults/{}/resolve", falla), &op, json!({ "note": "Cambio de fusible" })).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["resolved_by"], "op");
    assert_eq!(s.get(&format!("/api/machines/{}", laser), &eva).await.json()["status"], "disponible");
    assert_eq!(s.post(&url, &eva, json!({ "start": tomorrow(19, 0), "end": tomorrow(20, 0) })).await.status, 201);

    // foto y QR
    let jpeg = "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQ==";
    assert_eq!(s.post(&format!("/api/machines/{}/photo", laser), &ana, json!({ "data": jpeg })).await.status, 403);
    let r = s.post(&format!("/api/machines/{}/photo", laser), &op, json!({ "data": jpeg })).await;
    let photo = r.json()["photo_id"].as_str().unwrap().to_string();
    let r = s.get(&format!("/api/machines/photos/{}", photo), &ana).await;
    assert_eq!(r.header("content-type"), "image/jpeg");
    let qr = s.get(&format!("/api/machines/{}/qr?base=https://192.168.50.114:3443", laser), &ana).await.json();
    assert_eq!(qr["url"], format!("https://192.168.50.114:3443/machines/{}", laser));
    assert!(qr["svg"].as_str().unwrap().contains("<svg"));
    assert_eq!(s.get(&format!("/api/machines/{}/qr?base=javascript:alert(1)", laser), &ana).await.status, 400);

    // borrar: solo admin
    assert_eq!(s.delete(&format!("/api/machines/{}", cnc), &op).await.status, 403);
    assert_eq!(s.delete(&format!("/api/machines/{}", cnc), &admin).await.status, 204);
    assert_eq!(s.get(&format!("/api/machines/{}", cnc), &ana).await.status, 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maquinas_acceso_y_modulo() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let pend = s.register("pepe", "pepe-password").await.json()["token"].as_str().unwrap().to_string();
    let ana = s.user(&admin, "ana", "observador").await;
    assert_eq!(s.get("/api/machines", &pend).await.status, 403, "cuenta sin aprobar");
    assert_eq!(s.get("/api/machines", &ana).await.status, 200);
    // modulo apagado: la ruta no existe
    assert_eq!(s.put("/api/modules/machines", &admin, json!({ "enabled": false })).await.status, 200);
    assert_eq!(s.get("/api/machines", &ana).await.status, 404);
}
