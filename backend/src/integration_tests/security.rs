use reqwest::Method;
use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registro_y_aprobacion() {
    let s = Server::start().await;
    let admin = s.admin().await;

    assert_eq!(s.register("x1", "1234").await.status, 400, "password corta");

    let bob = s.register("bob", "bobbobbob").await;
    assert_eq!(bob.json()["role"], "pendiente");
    let bob = bob.json()["token"].as_str().unwrap().to_string();
    assert_eq!(s.get("/api/files", &bob).await.status, 403, "pendiente no usa la API");
    assert_eq!(s.get("/api/auth/me", &bob).await.status, 200, "pendiente ve su cuenta");

    assert_eq!(s.post("/api/auth/users/bob/role", &admin, json!({ "role": "observador" })).await.status, 200);
    assert_eq!(s.get("/api/auth/me", &bob).await.json()["role"], "observador");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn observador_no_sale_de_las_raices_ni_escribe() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let bob = s.user(&admin, "bob", "observador").await;
    std::fs::write(s.root.join("fuera.txt"), "secreto").unwrap();
    let db = s.path(".labnas/labnas.db");
    let fuera = s.root.join("fuera.txt").to_string_lossy().to_string();
    let escape = format!("{}/../fuera.txt", s.home.display());

    for path in ["/etc/passwd", fuera.as_str(), db.as_str(), escape.as_str()] {
        let r = s.get(&format!("/api/files/download?path={}", urlencoding::encode(path)), &bob).await;
        assert_eq!(r.status, 403, "descarga de {}", path);
    }
    assert_eq!(s.get("/api/files?path=/etc", &bob).await.status, 403);

    let home = s.path("");
    let cases = [
        (Method::POST, "/api/download-url", json!({ "url": "http://example.com/x", "destination": home })),
        (Method::POST, "/api/system/upload-limit", json!({ "limit_mb": 500 })),
        (Method::POST, "/api/music/mpv-args", json!(["--script=/tmp/x.lua"])),
        (Method::POST, "/api/system/services", json!({})),
        (Method::POST, "/api/printing/print-file", json!({ "printer": "x", "path": "/etc/shadow" })),
        (Method::PUT, "/api/files/roots", json!({ "roots": ["/"] })),
    ];
    for (method, path, body) in cases {
        let r = s.req(method.clone(), path, Some(&bob), Some(body)).await;
        assert_eq!(r.status, 403, "{} {}", method, path);
    }

    // el token de sesion no se acepta por URL
    let r = s.req(Method::GET, &format!("/api/files?token={}", admin), None, None).await;
    assert_eq!(r.status, 401);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn archivos_dentro_de_las_raices() {
    let s = Server::start().await;
    let admin = s.admin().await;
    std::fs::create_dir_all(s.home.join("Docs")).unwrap();
    std::fs::write(s.root.join("fuera.txt"), "secreto").unwrap();

    // nombre con ../ se sanea: queda dentro de la carpeta destino
    let form = reqwest::multipart::Form::new()
        .text("path", s.path("Docs"))
        .part("file", reqwest::multipart::Part::bytes(b"hola".to_vec()).file_name("../../../escape.txt"));
    let r = s.raw(s.client().post(format!("{}/api/files/upload", s.base)).bearer_auth(&admin).multipart(form)).await;
    assert_eq!(r.status, 201);
    assert!(!s.root.join("escape.txt").exists());
    assert!(s.home.join("Docs/escape.txt").exists());

    let r = s.get(&format!("/api/files/download?path={}", urlencoding::encode(&s.path("Docs/escape.txt"))), &admin).await;
    assert_eq!(r.text, "hola");

    let fuera = s.root.join("fuera.txt").to_string_lossy().to_string();
    assert_eq!(s.get(&format!("/api/files/download?path={}", urlencoding::encode(&fuera)), &admin).await.status, 403);
    assert_eq!(s.delete(&format!("/api/files?path={}", urlencoding::encode(&s.path(""))), &admin).await.status, 403, "no se borra una raiz");
    assert_eq!(s.post("/api/shares", &admin, json!({ "path": fuera })).await.status, 403);

    // "/" muestra las raices y ~/.labnas no aparece en el listado
    let roots = s.get("/api/files?path=/", &admin).await.json();
    assert_eq!(roots[0]["path"], s.path("").trim_end_matches('/'));
    let listing = s.get(&format!("/api/files?path={}", urlencoding::encode(&s.path(""))), &admin).await.json();
    assert!(listing.as_array().unwrap().iter().all(|e| e["name"] != ".labnas"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bloqueo_tras_intentos_fallidos() {
    let s = Server::start().await;
    let admin = s.admin().await;
    s.user(&admin, "bob", "observador").await;
    for _ in 0..5 {
        let r = s.req(Method::POST, "/api/auth/login", None, Some(json!({ "username": "bob", "password": "malmalmal" }))).await;
        assert_eq!(r.status, 401);
    }
    let r = s.req(Method::POST, "/api/auth/login", None, Some(json!({ "username": "bob", "password": "bob-password" }))).await;
    assert_eq!(r.status, 429, "bloqueado aunque ahora la clave sea correcta");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sesiones_sobreviven_reinicio() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let db = s.home.join(".labnas/labnas.db");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&db).unwrap().permissions().mode() & 0o777, 0o600);

    let s = s.restart().await;
    assert_eq!(s.get("/api/auth/me", &admin).await.status, 200);

    // cambiar la contraseña cierra las otras sesiones
    let other = s.req(Method::POST, "/api/auth/login", None, Some(json!({ "username": "admin", "password": "adminadmin" }))).await;
    let other = other.json()["token"].as_str().unwrap().to_string();
    let r = s.post("/api/auth/password", &admin, json!({ "current_password": "adminadmin", "new_password": "nuevaclave" })).await;
    assert_eq!(r.status, 200);
    assert_eq!(s.get("/api/auth/me", &other).await.status, 401);
    assert_eq!(s.get("/api/auth/me", &admin).await.status, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sin_cors_para_otros_origenes() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let r = s
        .raw(s.client().get(format!("{}/api/auth/me", s.base)).bearer_auth(&admin).header("Origin", "http://evil.example"))
        .await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("access-control-allow-origin"), "", "no se autoriza a otros origenes");
}
