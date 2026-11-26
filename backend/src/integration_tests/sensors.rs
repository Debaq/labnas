use reqwest::Method;
use serde_json::{json, Value};

use super::harness::Server;

const MAC: &str = "AA:BB:CC:00:11:22";

fn reading(extra: Value) -> Value {
    let mut body = json!({ "mac": MAC, "readings": [{ "key": "temp", "val": 20.0 }] });
    if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
        b.extend(e.clone());
    }
    body
}

async fn send(s: &Server, body: Value, header: Option<&str>) -> u16 {
    let mut r = s.client().post(format!("{}/api/sensors/data", s.base)).json(&body);
    if let Some(t) = header {
        r = r.header("X-Sensor-Token", t);
    }
    s.raw(r).await.status
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn token_por_dispositivo() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let obs = s.user(&admin, "obs", "observador").await;

    // modo compatible: sin token se acepta (y se autoregistra)
    assert_eq!(send(&s, reading(json!({})), None).await, 200);
    let devices = s.get("/api/sensors/devices", &admin).await.json();
    let id = devices[0]["id"].as_str().unwrap().to_string();
    assert_eq!(devices[0]["has_token"], false);

    // solo admin genera tokens
    assert_eq!(s.post(&format!("/api/sensors/devices/{}/token", id), &obs, json!({})).await.status, 403);
    let token = s.post(&format!("/api/sensors/devices/{}/token", id), &admin, json!({})).await.json()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(s.get("/api/sensors/devices", &admin).await.json()[0]["has_token"], true);

    // con token: se exige siempre (header o campo del JSON)
    assert_eq!(send(&s, reading(json!({})), None).await, 401, "sin token");
    assert_eq!(send(&s, reading(json!({})), Some("otro")).await, 401, "token incorrecto");
    assert_eq!(send(&s, reading(json!({})), Some(&token)).await, 200, "header");
    assert_eq!(send(&s, reading(json!({ "token": token })), None).await, 200, "campo JSON");

    // la base guarda solo el hash
    let conn = rusqlite::Connection::open(s.home.join(".labnas/labnas.db")).unwrap();
    let stored: String = conn.query_row("SELECT token_hash FROM sensor_devices", [], |r| r.get(0)).unwrap();
    assert_ne!(stored, token);
    assert_eq!(stored.len(), 64);

    // revocar vuelve al modo compatible
    assert_eq!(s.delete(&format!("/api/sensors/devices/{}/token", id), &admin).await.status, 204);
    assert_eq!(send(&s, reading(json!({})), None).await, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exigir_token_bloquea_sin_token_y_desconocidos() {
    let s = Server::start().await;
    let admin = s.admin().await;

    assert_eq!(s.get("/api/sensors/security", &admin).await.json()["require_token"], false);
    let r = s.req(Method::PUT, "/api/sensors/security", Some(&admin), Some(json!({ "require_token": true }))).await;
    assert_eq!(r.status, 200);

    // dispositivo desconocido: no se autoregistra
    assert_eq!(send(&s, reading(json!({})), None).await, 401);
    assert_eq!(s.get("/api/sensors/devices", &admin).await.json().as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn alarma_critica_con_incidente() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "operador").await;
    let obs = s.user(&admin, "obs", "observador").await;

    // Freezer aceptado
    assert_eq!(send(&s, reading(json!({})), None).await, 200);
    let id = s.get("/api/sensors/devices", &admin).await.json()[0]["id"].as_str().unwrap().to_string();
    s.put(&format!("/api/sensors/devices/{}/status", id), &admin, json!({ "status": "accepted" })).await;

    // Alerta critica: lista con una cuenta inexistente no se acepta
    let bad = json!({ "device_id": id, "key": "temp", "condition": "above", "threshold": -15.0, "critical": true, "escalation": ["nadie"] });
    assert_eq!(s.post("/api/sensors/alerts", &admin, bad).await.status, 400);
    let alert = json!({ "device_id": id, "key": "temp", "condition": "above", "threshold": -15.0, "critical": true, "escalation": ["ana", "obs"], "escalate_minutes": 5 });
    let a = s.post("/api/sensors/alerts", &admin, alert).await;
    assert_eq!(a.status, 200);
    assert_eq!(a.json()["escalation"], json!(["ana", "obs"]));

    // Sube a -10: incidente abierto, avisada solo la primera
    let hot = json!({ "mac": MAC, "readings": [{ "key": "temp", "val": -10.0 }] });
    assert_eq!(send(&s, hot.clone(), None).await, 200);
    let open = s.get("/api/sensors/incidents?open=true", &obs).await.json();
    assert_eq!(open.as_array().unwrap().len(), 1);
    assert_eq!(open[0]["notified"], json!(["ana"]));
    let inc = open[0]["id"].as_str().unwrap().to_string();
    // Otra lectura alta no abre otro incidente, anota el peor valor
    send(&s, json!({ "mac": MAC, "readings": [{ "key": "temp", "val": -5.0 }] }), None).await;
    let open = s.get("/api/sensors/incidents?open=true", &obs).await.json();
    assert_eq!(open.as_array().unwrap().len(), 1);
    assert_eq!(open[0]["worst_value"], -5.0);

    // «Lo vi» desde la web
    let ack = s.post(&format!("/api/sensors/incidents/{}/ack", inc), &ana, json!({})).await;
    assert_eq!(ack.status, 200);
    assert_eq!(ack.json()["acked_by"], "ana");
    assert_eq!(s.post(&format!("/api/sensors/incidents/{}/ack", inc), &obs, json!({})).await.status, 404, "ya confirmada");

    // Vuelve a rango: se cierra
    send(&s, json!({ "mac": MAC, "readings": [{ "key": "temp", "val": -20.0 }] }), None).await;
    assert_eq!(s.get("/api/sensors/incidents?open=true", &obs).await.json().as_array().unwrap().len(), 0);
    let all = s.get("/api/sensors/incidents", &obs).await.json();
    assert!(all[0]["resolved_at"].is_string());
    let log = s.get("/api/audit?limit=100", &admin).await.text;
    assert!(log.contains("Alarma critica abierta") && log.contains("Confirmo la alarma critica") && log.contains("Alarma critica cerrada"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exportar_historial_y_informe() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let obs = s.user(&admin, "obs", "observador").await;

    for v in [-20.0, -18.0, -10.0, -19.0] {
        let body = json!({ "mac": MAC, "readings": [{ "key": "temp", "val": v }] });
        assert_eq!(send(&s, body, None).await, 200);
    }
    let id = s.get("/api/sensors/devices", &admin).await.json()[0]["id"].as_str().unwrap().to_string();
    s.put(&format!("/api/sensors/devices/{}/status", id), &admin, json!({ "status": "accepted" })).await;
    let alert = json!({ "device_id": id, "key": "temp", "condition": "above", "threshold": -15.0, "critical": true, "escalation": ["obs"] });
    assert_eq!(s.post("/api/sensors/alerts", &admin, alert).await.status, 200);
    // lectura fuera de rango tras crear la alerta: abre un incidente
    send(&s, json!({ "mac": MAC, "readings": [{ "key": "temp", "val": -5.0 }] }), None).await;

    let hoy = chrono::Local::now().format("%Y-%m-%d").to_string();
    let q = format!("from={}&to={}", hoy, hoy);

    // CSV: adjunto, cabecera y una fila por lectura
    let csv = s.get(&format!("/api/sensors/devices/{}/export?{}&format=csv", id, q), &obs).await;
    assert_eq!(csv.status, 200);
    assert!(csv.header("content-disposition").starts_with("attachment"));
    assert!(csv.header("content-type").starts_with("text/csv"));
    let lines: Vec<&str> = csv.text.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(lines[0], "fecha_hora_local,clave,valor,unidad");
    assert_eq!(lines.len(), 6);
    assert!(lines[1].contains(",temp,-20,"));

    // Informe: resumen por clave y alerta
    let rep = s.get(&format!("/api/sensors/devices/{}/report?{}", id, q), &obs).await.json();
    assert_eq!(rep["total_readings"], 5);
    let k = &rep["keys"][0];
    assert_eq!(k["key"], "temp");
    assert_eq!(k["count"], 5);
    assert_eq!(k["min"], -20.0);
    assert_eq!(k["max"], -5.0);
    assert!((k["avg"].as_f64().unwrap() - (-14.4)).abs() < 1e-9);
    assert_eq!(rep["alerts"][0]["readings_out_of_range"], 2);
    assert_eq!(rep["alerts"][0]["incidents"], 1);

    // Validaciones: rango invertido, demasiado largo, sensor inexistente
    let base = format!("/api/sensors/devices/{}/export", id);
    assert_eq!(s.get(&format!("{}?from=2026-02-01&to=2026-01-01", base), &obs).await.status, 400);
    assert_eq!(s.get(&format!("{}?from=2024-01-01&to=2026-01-01", base), &obs).await.status, 400);
    assert_eq!(s.get(&format!("{}?from=ayer&to=hoy", base), &obs).await.status, 400);
    assert_eq!(s.get(&format!("/api/sensors/devices/nope/report?{}", q), &obs).await.status, 404);
}
