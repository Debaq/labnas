//! Passkeys (WebAuthn) con un autenticador de software ES256

use serde_json::{json, Value};

use super::harness::Server;
use crate::handlers::passkeys::testing::SoftKey;

const ORIGIN: &str = "https://labnas.ejemplo.cl";
const RP: &str = "labnas.ejemplo.cl";

async fn post(s: &Server, path: &str, origin: &str, token: Option<&str>, body: Value) -> (u16, Value, reqwest::header::HeaderMap) {
    let mut r = s.client().post(format!("{}{}", s.base, path)).header("origin", origin).json(&body);
    if let Some(t) = token {
        r = r.bearer_auth(t);
    }
    let resp = r.send().await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let text = resp.text().await.unwrap_or_default();
    (status, serde_json::from_str(&text).unwrap_or(Value::String(text)), headers)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registrar_e_iniciar_sesion_con_passkey() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let mut key = SoftKey::new();

    // Requisitos del navegador: dominio y HTTPS
    assert_eq!(post(&s, "/api/auth/passkey/register/options", "http://192.168.1.5:3001", Some(&admin), json!({})).await.0, 400);
    assert_eq!(post(&s, "/api/auth/passkey/register/options", "http://labnas.local", Some(&admin), json!({})).await.0, 400);

    // Registro
    let (st, opts, _) = post(&s, "/api/auth/passkey/register/options", ORIGIN, Some(&admin), json!({})).await;
    assert_eq!(st, 200, "{}", opts);
    assert_eq!(opts["rp"]["id"], RP);
    let challenge = opts["challenge"].as_str().unwrap();
    let (st, r, _) = post(&s, "/api/auth/passkey/register", ORIGIN, Some(&admin), key.create(RP, challenge, ORIGIN)).await;
    assert_eq!(st, 200, "{}", r);
    // El desafio era de un solo uso
    let (st, _, _) = post(&s, "/api/auth/passkey/register", ORIGIN, Some(&admin), key.create(RP, challenge, ORIGIN)).await;
    assert_eq!(st, 401);

    // Inicio de sesion sin contraseña
    let (_, opts, _) = post(&s, "/api/auth/passkey/login/options", ORIGIN, None, json!({ "username": "admin" })).await;
    assert_eq!(opts["allowCredentials"].as_array().unwrap().len(), 1);
    let challenge = opts["challenge"].as_str().unwrap().to_string();
    let assertion = key.get(RP, &challenge, ORIGIN);
    let (st, r, headers) = post(&s, "/api/auth/passkey/login", ORIGIN, None, assertion.clone()).await;
    assert_eq!(st, 200, "{}", r);
    assert!(headers.get("set-cookie").unwrap().to_str().unwrap().contains("HttpOnly"));
    let token = r["token"].as_str().unwrap().to_string();
    assert_eq!(s.get("/api/auth/me", &token).await.json()["username"], "admin");

    // Reusar la misma asercion: rechazado
    assert_eq!(post(&s, "/api/auth/passkey/login", ORIGIN, None, assertion).await.0, 401);

    // Desde otro sitio (phishing): el origen no coincide
    let (_, opts, _) = post(&s, "/api/auth/passkey/login/options", ORIGIN, None, json!({})).await;
    let evil = key.get(RP, opts["challenge"].as_str().unwrap(), "https://labnas-ejemplo.evil.com");
    assert_eq!(post(&s, "/api/auth/passkey/login", ORIGIN, None, evil).await.0, 401);

    // Firma alterada
    let (_, opts, _) = post(&s, "/api/auth/passkey/login/options", ORIGIN, None, json!({})).await;
    let mut forged = key.get(RP, opts["challenge"].as_str().unwrap(), ORIGIN);
    forged["authenticatorData"] = json!(forged["authenticatorData"].as_str().unwrap().replace('A', "B"));
    assert_eq!(post(&s, "/api/auth/passkey/login", ORIGIN, None, forged).await.0, 401);

    // Contador que retrocede (llave clonada)
    key.count = 0;
    let (_, opts, _) = post(&s, "/api/auth/passkey/login/options", ORIGIN, None, json!({})).await;
    let cloned = key.get(RP, opts["challenge"].as_str().unwrap(), ORIGIN);
    assert_eq!(post(&s, "/api/auth/passkey/login", ORIGIN, None, cloned).await.0, 401);

    // Listar y borrar
    let list = s.get("/api/auth/passkeys", &admin).await.json();
    assert_eq!(list[0]["rp_id"], RP);
    let id = list[0]["id"].as_str().unwrap().to_string();
    assert_eq!(s.delete(&format!("/api/auth/passkeys/{}", id), &admin).await.status, 204);
    key.count = 100;
    let (_, opts, _) = post(&s, "/api/auth/passkey/login/options", ORIGIN, None, json!({})).await;
    let gone = key.get(RP, opts["challenge"].as_str().unwrap(), ORIGIN);
    assert_eq!(post(&s, "/api/auth/passkey/login", ORIGIN, None, gone).await.0, 401);
}
