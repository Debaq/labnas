//! Niveles de acceso: privado / con autorizacion / publico (handlers::access)

use reqwest::Method;
use serde_json::json;

use super::harness::Server;

fn enc(p: &str) -> String {
    urlencoding::encode(p).into_owned()
}

fn in_days(d: i64) -> i64 {
    chrono::Utc::now().timestamp() + d * 86400
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn proyecto_privado_autorizado_y_publico() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let inv = s.user(&admin, "ines", "investigador").await;
    let est = s.user(&admin, "eva", "estudiante").await;
    let ext = s.user(&admin, "fin", "externo").await;

    let proyecto = json!({ "id": "p1", "created_at": "2026-01-01T00:00:00Z", "title": "FONDEF Sensores",
        "budget": 12500000.0, "funding_source": "ANID", "contact": "secreto@uach.cl", "notes": "interno",
        "status": "active" });
    let r = s.post("/api/portfolio", &inv, proyecto).await;
    assert_eq!(r.status, 200);
    let pid = r.json()["id"].as_str().unwrap().to_string();

    // Privado: nadie de afuera lo ve; el externo no tiene Portafolio
    let a = s.get(&format!("/api/access?kind=proyecto&id={pid}"), &inv).await;
    assert_eq!(a.status, 200);
    assert_eq!(a.json()["visibility"], "privado");
    assert_eq!(s.get(&format!("/api/shared/view?kind=proyecto&id={pid}"), &ext).await.status, 404);
    // Estudiante ve el Portafolio pero no gestiona: no decide quien lo ve
    assert_eq!(s.get(&format!("/api/access?kind=proyecto&id={pid}"), &est).await.status, 403);
    assert_eq!(s.put("/api/access", &est, json!({ "kind": "proyecto", "id": pid, "visibility": "publico" })).await.status, 403);

    // Autorizar al externo (de Finanzas) hasta una fecha
    assert_eq!(s.post("/api/access/grants", &inv, json!({ "kind": "proyecto", "id": pid, "username": "fin", "expires_at": in_days(-1) })).await.status, 400, "fecha pasada");
    assert_eq!(s.post("/api/access/grants", &inv, json!({ "kind": "proyecto", "id": pid, "username": "nadie", "expires_at": in_days(10) })).await.status, 400);
    let g = s.post("/api/access/grants", &inv, json!({ "kind": "proyecto", "id": pid, "username": "fin", "expires_at": in_days(10), "note": "rendicion" })).await;
    assert_eq!(g.status, 200);
    assert_eq!(g.json()["visibility"], "autorizado", "autorizar a alguien lo saca de privado");
    let grant_id = g.json()["grants"][0]["id"].as_str().unwrap().to_string();

    let shared = s.get("/api/shared", &ext).await.json();
    assert_eq!(shared.as_array().unwrap().len(), 1);
    assert_eq!(shared[0]["title"], "FONDEF Sensores");
    let v = s.get(&format!("/api/shared/view?kind=proyecto&id={pid}"), &ext).await;
    assert_eq!(v.status, 200);
    let text = v.text.clone();
    assert!(text.contains("$12.500.000") && text.contains("ANID"));
    assert!(!text.contains("secreto@uach.cl") && !text.contains("interno"), "la ficha no lleva contacto ni notas internas");
    // El externo sigue sin el modulo
    assert_eq!(s.get("/api/portfolio", &ext).await.status, 403);
    // Otro sin autorizacion no lo ve
    assert_eq!(s.get(&format!("/api/shared/view?kind=proyecto&id={pid}"), &est).await.status, 404);

    // Insignias de la lista
    let lv = s.get(&format!("/api/access/levels?kind=proyecto&ids={pid}%0Ax"), &est).await.json();
    assert_eq!(lv[&pid], "autorizado");

    // Publico: enlace sin cuenta
    let p = s.put("/api/access", &inv, json!({ "kind": "proyecto", "id": pid, "visibility": "publico" })).await.json();
    let token = p["public_token"].as_str().unwrap().to_string();
    let anon = s.req(Method::GET, &format!("/api/public/{}", token), None, None).await;
    assert_eq!(anon.status, 200);
    assert_eq!(anon.json()["title"], "FONDEF Sensores");
    assert_eq!(s.req(Method::GET, "/api/public/abc", None, None).await.status, 404);

    // Volver a privado: el enlace muere y la autorizacion queda en suspenso
    s.put("/api/access", &inv, json!({ "kind": "proyecto", "id": pid, "visibility": "privado" })).await;
    assert_eq!(s.req(Method::GET, &format!("/api/public/{}", token), None, None).await.status, 404);
    assert_eq!(s.get(&format!("/api/shared/view?kind=proyecto&id={pid}"), &ext).await.status, 404);
    assert_eq!(s.get("/api/shared", &ext).await.json().as_array().unwrap().len(), 0);

    // Quitar la autorizacion
    s.put("/api/access", &inv, json!({ "kind": "proyecto", "id": pid, "visibility": "autorizado" })).await;
    assert_eq!(s.get(&format!("/api/shared/view?kind=proyecto&id={pid}"), &ext).await.status, 200);
    assert_eq!(s.delete(&format!("/api/access/grants/{}", grant_id), &inv).await.status, 204);
    assert_eq!(s.get(&format!("/api/shared/view?kind=proyecto&id={pid}"), &ext).await.status, 404);

    // Todo queda en la auditoria
    let log = s.get("/api/audit?limit=200", &admin).await.text;
    assert!(log.contains("Nivel de acceso") && log.contains("Autorizo a fin"));
    assert!(log.contains("Vio por autorizacion") && log.contains("Vista publica"));

    // Borrar el proyecto borra su nivel
    assert_eq!(s.delete(&format!("/api/portfolio/{pid}"), &inv).await.status, 204);
    assert_eq!(s.get(&format!("/api/access?kind=proyecto&id={pid}"), &inv).await.status, 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nota_propia_y_archivo_publico() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "observador").await;
    let bob = s.user(&admin, "bob", "observador").await;
    let aud = s.user(&admin, "aud", "externo").await;

    // La autora de una nota decide quien la ve; otro sin gestionar, no
    let n = s.post("/api/notes", &ana, json!({ "title": "Acta", "content": "Se acordo comprar" })).await.json();
    let nid = n["id"].as_str().unwrap().to_string();
    assert_eq!(s.get(&format!("/api/access?kind=nota&id={}", nid), &ana).await.status, 200);
    assert_eq!(s.get(&format!("/api/access?kind=nota&id={}", nid), &bob).await.status, 403);
    let g = s.post("/api/access/grants", &ana, json!({ "kind": "nota", "id": nid, "username": "aud", "expires_at": in_days(3) })).await;
    assert_eq!(g.status, 200);
    let v = s.get(&format!("/api/shared/view?kind=nota&id={}", nid), &aud).await.json();
    assert_eq!(v["text"], "Se acordo comprar");

    // Archivo publico: descarga sin cuenta
    std::fs::create_dir_all(s.home.join("Docs")).unwrap();
    std::fs::write(s.home.join("Docs/informe.txt"), "contenido").unwrap();
    let path = s.path("Docs/informe.txt");
    let p = s.put("/api/access", &admin, json!({ "kind": "archivo", "id": path, "visibility": "publico" })).await;
    assert_eq!(p.status, 200);
    let token = p.json()["public_token"].as_str().unwrap().to_string();
    let card = s.req(Method::GET, &format!("/api/public/{}", token), None, None).await.json();
    assert_eq!(card["title"], "informe.txt");
    assert_eq!(card["download"], true);
    let dl = s.req(Method::GET, &format!("/api/public/{}/download", token), None, None).await;
    assert_eq!(dl.status, 200);
    assert_eq!(dl.text, "contenido");
    let lv = s.get(&format!("/api/access/levels?kind=archivo&ids={}", enc(&path)), &admin).await.json();
    assert_eq!(lv[&path], "publico");

    // Modulo apagado: el enlace publico no existe
    assert_eq!(s.put("/api/modules/files", &admin, json!({ "enabled": false })).await.status, 200);
    assert_eq!(s.req(Method::GET, &format!("/api/public/{}", token), None, None).await.status, 404);
    s.put("/api/modules/files", &admin, json!({ "enabled": true })).await;
    assert_eq!(s.req(Method::GET, &format!("/api/public/{}", token), None, None).await.status, 200);
}
