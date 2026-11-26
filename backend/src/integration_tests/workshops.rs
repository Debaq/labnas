//! Talleres: inscripcion, cupos, lista de espera, asistencia y certificados (handlers::workshops)

use reqwest::Method;
use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inscripcion_cupos_asistencia_y_certificado() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let inv = s.user(&admin, "ines", "investigador").await;
    let est = s.user(&admin, "eva", "estudiante").await;

    let r = s
        .post("/api/portfolio", &inv, json!({ "id": "x", "created_at": "2026-01-01T00:00:00Z", "title": "Taller de soldadura",
            "entry_type": "workshop", "hours": 6, "start_date": "2026-12-01" }))
        .await;
    assert_eq!(r.status, 200);
    let id = r.json()["id"].as_str().unwrap().to_string();
    let proyecto = s.post("/api/portfolio", &inv, json!({ "id": "y", "created_at": "2026-01-01T00:00:00Z", "title": "No taller" })).await;
    let pid = proyecto.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.get(&format!("/api/portfolio/{pid}/workshop"), &inv).await.status, 404, "solo talleres y cursos");

    // Gestionar: el estudiante no configura
    let cfg = json!({ "capacity": 1, "registration_open": true, "sessions": ["2026-12-01", "2026-12-02", "2026-12-03", "2026-12-04"], "min_attendance": 75 });
    assert_eq!(s.put(&format!("/api/portfolio/{id}/workshop"), &est, cfg.clone()).await.status, 403);
    let r = s.put(&format!("/api/portfolio/{id}/workshop"), &inv, cfg.clone()).await;
    assert_eq!(r.status, 200);
    let token = r.json()["public_token"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 32);

    // Publica: sin sesion
    let url = format!("/api/portfolio/taller/{token}");
    let info = s.req(Method::GET, &url, None, None).await;
    assert_eq!(info.status, 200);
    assert_eq!(info.json()["title"], "Taller de soldadura");
    assert!(!info.text.contains("ines"));
    let bad = s.req(Method::POST, &url, None, Some(json!({ "name": "Ana", "email": "no-es-correo" }))).await;
    assert_eq!(bad.status, 400);
    let a = s.req(Method::POST, &url, None, Some(json!({ "name": "Ana", "email": "Ana@Uach.cl", "affiliation": "Ing" }))).await;
    assert_eq!(a.status, 200);
    assert_eq!(a.json()["status"], "inscrito");
    let dup = s.req(Method::POST, &url, None, Some(json!({ "name": "Ana", "email": "ana@uach.cl" }))).await;
    assert_eq!(dup.status, 409);
    // Cupo lleno: espera
    let b = s.req(Method::POST, &url, None, Some(json!({ "name": "Beto", "email": "beto@uach.cl" }))).await;
    assert_eq!(b.json()["status"], "espera");
    // Un usuario con sesion tambien entra (a la espera)
    let e = s.post(&format!("/api/portfolio/{id}/workshop/enroll"), &est, json!({})).await;
    assert_eq!(e.status, 200);
    assert_eq!(e.json()["status"], "espera");

    // El estudiante no ve la lista ni el enlace
    let v = s.get(&format!("/api/portfolio/{id}/workshop"), &est).await.json();
    assert!(v.get("registrations").is_none() && v.get("public_token").is_none());
    assert_eq!(v["mine"]["status"], "espera");

    // Cancela Ana: sube Beto
    let g = s.get(&format!("/api/portfolio/{id}/workshop"), &inv).await.json();
    let regs = g["registrations"].as_array().unwrap();
    assert_eq!(regs.len(), 3);
    let ana = regs[0]["id"].as_i64().unwrap();
    let beto = regs[1]["id"].as_i64().unwrap();
    assert_eq!(s.post(&format!("/api/portfolio/{id}/workshop/registrations/{ana}/cancel"), &inv, json!({})).await.status, 204);
    let g = s.get(&format!("/api/portfolio/{id}/workshop"), &inv).await.json();
    assert_eq!(g["registrations"][1]["status"], "inscrito");
    assert_eq!(g["registrations"][2]["status"], "espera");

    // Certificado: sin asistencia no
    let cert = format!("/api/portfolio/{id}/workshop/registrations/{beto}/certificate");
    assert_eq!(s.post(&cert, &inv, json!({})).await.status, 409);
    let att = format!("/api/portfolio/{id}/workshop/registrations/{beto}/attendance");
    assert_eq!(s.put(&att, &est, json!({ "attended": [] })).await.status, 403);
    assert_eq!(s.put(&att, &inv, json!({ "attended": ["2026-12-01", "2026-12-02"] })).await.status, 200);
    assert_eq!(s.post(&cert, &inv, json!({})).await.status, 409, "50% < 75%");
    assert_eq!(s.put(&att, &inv, json!({ "attended": ["2026-12-01", "2026-12-02", "2026-12-03"] })).await.status, 200);
    let c = s.post(&cert, &inv, json!({})).await;
    assert_eq!(c.status, 200);
    let code = c.json()["code"].as_str().unwrap().to_string();
    assert_eq!(c.json()["name"], "Beto");

    // Verificacion publica
    let ver = s.req(Method::GET, &format!("/api/portfolio/certificado/{code}"), None, None).await;
    assert_eq!(ver.status, 200);
    assert_eq!(ver.json()["workshop"], "Taller de soldadura");
    assert!(!ver.text.contains("beto@uach.cl"), "la verificacion no filtra el correo");
    assert_eq!(s.req(Method::GET, "/api/portfolio/certificado/AAAA-BBBB-CCCC-DDDD", None, None).await.status, 404);

    // CSV
    let csv = s.get(&format!("/api/portfolio/{id}/workshop/export.csv"), &inv).await;
    assert_eq!(csv.status, 200);
    assert!(csv.text.contains("beto@uach.cl"));
    assert_eq!(s.get(&format!("/api/portfolio/{id}/workshop/export.csv"), &est).await.status, 403);

    // Cerrada: la ruta publica no existe
    let mut closed = cfg.clone();
    closed["registration_open"] = json!(false);
    assert_eq!(s.put(&format!("/api/portfolio/{id}/workshop"), &inv, closed).await.status, 200);
    assert_eq!(s.req(Method::GET, &url, None, None).await.status, 404);
    assert_eq!(s.req(Method::GET, &format!("/api/portfolio/certificado/{code}"), None, None).await.status, 200);

    // Borrar la ficha borra las inscripciones y el certificado deja de existir
    assert_eq!(s.delete(&format!("/api/portfolio/{id}"), &inv).await.status, 204);
    assert_eq!(s.req(Method::GET, &format!("/api/portfolio/certificado/{code}"), None, None).await.status, 404);
}
