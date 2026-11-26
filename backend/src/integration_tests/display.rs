//! Pantalla del laboratorio (handlers::display)

use reqwest::Method;
use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pantalla_por_token() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;

    // Solo el admin administra pantallas
    assert_eq!(s.get("/api/display/screens", &op).await.status, 403);
    assert_eq!(s.post("/api/display/screens", &admin, json!({ "name": "", "sections": ["maquinas"] })).await.status, 400);
    assert_eq!(s.post("/api/display/screens", &admin, json!({ "name": "TV", "sections": ["nada"] })).await.status, 400);
    let r = s.post("/api/display/screens", &admin, json!({ "name": "TV entrada", "sections": ["maquinas", "agenda", "avisos", "plazos", "camaras"] })).await;
    assert_eq!(r.status, 201);
    let token = r.json()["token"].as_str().unwrap().to_string();
    assert_eq!(r.json()["sections"].as_array().unwrap().len(), 4, "las secciones desconocidas se descartan");

    // Datos para la pantalla: maquina en uso, falla como aviso
    let m = s.post("/api/machines", &op, json!({ "name": "Laser", "kind": "laser" })).await.json()["id"].as_str().unwrap().to_string();
    s.post(&format!("/api/machines/{}/faults", m), &op, json!({ "description": "Sin extractor" })).await;

    let d = s.req(Method::GET, &format!("/api/pantalla/{}", token), None, None).await;
    assert_eq!(d.status, 200, "sin sesion");
    let d = d.json();
    assert_eq!(d["name"], "TV entrada");
    assert_eq!(d["machines"][0]["name"], "Laser");
    assert_eq!(d["machines"][0]["state"], "fuera_servicio");
    assert!(d["notices"].as_array().unwrap().iter().any(|n| n["text"].as_str().unwrap().contains("Sin extractor")));

    // Un modulo apagado saca su seccion
    s.put("/api/modules/machines", &admin, json!({ "enabled": false, "force": true })).await;
    let d = s.req(Method::GET, &format!("/api/pantalla/{}", token), None, None).await.json();
    assert_eq!(d["sections"], json!(["plazos"]));
    s.put("/api/modules/machines", &admin, json!({ "enabled": true })).await;

    // La lista muestra cuando se vio; revocar mata el enlace
    let list = s.get("/api/display/screens", &admin).await.json();
    assert!(list[0]["last_seen_at"].is_string());
    assert_eq!(s.req(Method::GET, "/api/pantalla/abc", None, None).await.status, 404);
    assert_eq!(s.delete(&format!("/api/display/screens/{}", token), &admin).await.status, 204);
    assert_eq!(s.req(Method::GET, &format!("/api/pantalla/{}", token), None, None).await.status, 404);
}
