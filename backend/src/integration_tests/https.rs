use axum::body::Body;
use axum::http::Request;
use serde_json::json;
use tower::ServiceExt;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn https_con_certificado_autofirmado() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let s = Server::start().await;
    let admin = s.admin().await;

    // estado: activo, autofirmado, con localhost en los nombres
    let st = s.get("/api/system/tls", &admin).await.json();
    assert_eq!(st["enabled"], true);
    assert_eq!(st["source"], "autofirmado");
    assert!(st["cert"]["names"].as_array().unwrap().iter().any(|n| n == "localhost"));
    let fingerprint = st["cert"]["fingerprint_sha256"].as_str().unwrap().to_string();

    // el certificado se descarga sin sesion (para instalarlo en los equipos)
    let pem = s.raw(s.client().get(format!("{}/api/tls/cert.pem", s.base))).await;
    assert_eq!(pem.status, 200);
    assert!(pem.text.starts_with("-----BEGIN CERTIFICATE-----"));

    // servir el router real por TLS y consultarlo confiando solo en ese certificado
    let (cert, key) = {
        let conn = s.state.db.get().unwrap();
        let (c, k, _) = crate::tls::active_paths(&conn).unwrap();
        (c, k)
    };
    let config = crate::tls::rustls_config(&cert, &key).await.unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = crate::build_api(&s.state, 50);
    tokio::spawn(async move {
        axum_server::tls_rustls::from_tcp_rustls(listener, config).unwrap().serve(app.into_make_service()).await.ok();
    });
    let client = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_pem(pem.text.as_bytes()).unwrap())
        .build()
        .unwrap();
    let r = client.get(format!("https://localhost:{}/api/health", port)).send().await.unwrap();
    assert_eq!(r.status(), 200);
    // sin confiar en el certificado, el cliente lo rechaza
    assert!(reqwest::get(format!("https://localhost:{}/api/health", port)).await.is_err());

    // validaciones
    assert_eq!(s.put("/api/system/tls", &admin, json!({ "enabled": true, "cert_path": "/tmp/x.pem" })).await.status, 400);
    assert_eq!(s.put("/api/system/tls", &admin, json!({ "enabled": true, "cert_path": "/no/existe.pem", "key_path": "/no/existe.key" })).await.status, 400);

    // certificado propio (una copia del autofirmado) y vuelta a autofirmado
    let r = s.put("/api/system/tls", &admin, json!({ "enabled": true, "redirect": true, "cert_path": cert, "key_path": key })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["source"], "propio");
    assert_eq!(r.json()["redirect"], true);
    let r = s.put("/api/system/tls", &admin, json!({ "enabled": true })).await;
    assert_eq!(r.json()["source"], "autofirmado");

    // regenerar cambia la huella
    let r = s.post("/api/system/tls/regenerate", &admin, json!({})).await.json();
    assert_ne!(r["cert"]["fingerprint_sha256"], fingerprint.as_str());
}

#[tokio::test]
async fn redireccion_http_a_https() {
    let app = axum::Router::new()
        .route("/{*p}", axum::routing::any(|| async { "http" }))
        .layer(axum::middleware::from_fn(crate::tls::redirect_middleware))
        .layer(axum::Extension(crate::tls::Redirect { https_port: 3443 }));

    let req = |path: &str, host: &str| Request::builder().uri(path).header("host", host).body(Body::empty()).unwrap();

    let r = app.clone().oneshot(req("/api/files?path=/x", "192.168.1.5:3001")).await.unwrap();
    assert_eq!(r.status(), 308);
    assert_eq!(r.headers()["location"], "https://192.168.1.5:3443/api/files?path=/x");

    // excepciones: sensores, health y localhost (el visor)
    for (path, host) in [("/api/sensors/data", "192.168.1.5:3001"), ("/api/health", "10.0.0.2"), ("/api/files", "localhost:3001")] {
        let r = app.clone().oneshot(req(path, host)).await.unwrap();
        assert_eq!(r.status(), 200, "{} {}", path, host);
    }

    // el visor de escritorio no se redirige
    let viewer = Request::builder().uri("/api/files").header("host", "192.168.1.5:3001").header("user-agent", "Mozilla/5.0 LabNAS-Viewer/2.9").body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(viewer).await.unwrap().status(), 200);

    let https443 = axum::Router::new()
        .route("/{*p}", axum::routing::any(|| async { "http" }))
        .layer(axum::middleware::from_fn(crate::tls::redirect_middleware))
        .layer(axum::Extension(crate::tls::Redirect { https_port: 443 }));
    let r = https443.oneshot(req("/dav/", "labnas.local")).await.unwrap();
    assert_eq!(r.headers()["location"], "https://labnas.local/dav/");
}
