use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configuracion_de_timelapse() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let obs = s.user(&admin, "obs", "observador").await;

    // por defecto: ninguna impresora, 30 s, carpeta Timelapses en la primera raiz
    let cfg = s.get("/api/printers3d/timelapse", &obs).await.json();
    assert_eq!(cfg["printers"], json!([]));
    assert_eq!(cfg["interval_secs"], 30);
    assert!(cfg["effective_dir"].as_str().unwrap().ends_with("/Timelapses"));

    assert_eq!(s.put("/api/printers3d/timelapse", &obs, json!({ "printers": ["x"] })).await.status, 403);
    assert_eq!(s.put("/api/printers3d/timelapse", &admin, json!({ "printers": [], "interval_secs": 2 })).await.status, 400);
    assert_eq!(s.put("/api/printers3d/timelapse", &admin, json!({ "printers": [], "dir": "/etc" })).await.status, 400, "fuera de las raices");

    let dir = s.path("Videos3D");
    let r = s.put("/api/printers3d/timelapse", &admin, json!({ "printers": ["p1"], "interval_secs": 60, "dir": dir })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["effective_dir"], dir.as_str());
    assert_eq!(r.json()["printers"], json!(["p1"]));
}
