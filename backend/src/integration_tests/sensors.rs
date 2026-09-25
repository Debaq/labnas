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
