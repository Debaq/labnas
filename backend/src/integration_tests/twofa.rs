use reqwest::Method;
use serde_json::{json, Value};

use super::harness::Server;
use crate::totp;

async fn login(s: &Server, user: &str, pass: &str, code: Option<&str>) -> super::harness::Resp {
    let mut body = json!({ "username": user, "password": pass });
    if let Some(c) = code {
        body["code"] = json!(c);
    }
    s.req(Method::POST, "/api/auth/login", None, Some(body)).await
}

async fn dav_status(s: &Server, user: &str, pass: &str) -> u16 {
    let r = s
        .client()
        .request(Method::from_bytes(b"PROPFIND").unwrap(), format!("{}/dav/", s.base))
        .basic_auth(user, Some(pass))
        .header("Depth", "0");
    s.raw(r).await.status
}

fn codes(v: &Value) -> Vec<String> {
    v["recovery_codes"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doble_factor_completo() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "operador").await;
    s.put("/api/files/webdav", &admin, json!({ "enabled": true })).await;
    assert_eq!(dav_status(&s, "ana", "ana-password").await, 207);

    assert_eq!(s.get("/api/auth/2fa", &ana).await.json()["enabled"], false);

    // activar: contrasena -> QR -> codigo
    assert_eq!(s.post("/api/auth/2fa/setup", &ana, json!({ "password": "mala-clave" })).await.status, 401);
    let setup = s.post("/api/auth/2fa/setup", &ana, json!({ "password": "ana-password" })).await.json();
    let secret = setup["secret"].as_str().unwrap().to_string();
    assert!(setup["qr_svg"].as_str().unwrap().contains("<svg"));
    assert!(setup["uri"].as_str().unwrap().starts_with("otpauth://totp/LabNAS%3Aana?"));
    assert_eq!(s.post("/api/auth/2fa/enable", &ana, json!({ "code": "000000" })).await.status, 401);
    let now = totp::current_step();
    let enabled = s.post("/api/auth/2fa/enable", &ana, json!({ "code": totp::code_at(&secret, now).unwrap() })).await;
    assert_eq!(enabled.status, 200);
    let recovery = codes(&enabled.json());
    assert_eq!(recovery.len(), 10);

    let st = s.get("/api/auth/2fa", &ana).await.json();
    assert_eq!(st["enabled"], true);
    assert_eq!(st["recovery_left"], 10);
    assert_eq!(s.get("/api/auth/me", &ana).await.json()["totp_enabled"], true);
    // el secreto se guarda cifrado
    let conn = s.state.db.get().unwrap();
    let stored: String = conn.query_row("SELECT totp_secret FROM web_users WHERE username='ana'", [], |r| r.get(0)).unwrap();
    assert!(stored.starts_with("enc:v1:"));
    drop(conn);

    // login en dos pasos
    let r = login(&s, "ana", "ana-password", None).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["totp_required"], true);
    assert!(r.json()["token"].is_null());
    assert_eq!(login(&s, "ana", "ana-password", Some("123456")).await.status, 401);
    // el codigo usado al activar no sirve de nuevo; el siguiente si
    assert_eq!(login(&s, "ana", "ana-password", Some(&totp::code_at(&secret, now).unwrap())).await.status, 401);
    let ok = login(&s, "ana", "ana-password", Some(&totp::code_at(&secret, now + 1).unwrap())).await;
    assert_eq!(ok.status, 200, "{}", ok.text);
    assert!(ok.json()["token"].is_string());
    // contrasena mala con codigo: no revela nada del codigo
    assert_eq!(login(&s, "ana", "otra-clave", Some("123456")).await.status, 401);

    // codigo de recuperacion: una sola vez (con guiones o sin ellos)
    let rc = recovery[0].to_uppercase().replace('-', "");
    assert_eq!(login(&s, "ana", "ana-password", Some(&rc)).await.status, 200);
    assert_eq!(login(&s, "ana", "ana-password", Some(&recovery[0])).await.status, 401);
    // codigos errados cuentan para el bloqueo: al 5.o ni el correcto entra
    for _ in 0..4 {
        assert_eq!(login(&s, "ana", "ana-password", Some("000000")).await.status, 401);
    }
    assert_eq!(login(&s, "ana", "ana-password", Some(&recovery[1])).await.status, 429);
    s.state.login_failures.lock().await.clear();
    assert_eq!(s.get("/api/auth/2fa", &ana).await.json()["recovery_left"], 9);

    // WebDAV: la contrasena normal ya no sirve; la de aplicacion si
    assert_eq!(dav_status(&s, "ana", "ana-password").await, 401);
    let app = s.post("/api/auth/2fa/app-password", &ana, json!({ "code": recovery[1] })).await.json();
    let app = app["password"].as_str().unwrap().to_string();
    assert_eq!(dav_status(&s, "ana", &app).await, 207);
    assert_eq!(login(&s, "ana", &app, None).await.status, 401, "la de aplicacion no entra a la web");

    // regenerar codigos invalida los anteriores
    let fresh = codes(&s.post("/api/auth/2fa/recovery", &ana, json!({ "code": recovery[2] })).await.json());
    assert_eq!(login(&s, "ana", "ana-password", Some(&recovery[3])).await.status, 401);
    s.state.login_failures.lock().await.clear();

    // desactivar exige contrasena y codigo
    assert_eq!(s.post("/api/auth/2fa/disable", &ana, json!({ "password": "ana-password", "code": "999999" })).await.status, 401);
    s.state.login_failures.lock().await.clear();
    assert_eq!(s.post("/api/auth/2fa/disable", &ana, json!({ "password": "ana-password", "code": fresh[0] })).await.status, 204);
    assert!(login(&s, "ana", "ana-password", None).await.json()["token"].is_string());
    assert_eq!(dav_status(&s, "ana", &app).await, 401);
    assert_eq!(dav_status(&s, "ana", "ana-password").await, 207);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admin_reinicia_doble_factor() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "operador").await;
    let secret = s.post("/api/auth/2fa/setup", &ana, json!({ "password": "ana-password" })).await.json()["secret"]
        .as_str()
        .unwrap()
        .to_string();
    s.post("/api/auth/2fa/enable", &ana, json!({ "code": totp::code_at(&secret, totp::current_step()).unwrap() })).await;

    let users = s.get("/api/auth/users", &admin).await.json();
    let row = users.as_array().unwrap().iter().find(|u| u["username"] == "ana").unwrap().clone();
    assert_eq!(row["totp_enabled"], true);

    // solo admin
    assert_eq!(s.delete("/api/auth/users/ana/2fa", &ana).await.status, 403);
    assert_eq!(s.delete("/api/auth/users/nadie/2fa", &admin).await.status, 404);
    assert_eq!(s.delete("/api/auth/users/ana/2fa", &admin).await.status, 204);
    assert!(login(&s, "ana", "ana-password", None).await.json()["token"].is_string());
    let conn = s.state.db.get().unwrap();
    let left: i64 = conn.query_row("SELECT COUNT(*) FROM totp_recovery", [], |r| r.get(0)).unwrap();
    assert_eq!(left, 0);
}
