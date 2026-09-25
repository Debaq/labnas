use futures_util::SinkExt;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

use super::harness::{events, kinds, Server};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tickets_de_websocket() {
    let s = Server::start().await;
    let admin = s.admin().await;

    let ticket = s.req(reqwest::Method::POST, "/api/live/ticket", Some(&admin), None).await.json();
    let ticket = ticket.as_str().unwrap();
    assert_eq!(ticket.len(), 64);
    assert_eq!(s.ws_status(&format!("/api/live?ticket={}", ticket)).await, 101);
    assert_eq!(s.ws_status(&format!("/api/live?ticket={}", ticket)).await, 401, "un solo uso");
    assert_eq!(s.ws_status(&format!("/api/terminal?token={}", admin)).await, 401, "token de sesion en URL");
    assert_eq!(s.req(reqwest::Method::POST, "/api/live/ticket", None, None).await.status, 401);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audiencias_y_aprobacion() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let bob = s.register("bob", "bobbobbob").await.json()["token"].as_str().unwrap().to_string();
    let mut ws_admin = s.live(&admin).await;
    let mut ws_bob = s.live(&bob).await;

    // nuevo pendiente: aviso a admins, no a otro pendiente
    s.register("carol", "carolcarol").await;
    let ev = events(&mut ws_admin, 800).await;
    let notify = ev.iter().find(|e| e["kind"] == "notify").expect("aviso");
    assert_eq!(notify["data"]["title"], "Usuario pendiente de aprobacion");
    assert!(events(&mut ws_bob, 300).await.is_empty());

    // aprobar a bob: solo el recibe auth.changed
    s.post("/api/auth/users/bob/role", &admin, json!({ "role": "observador" })).await;
    assert_eq!(kinds(&events(&mut ws_bob, 800).await), vec!["auth.changed"]);
    assert!(!kinds(&events(&mut ws_admin, 300).await).contains(&"auth.changed".to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sensores_y_modulos() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let bob = s.user(&admin, "bob", "observador").await;
    let mut ws = s.live(&bob).await;
    let reading = json!({ "mac": "AA:BB:CC:DD:EE:FF", "readings": [{ "key": "temp", "val": 21.5 }] });

    let r = s.req(reqwest::Method::POST, "/api/sensors/data", None, Some(reading.clone())).await;
    assert_eq!(r.status, 200);
    assert!(kinds(&events(&mut ws, 800).await).contains(&"sensors.updated".to_string()));

    // modulo desactivado: no hay eventos
    s.put("/api/modules/sensors", &admin, json!({ "enabled": false })).await;
    events(&mut ws, 200).await;
    s.req(reqwest::Method::POST, "/api/sensors/data", None, Some(reading)).await;
    assert!(!kinds(&events(&mut ws, 600).await).iter().any(|k| k.starts_with("sensors")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn musica_por_interes() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let bob = s.user(&admin, "bob", "observador").await;
    let mut ws = s.live(&bob).await;

    // sin interes declarado no llega music.state
    s.post("/api/music/volume", &bob, json!({ "volume": 30 })).await;
    assert!(!kinds(&events(&mut ws, 1500).await).contains(&"music.state".to_string()));

    ws.send(Message::Text(json!({ "sub": "music.state" }).to_string().into())).await.unwrap();
    s.post("/api/music/volume", &bob, json!({ "volume": 42 })).await;
    let ev = events(&mut ws, 2500).await;
    let last = ev.iter().filter(|e| e["kind"] == "music.state").last().expect("music.state");
    assert_eq!(last["data"]["volume"], 42);
}
