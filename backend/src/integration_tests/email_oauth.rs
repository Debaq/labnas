//! Cuenta Microsoft por OAuth2 (codigo de dispositivo) contra un servidor de login falso

use axum::{extract::Path, routing::post, Form, Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::harness::{wait_until, Server};
use crate::handlers::email as mail;
use crate::models::email::{EmailAccount, MailAuth, MailProtocol};

const CLIENT_ID: &str = "11111111-2222-3333-4444-555555555555";

async fn fake_microsoft() -> String {
    let polls = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route(
            "/{tenant}/oauth2/v2.0/devicecode",
            post(|Path(_t): Path<String>, Form(f): Form<HashMap<String, String>>| async move {
                assert_eq!(f["client_id"], CLIENT_ID);
                assert!(f["scope"].contains("IMAP.AccessAsUser.All") && f["scope"].contains("offline_access"));
                Json(json!({ "device_code": "dc1", "user_code": "ABCD-1234",
                    "verification_uri": "https://microsoft.com/devicelogin", "expires_in": 900, "interval": 1 }))
            }),
        )
        .route(
            "/{tenant}/oauth2/v2.0/token",
            post(move |Path(_t): Path<String>, Form(f): Form<HashMap<String, String>>| {
                let polls = polls.clone();
                async move {
                    let v: Value = match f["grant_type"].as_str() {
                        "urn:ietf:params:oauth:grant-type:device_code" => {
                            // el usuario tarda un poco en ingresar el codigo
                            if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                                json!({ "error": "authorization_pending" })
                            } else {
                                json!({ "access_token": "at1", "refresh_token": "rt1", "expires_in": 3600 })
                            }
                        }
                        "refresh_token" if f["refresh_token"] == "rt1" => {
                            json!({ "access_token": "at2", "refresh_token": "rt2", "expires_in": 3600 })
                        }
                        _ => json!({ "error": "invalid_grant", "error_description": "AADSTS70000: token revocado" }),
                    };
                    Json(v)
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    format!("http://{}", addr)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cuenta_microsoft_por_codigo_de_dispositivo() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "observador").await;

    *mail::LOGIN_BASE_OVERRIDE.write().unwrap() = Some(fake_microsoft().await);
    // IMAP inalcanzable: la primera revision falla rapido y queda registrada
    *mail::IMAP_OVERRIDE.write().unwrap() = Some(("127.0.0.1".to_string(), 1));

    // Sin Client ID no se puede empezar
    let r = s.post("/api/email/oauth/microsoft/start", &ana, json!({ "email": "ana@uach.cl" })).await;
    assert_eq!(r.status, 400);
    assert_eq!(s.put("/api/email/oauth/microsoft/config", &ana, json!({ "client_id": CLIENT_ID })).await.status, 403);
    assert_eq!(s.put("/api/email/oauth/microsoft/config", &admin, json!({ "client_id": "x" })).await.status, 400);
    let r = s.put("/api/email/oauth/microsoft/config", &admin, json!({ "client_id": CLIENT_ID, "tenant": "organizations" })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(s.get("/api/email/oauth/microsoft/config", &ana).await.json()["configured"], true);

    assert_eq!(s.get("/api/email/account", &ana).await.status, 404);
    let r = s.post("/api/email/oauth/microsoft/start", &ana, json!({ "email": "Ana@uach.cl" })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["user_code"], "ABCD-1234");
    assert_eq!(r.json()["status"], "pendiente");

    wait_until(10_000, || async { s.get("/api/email/oauth/microsoft/status", &ana).await.json()["status"] == "conectado" }).await;

    let acc = s.get("/api/email/account", &ana).await.json();
    assert_eq!(acc["auth"], "microsoft");
    assert_eq!(acc["email"], "ana@uach.cl");
    assert!(acc.get("password").is_none());
    let stored: String = s.state.db.get().unwrap()
        .query_row("SELECT labnas_decrypt(password) FROM email_accounts WHERE username = 'ana'", [], |r| r.get(0)).unwrap();
    assert_eq!(stored, "rt1", "refresh token cifrado en la base");

    // la primera revision corre sola y deja el error a la vista
    wait_until(10_000, || async { s.get("/api/email/account", &ana).await.json()["last_error"].is_string() }).await;

    // Renovar el access token rota el refresh token
    mail::forget_tokens("ana");
    let account = EmailAccount {
        username: "ana".into(), host: "x".into(), port: 993, protocol: MailProtocol::Imap,
        email: "ana@uach.cl".into(), password: "rt1".into(), filters: vec![], auth: MailAuth::Microsoft,
    };
    assert_eq!(mail::access_token(&s.state, &account).await.unwrap(), "at2");
    let stored: String = s.state.db.get().unwrap()
        .query_row("SELECT labnas_decrypt(password) FROM email_accounts WHERE username = 'ana'", [], |r| r.get(0)).unwrap();
    assert_eq!(stored, "rt2");

    // Revocado: mensaje claro para reconectar
    mail::forget_tokens("ana");
    let revoked = EmailAccount { password: "viejo".into(), ..account };
    assert!(mail::access_token(&s.state, &revoked).await.unwrap_err().contains("vuelve a conectar"));

    *mail::LOGIN_BASE_OVERRIDE.write().unwrap() = None;
    *mail::IMAP_OVERRIDE.write().unwrap() = None;
}
