//! Mantenimiento y calibracion por maquina (handlers::machine_maintenance)

use chrono::{Duration, Local, NaiveTime};
use serde_json::json;

use super::harness::Server;

fn tomorrow(h: u32) -> String {
    let d = Local::now().date_naive() + Duration::days(1);
    d.and_time(NaiveTime::from_hms_opt(h, 0, 0).unwrap()).format("%Y-%m-%dT%H:%M").to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plan_bitacora_y_bloqueo() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let ana = s.user(&admin, "ana", "observador").await;
    let eva = s.user(&admin, "eva", "observador").await;

    let r = s.post("/api/machines", &op, json!({ "name": "Laser CO2", "kind": "laser", "responsible": "ana" })).await;
    assert_eq!(r.status, 201);
    let m = r.json()["id"].as_str().unwrap().to_string();
    let base = format!("/api/machines/{}", m);

    // Horometro: solo staff
    assert_eq!(s.put(&format!("{base}/usage"), &ana, json!({ "hours": 100 })).await.status, 403);
    let u = s.put(&format!("{base}/usage"), &op, json!({ "hours": 100 })).await;
    assert_eq!(u.status, 200);
    assert_eq!(u.json()["usage_hours"], 100.0);

    // Plan: una por horas que bloquea, otra por fecha
    assert_eq!(s.post(&format!("{base}/maintenance/tasks"), &op, json!({ "title": "Sin frecuencia" })).await.status, 400);
    assert_eq!(s.post(&format!("{base}/maintenance/tasks"), &ana, json!({ "title": "X", "every_days": 3 })).await.status, 403);
    let lente = s.post(&format!("{base}/maintenance/tasks"), &op, json!({ "title": "Limpiar lente", "every_hours": 10, "blocks": true })).await;
    assert_eq!(lente.status, 201);
    let lente = lente.json();
    assert_eq!(lente["hours_left"], 10.0, "las horas cuentan desde que se crea");
    assert_eq!(lente["level"], "ok");
    let lente_id = lente["id"].as_str().unwrap().to_string();
    let cal = s.post(&format!("{base}/maintenance/tasks"), &op, json!({ "title": "Alinear espejos", "kind": "calibracion", "every_days": 5 })).await;
    assert_eq!(cal.status, 201);
    assert_eq!(cal.json()["level"], "ok", "faltan 5 de 5 dias");

    // La de fecha sale en Plazos
    let plazos = s.get("/api/deadlines", &op).await.text;
    assert!(plazos.contains("Alinear espejos") && plazos.contains("mantencion"));

    // Pasan 15 horas: vencida y bloquea reservas (menos al staff)
    s.put(&format!("{base}/usage"), &op, json!({ "hours": 115 })).await;
    let ov = s.get(&format!("{base}/maintenance"), &eva).await.json();
    let t = ov["tasks"].as_array().unwrap().iter().find(|t| t["id"] == lente_id.as_str()).unwrap().clone();
    assert_eq!(t["level"], "vencida");
    let r = s.post(&format!("{base}/bookings"), &eva, json!({ "start": tomorrow(10), "end": tomorrow(11) })).await;
    assert_eq!(r.status, 409);
    assert!(r.text.contains("Limpiar lente"));
    assert_eq!(s.post(&format!("{base}/bookings"), &op, json!({ "start": tomorrow(8), "end": tomorrow(9) })).await.status, 201);

    // La registra la responsable (no cualquiera)
    assert_eq!(s.post(&format!("{base}/maintenance/log"), &eva, json!({ "task_id": lente_id })).await.status, 403);
    let r = s.post(&format!("{base}/maintenance/log"), &ana, json!({ "task_id": lente_id, "note": "Lente limpio, sin rayas" })).await;
    assert_eq!(r.status, 201);
    let ov = r.json();
    let t = ov["tasks"].as_array().unwrap().iter().find(|t| t["id"] == lente_id.as_str()).unwrap().clone();
    assert_eq!(t["level"], "ok");
    assert_eq!(t["hours_left"], 10.0);
    assert_eq!(ov["log"][0]["title"], "Limpiar lente");
    assert_eq!(ov["log"][0]["usage_hours"], 115.0);
    assert_eq!(s.post(&format!("{base}/bookings"), &eva, json!({ "start": tomorrow(10), "end": tomorrow(11) })).await.status, 201);

    // Registro suelto y falla reparada entran a la bitacora
    assert_eq!(s.post(&format!("{base}/maintenance/log"), &op, json!({ "kind": "rara", "title": "x" })).await.status, 400);
    assert_eq!(s.post(&format!("{base}/maintenance/log"), &op, json!({ "kind": "nota", "title": "Cambio de tubo" })).await.status, 201);
    let f = s.post(&format!("{base}/faults"), &eva, json!({ "description": "No enciende el extractor" })).await.json();
    let fid = f["id"].as_str().unwrap();
    assert_eq!(s.post(&format!("/api/machines/faults/{}/resolve", fid), &op, json!({ "note": "Fusible cambiado" })).await.status, 200);
    let log = s.get(&format!("{base}/maintenance"), &ana).await.json()["log"].clone();
    let kinds: Vec<&str> = log.as_array().unwrap().iter().map(|l| l["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["reparacion", "nota", "mantencion"]);
    assert_eq!(log[0]["note"], "Fusible cambiado");

    // Vista general (staff) y quitar una tarea conserva la bitacora
    let all = s.get("/api/machines/maintenance", &op).await.json();
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert_eq!(s.delete(&format!("/api/machines/maintenance/tasks/{}", lente_id), &ana).await.status, 403);
    assert_eq!(s.delete(&format!("/api/machines/maintenance/tasks/{}", lente_id), &op).await.status, 204);
    let ov = s.get(&format!("{base}/maintenance"), &op).await.json();
    assert_eq!(ov["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(ov["log"].as_array().unwrap().len(), 3);
}
