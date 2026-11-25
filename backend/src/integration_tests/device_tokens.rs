//! Tokens de dispositivo (labnas-tray): alcance limitado a controlar la musica

use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn descarga_de_la_bandeja_desde_la_cache() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "observador").await;
    // Ya descargada (no se toca GitHub en los tests)
    let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    let dir = crate::db::data_dir().join("downloads");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("labnas-tray-{tag}-windows-x86_64.zip")), b"PK-falso").unwrap();

    let r = s.get("/api/music/tray/windows-x86_64", &ana).await;
    assert_eq!(r.status, 200);
    assert!(r.header("content-disposition").contains(&format!("labnas-tray-{tag}-windows-x86_64.zip")));
    assert_eq!(s.get("/api/music/tray/macos-arm64", &ana).await.status, 404);
    assert_eq!(s.get("/api/music/tray/..%2F..%2Fetc", &ana).await.status, 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn token_de_dispositivo_solo_controla_la_musica() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "observador").await;
    let beto = s.user(&admin, "beto", "observador").await;

    let r = s.post("/api/auth/device-tokens", &ana, json!({ "name": "PC de Ana\n" })).await;
    assert_eq!(r.status, 200);
    let created = r.json();
    let token = created["token"].as_str().unwrap().to_string();
    let id = created["id"].as_str().unwrap().to_string();
    assert!(token.starts_with("lnd_"));
    assert_eq!(created["name"], "PC de Ana");

    // Lo que si puede
    assert_eq!(s.get("/api/music/current", &token).await.status, 200);
    assert_eq!(s.post("/api/music/volume", &token, json!({ "volume": 40 })).await.status, 200);
    assert_eq!(s.post("/api/music/pause", &token, json!({})).await.status, 200);
    assert_eq!(s.get("/api/auth/device-tokens/me", &token).await.json()["username"], "ana");

    // Lo que no: otras rutas, ni siquiera de musica, ni crear mas tokens
    assert_eq!(s.post("/api/music/play", &token, json!({ "id": "x" })).await.status, 403);
    assert_eq!(s.get("/api/files", &token).await.status, 403);
    assert_eq!(s.get("/api/auth/me", &token).await.status, 403);
    assert_eq!(s.post("/api/auth/device-tokens", &token, json!({ "name": "otro" })).await.status, 403);

    // La lista no muestra el token; otro usuario no lo ve ni lo borra
    let list = s.get("/api/auth/device-tokens", &ana).await.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(list[0].get("token").is_none());
    assert_eq!(s.get("/api/auth/device-tokens", &beto).await.json().as_array().unwrap().len(), 0);
    assert_eq!(s.delete(&format!("/api/auth/device-tokens/{id}"), &beto).await.status, 404);

    // Token inventado o con formato raro
    let fake = format!("lnd_{}", "0".repeat(48));
    assert_eq!(s.get("/api/music/current", &fake).await.status, 401);
    assert_eq!(s.get("/api/music/current", "lnd_corto").await.status, 401);

    // Si la cuenta queda pendiente, el token deja de servir al instante
    let pend = json!({ "role": "pendiente", "permissions": { "terminal": false, "impresion": false, "archivos_escritura": false } });
    assert_eq!(s.post("/api/auth/users/ana/role", &admin, pend).await.status, 200);
    assert_eq!(s.get("/api/music/current", &token).await.status, 403);
    let obs = json!({ "role": "observador", "permissions": { "terminal": false, "impresion": false, "archivos_escritura": false } });
    assert_eq!(s.post("/api/auth/users/ana/role", &admin, obs).await.status, 200);
    assert_eq!(s.get("/api/music/current", &token).await.status, 200);

    // Revocado: ya no entra
    assert_eq!(s.delete(&format!("/api/auth/device-tokens/{id}"), &ana).await.status, 204);
    assert_eq!(s.get("/api/music/current", &token).await.status, 401);

    // La app se desvincula sola; con sesion normal no aplica
    let t2 = s.post("/api/auth/device-tokens", &ana, json!({ "name": "otro" })).await.json()["token"].as_str().unwrap().to_string();
    assert_eq!(s.delete("/api/auth/device-tokens/me", &ana).await.status, 400);
    assert_eq!(s.delete("/api/auth/device-tokens/me", &t2).await.status, 204);
    assert_eq!(s.get("/api/music/current", &t2).await.status, 401);
    assert_eq!(s.get("/api/auth/device-tokens", &ana).await.json().as_array().unwrap().len(), 0);
}
