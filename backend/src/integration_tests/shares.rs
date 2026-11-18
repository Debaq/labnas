//! Links compartidos (dueño, streaming) y descarga desde URL (anti-SSRF)

use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn links_compartidos_tienen_dueno() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "operador").await;
    let bob = s.user(&admin, "bob", "operador").await;

    std::fs::write(s.home.join("informe ñ.txt"), "contenido").unwrap();
    let r = s.post("/api/shares", &ana, json!({ "path": s.path("informe ñ.txt"), "expires_hours": 1 })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let token = r.json()["token"].as_str().unwrap().to_string();
    assert_eq!(r.json()["url"], format!("/api/share/{}", token), "URL relativa: el cliente pone el origen");

    // bob no ve ni puede borrar el link de ana
    assert_eq!(s.get("/api/shares", &bob).await.json(), json!([]));
    assert_eq!(s.delete(&format!("/api/shares/{}", token), &bob).await.status, 404);
    // ana y el admin si lo ven
    assert_eq!(s.get("/api/shares", &ana).await.json().as_array().unwrap().len(), 1);
    assert_eq!(s.get("/api/shares", &admin).await.json()[0]["owner"], "ana");

    // descarga publica, en streaming, con nombre UTF-8 en filename*
    let r = s.raw(s.client().get(format!("{}/api/share/{}", s.base, token))).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.text, "contenido");
    assert!(r.header("content-disposition").contains("filename*=UTF-8''informe%20%C3%B1.txt"));

    assert_eq!(s.delete(&format!("/api/shares/{}", token), &ana).await.status, 204);
    let r = s.raw(s.client().get(format!("{}/api/share/{}", s.base, token))).await;
    assert_eq!(r.status, 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn descarga_url_no_alcanza_la_red_local() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let dest = s.path("");

    // El propio NAS, localhost por nombre, metadatos cloud, LAN y esquemas raros
    let objetivo_local = format!("{}/api/health", s.base);
    for url in [
        objetivo_local.as_str(),
        "http://localhost:3001/api/health",
        "http://169.254.169.254/latest/meta-data/",
        "http://192.168.1.1/",
        "http://[::1]/",
        "file:///etc/passwd",
    ] {
        let r = s.post("/api/download-url", &admin, json!({ "url": url, "destination": dest })).await;
        assert_eq!(r.status, 400, "{} deberia rechazarse: {}", url, r.text);
    }
    // No quedo ningun archivo ni temporal .part
    let quedan: Vec<_> = std::fs::read_dir(&s.home)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != ".labnas")
        .collect();
    assert!(quedan.is_empty(), "{:?}", quedan);
}
