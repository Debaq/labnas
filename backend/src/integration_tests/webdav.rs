use reqwest::Method;
use serde_json::json;

use super::harness::{Resp, Server};

fn m(name: &str) -> Method {
    Method::from_bytes(name.as_bytes()).unwrap()
}

async fn dav(s: &Server, method: &str, path: &str, user: Option<(&str, &str)>, body: Option<&str>, headers: &[(&str, &str)]) -> Resp {
    let mut r = s.client().request(m(method), format!("{}{}", s.base, path));
    if let Some((u, p)) = user {
        r = r.basic_auth(u, Some(p));
    }
    for (k, v) in headers {
        r = r.header(*k, *v);
    }
    if let Some(b) = body {
        r = r.body(b.to_string());
    }
    s.raw(r).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn webdav_con_permisos_por_carpeta() {
    let s = Server::start().await;
    let admin = s.admin().await;
    s.user(&admin, "obs", "observador").await;
    s.user(&admin, "bob", "observador").await;
    let obs = Some(("obs", "obs-password"));
    let bob = Some(("bob", "bob-password"));

    let docs = s.root.join("docs");
    let privada = s.root.join("privada");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::create_dir_all(&privada).unwrap();
    std::fs::write(docs.join("hola.txt"), "hola dav").unwrap();
    s.put("/api/files/roots", &admin, json!({ "roots": [
        { "path": docs.to_string_lossy(), "readers": ["*"], "writers": ["user:bob"] },
        { "path": privada.to_string_lossy(), "readers": ["user:bob"], "writers": ["user:bob"] },
        { "path": s.home.to_string_lossy() }
    ] })).await;

    // desactivado por defecto
    assert_eq!(dav(&s, "PROPFIND", "/dav/", obs, None, &[]).await.status, 404);
    assert_eq!(s.put("/api/files/webdav", &admin, json!({ "enabled": true })).await.status, 200);

    // autenticacion
    assert_eq!(dav(&s, "PROPFIND", "/dav/", None, None, &[]).await.status, 401);
    assert_eq!(dav(&s, "PROPFIND", "/dav/", Some(("obs", "mala")), None, &[]).await.status, 401);

    // /dav/ lista solo las carpetas legibles
    let top = dav(&s, "PROPFIND", "/dav/", obs, None, &[("Depth", "1")]).await;
    assert_eq!(top.status, 207);
    assert!(top.text.contains("/dav/docs/"));
    assert!(!top.text.contains("/dav/privada/"), "obs no ve la carpeta privada");
    assert!(dav(&s, "PROPFIND", "/dav/", bob, None, &[("Depth", "1")]).await.text.contains("/dav/privada/"));

    // lectura
    assert_eq!(dav(&s, "GET", "/dav/docs/hola.txt", obs, None, &[]).await.text, "hola dav");
    assert_eq!(dav(&s, "GET", "/dav/privada/", obs, None, &[]).await.status, 404, "no montada para obs");
    let listing = dav(&s, "PROPFIND", "/dav/docs/", obs, None, &[("Depth", "1")]).await;
    assert_eq!(listing.status, 207);
    assert!(listing.text.contains("hola.txt"));

    // escritura segun la carpeta
    assert_eq!(dav(&s, "PUT", "/dav/docs/nuevo.txt", obs, Some("x"), &[]).await.status, 403);
    let put = dav(&s, "PUT", "/dav/docs/nuevo.txt", bob, Some("de bob"), &[]).await.status;
    assert!(put == 201 || put == 204, "PUT {}", put);
    assert_eq!(std::fs::read_to_string(docs.join("nuevo.txt")).unwrap(), "de bob");
    assert_eq!(dav(&s, "MKCOL", "/dav/docs/sub", bob, None, &[]).await.status, 201);
    assert!(docs.join("sub").is_dir());
    let dest = format!("{}/dav/docs/sub/movido.txt", s.base);
    let mv = dav(&s, "MOVE", "/dav/docs/nuevo.txt", bob, None, &[("Destination", &dest)]).await.status;
    assert!(mv == 201 || mv == 204, "MOVE {}", mv);
    assert!(docs.join("sub/movido.txt").exists());
    let fuera = format!("{}/dav/privada/x.txt", s.base);
    assert_eq!(dav(&s, "MOVE", "/dav/docs/sub/movido.txt", bob, None, &[("Destination", &fuera)]).await.status, 502);

    // DELETE va a la papelera
    assert_eq!(dav(&s, "DELETE", "/dav/docs/sub/movido.txt", bob, None, &[]).await.status, 204);
    assert!(!docs.join("sub/movido.txt").exists());
    let trash = s.get("/api/trash", &admin).await.json();
    assert!(trash.as_array().unwrap().iter().any(|i| i["name"] == "movido.txt"));
    assert_eq!(dav(&s, "DELETE", "/dav/docs/", bob, None, &[]).await.status, 403, "no se borra una raiz");

    // no escapa de la raiz ni expone ~/.labnas
    let home_name = s.home.file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(dav(&s, "GET", &format!("/dav/{}/.labnas/labnas.db", home_name), Some(("admin", "adminadmin")), None, &[]).await.status, 403);
    // (el cliente puede normalizar la URL y la request cae fuera de /dav: 404; nunca se sirve)
    let escape = dav(&s, "GET", "/dav/docs/%2e%2e/%2e%2e/etc/passwd", obs, None, &[]).await;
    assert!(matches!(escape.status, 403 | 404), "{}", escape.status);
    assert!(!escape.text.contains("root:"));

    // pendiente no entra
    s.register("pepe", "pepepepe").await;
    assert_eq!(dav(&s, "PROPFIND", "/dav/", Some(("pepe", "pepepepe")), None, &[]).await.status, 403);
}
