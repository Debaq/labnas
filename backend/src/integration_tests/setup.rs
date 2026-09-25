use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn asistente_inicial_solo_admin_y_se_termina() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;

    let r = s.get("/api/setup", &admin).await.json();
    assert_eq!(r["done"], false);
    assert!(r["secret_key_path"].as_str().unwrap().ends_with("secret.key"));

    // deny-by-default: solo admin
    assert_eq!(s.get("/api/setup", &op).await.status, 403);
    assert_eq!(s.post("/api/setup/done", &op, serde_json::json!({})).await.status, 403);

    assert_eq!(s.post("/api/setup/done", &admin, serde_json::json!({})).await.status, 204);
    assert_eq!(s.get("/api/setup", &admin).await.json()["done"], true);
}
