//! Roles editables y permisos por modulo (ver roles.rs y middleware::required_access)

use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plantillas_y_niveles() {
    let s = Server::start().await;
    let admin = s.admin().await;

    let r = s.get("/api/roles", &admin).await;
    assert_eq!(r.status, 200);
    let roles = r.json();
    let ids: Vec<&str> = roles["roles"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    for id in ["admin", "operador", "docente", "estudiante", "externo", "auditor", "tecnico", "ayudante"] {
        assert!(ids.contains(&id), "falta la plantilla {id}");
    }
    let est = roles["roles"].as_array().unwrap().iter().find(|r| r["id"] == "estudiante").unwrap();
    assert_eq!(est["levels"]["machines"], "usar");
    assert_eq!(est["levels"]["terminal"], "ninguno");

    // Externo: reserva maquinas, no ve archivos ni tareas
    let ext = s.user(&admin, "ext", "externo").await;
    assert_eq!(s.get("/api/machines", &ext).await.status, 200);
    assert_eq!(s.get("/api/files", &ext).await.status, 403);
    assert_eq!(s.get("/api/tasks", &ext).await.status, 403);
    let me = s.get("/api/auth/me", &ext).await.json();
    assert_eq!(me["role_name"], "Usuario externo");
    assert_eq!(me["levels"]["files"], "ninguno");
    let files = me["enabled_modules"].as_array().unwrap().iter().find(|m| m["id"] == "files").unwrap().clone();
    assert_eq!(files["enabled"], false, "el menu no muestra lo que el rol no ve");

    // Excepcion por persona: Tareas en Ver
    let ok = s.post("/api/auth/users/ext/role", &admin, json!({ "role": "externo", "overrides": { "tasks": "ver" } })).await;
    assert_eq!(ok.status, 200);
    assert_eq!(s.get("/api/tasks", &ext).await.status, 200, "la sesion abierta toma la excepcion");
    assert_eq!(s.post("/api/tasks", &ext, json!({ "title": "x" })).await.status, 403, "ver no es usar");

    // Auditor: lee, no escribe
    let aud = s.user(&admin, "aud", "auditor").await;
    assert_eq!(s.get("/api/notes", &aud).await.status, 200);
    assert_eq!(s.post("/api/notes", &aud, json!({ "title": "x", "content": "y" })).await.status, 403);
    assert_eq!(s.get("/api/terminal", &aud).await.status, 403);

    // Rol inexistente
    assert_eq!(s.post("/api/auth/users/aud/role", &admin, json!({ "role": "jefe" })).await.status, 400);
    // Roles solo para el admin
    assert_eq!(s.get("/api/roles", &aud).await.status, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crear_editar_y_borrar_un_rol() {
    let s = Server::start().await;
    let admin = s.admin().await;

    let r = s.post("/api/roles", &admin, json!({ "name": "Jefe de Taller", "copy_from": "estudiante" })).await;
    assert_eq!(r.status, 200);
    let role = r.json();
    assert_eq!(role["id"], "jefe_de_taller");
    assert_eq!(role["levels"]["machines"], "usar", "copia los niveles");

    let pepe = s.user(&admin, "pepe", "jefe_de_taller").await;
    assert_eq!(s.get("/api/notes", &pepe).await.status, 200);

    // Subir Maquinas a gestionar y quitar Notas: la sesion abierta lo toma al instante
    let mut levels = role["levels"].clone();
    levels["machines"] = json!("gestionar");
    levels["notes"] = json!("ninguno");
    let r = s.put("/api/roles/jefe_de_taller", &admin, json!({ "name": "Jefe de taller", "levels": levels })).await;
    assert_eq!(r.status, 200);
    assert_eq!(s.get("/api/notes", &pepe).await.status, 403);
    assert_eq!(s.get("/api/auth/me", &pepe).await.json()["levels"]["machines"], "gestionar");

    // El admin no se edita
    assert_eq!(s.put("/api/roles/admin", &admin, json!({ "name": "x", "levels": {} })).await.status, 400);
    // En uso: no se borra
    assert_eq!(s.delete("/api/roles/jefe_de_taller", &admin).await.status, 409);
    s.post("/api/auth/users/pepe/role", &admin, json!({ "role": "observador" })).await;
    assert_eq!(s.delete("/api/roles/jefe_de_taller", &admin).await.status, 204);

    // Restaurar plantilla
    let mut levels = json!({});
    levels["terminal"] = json!("gestionar");
    s.put("/api/roles/docente", &admin, json!({ "name": "Docente", "levels": levels })).await;
    let r = s.post("/api/roles/docente/reset", &admin, json!({})).await;
    assert_eq!(r.json()["levels"]["terminal"], "ninguno");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cuenta_con_fecha_de_termino() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let aud = s.user(&admin, "aud", "auditor").await;
    assert_eq!(s.get("/api/notes", &aud).await.status, 200);

    let past = chrono::Utc::now().timestamp() - 10;
    let r = s.post("/api/auth/users/aud/role", &admin, json!({ "role": "auditor", "expires_at": past })).await;
    assert_eq!(r.status, 200);
    assert_eq!(s.get("/api/notes", &aud).await.status, 401, "la sesion abierta se corta");
    let login = s.req(reqwest::Method::POST, "/api/auth/login", None, Some(json!({ "username": "aud", "password": "aud-password" }))).await;
    assert_eq!(login.status, 403);

    // El admin no puede vencer
    let r = s.post("/api/auth/users/admin/role", &admin, json!({ "role": "admin", "expires_at": past })).await;
    assert_eq!(r.status, 400);
}
