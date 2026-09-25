use serde_json::json;

use super::harness::Server;

fn enc(p: &str) -> String {
    urlencoding::encode(p).to_string()
}

async fn raiz(s: &Server, token: &str) -> Vec<String> {
    let listing = s.get("/api/files?path=/", token).await.json();
    listing.as_array().unwrap().iter().map(|e| e["path"].as_str().unwrap().to_string()).collect()
}

async fn upload(s: &Server, token: &str, dir: &str, name: &str) -> u16 {
    let form = reqwest::multipart::Form::new()
        .text("path", dir.to_string())
        .part("file", reqwest::multipart::Part::bytes(b"x".to_vec()).file_name(name.to_string()));
    s.raw(s.client().post(format!("{}/api/files/upload", s.base)).bearer_auth(token).multipart(form)).await.status
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn permisos_por_carpeta() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let obs = s.user(&admin, "obs", "observador").await;
    let op = s.user(&admin, "op", "operador").await;
    let bob = s.user(&admin, "bob", "observador").await;

    let publica = s.root.join("publica");
    let privada = s.root.join("privada");
    std::fs::create_dir_all(&publica).unwrap();
    std::fs::create_dir_all(&privada).unwrap();
    std::fs::write(privada.join("secreto.txt"), "s").unwrap();
    let (publica, privada) = (publica.to_string_lossy().to_string(), privada.to_string_lossy().to_string());

    // permiso invalido
    let r = s.put("/api/files/roots", &admin, json!({ "roots": [{ "path": publica, "readers": ["grupo:x"], "writers": [] }] })).await;
    assert_eq!(r.status, 400);

    let r = s.put("/api/files/roots", &admin, json!({ "roots": [
        { "path": publica },
        { "path": privada, "readers": ["role:operador"], "writers": ["user:bob"] }
    ] })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let cfg = r.json();
    assert_eq!(cfg["roots"][0]["readers"], json!(["*"]), "por defecto: todos leen");
    assert_eq!(cfg["roots"][1]["writers"], json!(["user:bob"]));

    // "/" muestra solo lo que cada uno puede leer
    assert_eq!(raiz(&s, &obs).await, vec![publica.clone()]);
    assert_eq!(raiz(&s, &op).await.len(), 2);

    // lectura restringida
    let secreto = format!("{}/secreto.txt", privada);
    assert_eq!(s.get(&format!("/api/files?path={}", enc(&privada)), &obs).await.status, 403);
    assert_eq!(s.get(&format!("/api/files/download?path={}", enc(&secreto)), &obs).await.status, 403);
    assert_eq!(s.post("/api/files/preview-token", &obs, json!({ "path": secreto })).await.status, 403);
    assert_eq!(s.post("/api/shares", &obs, json!({ "path": secreto })).await.status, 403);
    assert_eq!(s.get(&format!("/api/files/download?path={}", enc(&secreto)), &op).await.status, 200);

    // escritura: bob (observador sin permiso global) escribe solo donde lo autorizan
    assert_eq!(upload(&s, &bob, &privada, "de-bob.txt").await, 201);
    assert_eq!(upload(&s, &bob, &publica, "no.txt").await, 403);
    assert_eq!(upload(&s, &op, &privada, "no.txt").await, 403, "el operador solo lee aqui");
    assert_eq!(s.get(&format!("/api/files?path={}", enc(&privada)), &bob).await.status, 200, "escribir implica leer");

    // papelera: bob ve y restaura lo de su carpeta; obs no
    assert_eq!(s.delete(&format!("/api/files?path={}", enc(&format!("{}/de-bob.txt", privada))), &bob).await.status, 204);
    let items = s.get("/api/trash", &bob).await.json();
    assert_eq!(items.as_array().unwrap().len(), 1);
    assert_eq!(s.get("/api/trash", &obs).await.json().as_array().unwrap().len(), 0);
    let id = items[0]["id"].as_str().unwrap();
    assert_eq!(s.post(&format!("/api/trash/{}/restore", id), &obs, json!({})).await.status, 403);
    assert_eq!(s.post(&format!("/api/trash/{}/restore", id), &bob, json!({})).await.status, 200);

    // los no admin ven las rutas visibles, sin la configuracion de permisos
    let roots_obs = s.get("/api/files/roots", &obs).await.json();
    assert_eq!(roots_obs["roots"].as_array().unwrap().len(), 1);
    assert_eq!(roots_obs["roots"][0]["readers"], json!([]));
}
