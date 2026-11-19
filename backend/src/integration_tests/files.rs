use serde_json::json;

use super::harness::Server;

fn enc(p: &str) -> String {
    urlencoding::encode(p).to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn papelera_mover_restaurar_y_vaciar() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let bob = s.user(&admin, "bob", "observador").await;
    std::fs::create_dir_all(s.home.join("Docs/sub")).unwrap();
    std::fs::write(s.home.join("Docs/a.txt"), "uno").unwrap();
    std::fs::write(s.home.join("Docs/sub/b.txt"), "dos").unwrap();
    let trash_dir = s.home.join(".labnas-trash");

    // borrar = mover a la papelera
    assert_eq!(s.delete(&format!("/api/files?path={}", enc(&s.path("Docs/a.txt"))), &admin).await.status, 204);
    assert!(!s.home.join("Docs/a.txt").exists());
    assert_eq!(std::fs::read_dir(&trash_dir).unwrap().count(), 1);
    assert_eq!(s.delete(&format!("/api/files?path={}", enc(&s.path("Docs/sub"))), &admin).await.status, 204);

    let items = s.get("/api/trash", &admin).await.json();
    assert_eq!(items.as_array().unwrap().len(), 2);
    let dir = items.as_array().unwrap().iter().find(|i| i["is_dir"] == true).unwrap();
    assert_eq!(dir["size"], 3, "tamaño de la carpeta");

    let visible = s.get("/api/trash", &bob).await;
    assert_eq!(visible.status, 200);
    assert_eq!(visible.json().as_array().unwrap().len(), 0, "sin escritura no ve lo borrado");
    let inside = std::fs::read_dir(&trash_dir).unwrap().next().unwrap().unwrap().path();
    assert_eq!(
        s.delete(&format!("/api/files?path={}", enc(&inside.to_string_lossy())), &admin).await.status,
        400,
        "no se borra desde dentro de la papelera"
    );
    let listing = s.get(&format!("/api/files?path={}", enc(&s.path(""))), &admin).await.json();
    assert!(listing.as_array().unwrap().iter().all(|e| !e["name"].as_str().unwrap().contains("trash")));

    // restaurar con sufijo si ya existe algo con ese nombre
    let file_id = items.as_array().unwrap().iter().find(|i| i["name"] == "a.txt").unwrap()["id"].as_str().unwrap().to_string();
    std::fs::write(s.home.join("Docs/a.txt"), "otro").unwrap();
    let restored = s.post(&format!("/api/trash/{}/restore", file_id), &admin, json!({})).await.json();
    let restored = restored.as_str().unwrap().to_string();
    assert_eq!(restored, s.path("Docs/a (restaurado 1).txt"));
    assert_eq!(std::fs::read_to_string(&restored).unwrap(), "uno");
    assert_eq!(std::fs::read_to_string(s.home.join("Docs/a.txt")).unwrap(), "otro");

    let dir_id = dir["id"].as_str().unwrap();
    s.post(&format!("/api/trash/{}/restore", dir_id), &admin, json!({})).await;
    assert_eq!(std::fs::read_to_string(s.home.join("Docs/sub/b.txt")).unwrap(), "dos");

    // borrado definitivo y vaciar (solo admin)
    s.delete(&format!("/api/files?path={}", enc(&s.path("Docs/sub"))), &admin).await;
    s.delete(&format!("/api/files?path={}", enc(&s.path("Docs/a.txt"))), &admin).await;
    let first = s.get("/api/trash", &admin).await.json()[0]["id"].as_str().unwrap().to_string();
    assert_eq!(s.delete(&format!("/api/trash/{}", first), &admin).await.status, 204);
    assert_eq!(s.get("/api/trash", &admin).await.json().as_array().unwrap().len(), 1);
    assert_eq!(s.delete("/api/trash", &bob).await.status, 403);
    assert_eq!(s.delete("/api/trash", &admin).await.json(), 1);
    assert_eq!(std::fs::read_dir(&trash_dir).unwrap().count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn vista_previa_con_link_temporal() {
    let s = Server::start().await;
    let admin = s.admin().await;
    std::fs::create_dir_all(s.home.join("Docs")).unwrap();
    std::fs::write(s.home.join("Docs/foto.png"), b"\x89PNG\r\n\x1a\n0000").unwrap();
    std::fs::write(s.home.join("Docs/malo.html"), "<script>alert(1)</script>").unwrap();
    std::fs::write(s.home.join("Docs/dibujo.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
    let big: String = (1..100_000).map(|n| format!("{}\n", n)).collect();
    std::fs::write(s.home.join("Docs/grande.txt"), &big).unwrap();
    std::fs::write(s.root.join("fuera.txt"), "secreto").unwrap();

    async fn token(s: &Server, admin: &str, path: String) -> super::harness::Resp {
        s.post("/api/files/preview-token", admin, json!({ "path": path })).await
    }
    async fn get(s: &Server, url: String) -> super::harness::Resp {
        s.raw(s.client().get(format!("{}{}", s.base, url))).await
    }

    let no_auth = s.req(reqwest::Method::POST, "/api/files/preview-token", None, Some(json!({ "path": s.path("Docs/foto.png") }))).await;
    assert_eq!(no_auth.status, 401);
    assert_eq!(token(&s, &admin, s.root.join("fuera.txt").to_string_lossy().to_string()).await.status, 403);

    let links = token(&s, &admin, s.path("Docs/foto.png")).await.json();
    let url = links["url"].as_str().unwrap().to_string();
    let r = get(&s, url.clone()).await;
    assert_eq!(r.header("content-type"), "image/png");
    assert!(r.header("content-disposition").starts_with("inline"));
    assert_eq!(r.header("x-content-type-options"), "nosniff");
    let r = get(&s, links["download_url"].as_str().unwrap().to_string()).await;
    assert!(r.header("content-disposition").starts_with("attachment"));

    let html = token(&s, &admin, s.path("Docs/malo.html")).await.json()["url"].as_str().unwrap().to_string();
    assert!(get(&s, html).await.header("content-type").starts_with("text/plain"), "HTML no se ejecuta");
    let svg = token(&s, &admin, s.path("Docs/dibujo.svg")).await.json()["url"].as_str().unwrap().to_string();
    assert!(get(&s, svg).await.header("content-security-policy").contains("script-src 'none'"));

    let txt = token(&s, &admin, s.path("Docs/grande.txt")).await.json()["url"].as_str().unwrap().to_string();
    let r = s.raw(s.client().get(format!("{}{}", s.base, txt)).header("Range", "bytes=0-99")).await;
    assert_eq!(r.status, 206);
    assert_eq!(r.text.len(), 100);
    assert_eq!(get(&s, txt).await.text.len(), big.len());

    assert_eq!(get(&s, "/api/preview/inventado/x.png".to_string()).await.status, 410);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subida_por_partes() {
    let s = Server::start().await;
    let admin = s.admin().await;
    std::fs::create_dir_all(s.home.join("Docs")).unwrap();

    let init = s.post("/api/files/upload/chunked", &admin, json!({ "path": s.path("Docs"), "name": "../tesis.pdf", "size": 10 })).await.json();
    let id = init["id"].as_str().unwrap().to_string();
    let put = |offset: usize, data: &'static [u8]| {
        s.client().put(format!("{}/api/files/upload/chunked/{}?offset={}", s.base, id, offset)).bearer_auth(&admin).body(data)
    };
    assert_eq!(put(0, b"hola ").send().await.unwrap().status(), 200);
    // Fuera de orden: 409 con el offset correcto para reanudar
    let r = put(9, b"x").send().await.unwrap();
    assert_eq!(r.status(), 409);
    assert_eq!(r.text().await.unwrap(), "5");
    assert_eq!(s.post(&format!("/api/files/upload/chunked/{}/complete", id), &admin, json!({})).await.status, 400, "incompleta");
    assert_eq!(put(5, b"mundo").send().await.unwrap().status(), 200);
    assert_eq!(put(10, b"!").send().await.unwrap().status(), 400, "supera el tamano anunciado");
    assert_eq!(s.post(&format!("/api/files/upload/chunked/{}/complete", id), &admin, json!({})).await.status, 201);
    // El nombre se sanea y no quedan temporales
    assert_eq!(std::fs::read_to_string(s.home.join("Docs/tesis.pdf")).unwrap(), "hola mundo");
    assert!(std::fs::read_dir(s.home.join("Docs")).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part")));

    // Otro usuario no puede escribir en una subida ajena; sobre el limite se rechaza
    let bob = s.user(&admin, "bob", "operador").await;
    let init = s.post("/api/files/upload/chunked", &admin, json!({ "path": s.path("Docs"), "name": "a", "size": 3 })).await.json();
    let r = s.client().put(format!("{}/api/files/upload/chunked/{}?offset=0", s.base, init["id"].as_str().unwrap())).bearer_auth(&bob).body("abc").send().await.unwrap();
    assert_eq!(r.status(), 404);
    let r = s.post("/api/files/upload/chunked", &admin, json!({ "path": s.path("Docs"), "name": "b", "size": 10_u64 << 40 })).await;
    assert_eq!(r.status, 413);
}
