use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wake_on_lan() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let obs = s.user(&admin, "obs", "observador").await;
    let op = s.user(&admin, "op", "operador").await;

    assert_eq!(s.post("/api/network/wake/AA:BB:CC:DD:EE:FF", &obs, json!({})).await.status, 403);
    assert_eq!(s.post("/api/network/wake/no-es-mac", &op, json!({})).await.status, 400);
    assert_eq!(s.post("/api/network/wake/AA:BB:CC:DD:EE:FF", &op, json!({})).await.status, 200);
    assert_eq!(s.get("/api/audit?q=Wake", &admin).await.json()[0]["details"], "AA:BB:CC:DD:EE:FF");
}
