//! Seguridad laboral: capacitaciones, fichas de seguridad e incidentes (handlers::safety)

use serde_json::json;

use super::harness::Server;

// PNG minimo valido (solo importa la firma)
const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacitaciones_con_vigencia_y_plazos() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let ana = s.user(&admin, "ana", "estudiante").await;

    // Catalogo: solo quien gestiona
    let body = json!({ "name": "Uso de extintores", "valid_months": 12, "mandatory": true });
    assert_eq!(s.post("/api/safety/trainings", &ana, body.clone()).await.status, 403);
    let r = s.post("/api/safety/trainings", &op, body.clone()).await;
    assert_eq!(r.status, 201);
    let tid = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.post("/api/safety/trainings", &op, body).await.status, 409, "nombre repetido");
    assert_eq!(s.post("/api/safety/trainings", &op, json!({ "name": "X", "valid_months": 0 })).await.status, 400);

    // Antes de registrarla, a Ana le falta
    let mine = s.get("/api/safety/trainings/mine", &ana).await.json();
    assert_eq!(mine[0]["status"], "pendiente");
    assert_eq!(s.get("/api/safety/summary", &ana).await.json()["my_missing_mandatory"], 1);

    // Registrarla: Ana no puede; el vencimiento sale de la vigencia
    let grant = json!({ "training_id": tid, "username": "ana" });
    assert_eq!(s.post("/api/safety/records", &ana, grant.clone()).await.status, 403);
    assert_eq!(s.post("/api/safety/records", &op, json!({ "training_id": tid, "username": "nadie" })).await.status, 400);
    let r = s.post("/api/safety/records", &op, grant).await;
    assert_eq!(r.status, 200);
    let rec = r.json();
    assert_eq!(rec["status"], "vigente");
    assert!(rec["expires_on"].is_string());
    assert_eq!(s.get("/api/safety/trainings/mine", &ana).await.json()[0]["status"], "vigente");
    assert_eq!(s.get("/api/safety/summary", &ana).await.json()["my_missing_mandatory"], 0);

    // Ana solo ve sus registros; el staff ve todos
    assert_eq!(s.get("/api/safety/records", &ana).await.json().as_array().unwrap().len(), 1);
    assert_eq!(s.get("/api/safety/records", &op).await.json().as_array().unwrap().len(), 1);

    // Sale en Plazos
    let plazos = s.get("/api/deadlines", &op).await.text;
    assert!(plazos.contains("capacitacion") && plazos.contains("Uso de extintores"));

    // Una ya vencida queda "vencida" y vuelve a faltar
    let r = s.post("/api/safety/records", &op, json!({ "training_id": tid, "username": "ana", "granted_on": "2020-01-01", "expires_on": "2020-06-01" })).await;
    assert_eq!(r.json()["status"], "vencida");
    assert_eq!(s.get("/api/safety/trainings/mine", &ana).await.json()[0]["status"], "vencida");
    assert_eq!(s.get("/api/safety/summary", &ana).await.json()["my_missing_mandatory"], 1);

    assert_eq!(s.delete(&format!("/api/safety/records/{}/ana", tid), &ana).await.status, 403);
    assert_eq!(s.delete(&format!("/api/safety/records/{}/ana", tid), &op).await.status, 204);
    assert_eq!(s.delete(&format!("/api/safety/trainings/{}", tid), &op).await.status, 204);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fichas_con_archivo() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let ana = s.user(&admin, "ana", "estudiante").await;

    let r = s.post("/api/machines", &op, json!({ "name": "Laser CO2", "kind": "laser" })).await;
    let mid = r.json()["id"].as_str().unwrap().to_string();

    assert_eq!(s.post("/api/safety/sheets", &ana, json!({ "title": "Resina", "content": "Usar guantes" })).await.status, 403);
    assert_eq!(s.post("/api/safety/sheets", &op, json!({ "title": "Vacia" })).await.status, 400, "sin texto, enlace ni archivo");
    assert_eq!(s.post("/api/safety/sheets", &op, json!({ "title": "X", "url": "javascript:alert(1)" })).await.status, 400);
    assert_eq!(s.post("/api/safety/sheets", &op, json!({ "title": "X", "content": "a", "machine_id": "zzz" })).await.status, 400);
    assert_eq!(s.post("/api/safety/sheets", &op, json!({ "title": "X", "file": { "name": "a.pdf", "data": "aGVsbG8=" } })).await.status, 400, "no es PDF ni imagen");

    let r = s
        .post("/api/safety/sheets", &op, json!({ "title": "Laser: ventilacion", "kind": "procedimiento", "content": "Encender el extractor", "machine_id": mid, "file": { "name": "foto.png", "data": PNG_B64 } }))
        .await;
    assert_eq!(r.status, 201);
    let sheet = r.json();
    assert_eq!(sheet["machine_name"], "Laser CO2");
    assert_eq!(sheet["file_mime"], "image/png");
    let sid = sheet["id"].as_str().unwrap().to_string();

    // Todos las ven y bajan el archivo
    let list = s.get("/api/safety/sheets", &ana).await.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    let f = s.get(&format!("/api/safety/sheets/{}/file", sid), &ana).await;
    assert_eq!(f.status, 200);

    // Editar sin archivo lo conserva; quitarlo exige que quede otra cosa (hay texto)
    let upd = json!({ "title": "Laser: ventilacion", "kind": "procedimiento", "content": "Texto nuevo" });
    let r = s.put(&format!("/api/safety/sheets/{}", sid), &op, upd).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["file_name"], "foto.png");
    let r = s.put(&format!("/api/safety/sheets/{}", sid), &op, json!({ "title": "Laser: ventilacion", "content": "Texto nuevo", "remove_file": true })).await;
    assert!(r.json()["file_name"].is_null());

    assert_eq!(s.delete(&format!("/api/safety/sheets/{}", sid), &ana).await.status, 403);
    assert_eq!(s.delete(&format!("/api/safety/sheets/{}", sid), &op).await.status, 204);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn incidentes_se_cierran_con_nota() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let ana = s.user(&admin, "ana", "estudiante").await;
    let eva = s.user(&admin, "eva", "estudiante").await;
    let aud = s.user(&admin, "aud", "auditor").await;

    // Validaciones
    assert_eq!(s.post("/api/safety/incidents", &ana, json!({ "severity": "rara", "description": "x" })).await.status, 400);
    assert_eq!(s.post("/api/safety/incidents", &ana, json!({ "severity": "leve", "description": "  " })).await.status, 400);
    assert_eq!(s.post("/api/safety/incidents", &ana, json!({ "severity": "leve", "description": "x", "photo": "aGVsbG8=" })).await.status, 400);
    assert_eq!(s.post("/api/safety/incidents", &ana, json!({ "severity": "leve", "description": "x", "occurred_at": "2999-01-01T10:00" })).await.status, 400);
    // El auditor solo ve: no reporta
    assert_eq!(s.post("/api/safety/incidents", &aud, json!({ "severity": "leve", "description": "x" })).await.status, 403);

    let r = s
        .post("/api/safety/incidents", &ana, json!({ "severity": "grave", "description": "Quemadura con el soldador", "place": "Electronica", "photo": PNG_B64 }))
        .await;
    assert_eq!(r.status, 201);
    let inc = r.json();
    assert_eq!(inc["status"], "abierto");
    assert_eq!(inc["has_photo"], true);
    let id = inc["id"].as_str().unwrap().to_string();

    // Visibilidad: la autora y el staff; otra persona no
    assert_eq!(s.get("/api/safety/incidents", &ana).await.json().as_array().unwrap().len(), 1);
    assert_eq!(s.get("/api/safety/incidents", &eva).await.json().as_array().unwrap().len(), 0);
    assert_eq!(s.get("/api/safety/incidents", &op).await.json().as_array().unwrap().len(), 1);
    assert_eq!(s.get(&format!("/api/safety/incidents/{}/photo", id), &ana).await.status, 200);
    assert_eq!(s.get(&format!("/api/safety/incidents/{}/photo", id), &eva).await.status, 403);

    // Revisar y cerrar: solo staff, y el cierre pide nota
    let upd = |status: &str, note: &str| json!({ "status": status, "actions": "Hielo y revision", "closing_note": note });
    assert_eq!(s.put(&format!("/api/safety/incidents/{}", id), &ana, upd("en_revision", "")).await.status, 403);
    let r = s.put(&format!("/api/safety/incidents/{}", id), &op, upd("en_revision", "")).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["status"], "en_revision");
    assert_eq!(s.put(&format!("/api/safety/incidents/{}", id), &op, upd("cerrado", " ")).await.status, 400);
    let r = s.put(&format!("/api/safety/incidents/{}", id), &op, upd("cerrado", "Se compro soporte para el soldador")).await;
    assert_eq!(r.status, 200);
    let c = r.json();
    assert_eq!(c["status"], "cerrado");
    assert_eq!(c["closed_by"], "op");
    assert_eq!(c["actions"], "Hielo y revision");

    // No se borra
    assert_eq!(s.delete(&format!("/api/safety/incidents/{}", id), &op).await.status, 405);
    assert_eq!(s.delete(&format!("/api/safety/incidents/{}", id), &admin).await.status, 405);
    assert_eq!(s.get("/api/safety/summary", &op).await.json()["open_incidents"], 0);
}
