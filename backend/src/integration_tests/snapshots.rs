//! Snapshots programados con un wrapper falso (simula btrfs con carpetas)

use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshots_con_retencion() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let vol = s.root.join("volumen");
    std::fs::create_dir_all(&vol).unwrap();

    // Sin el wrapper configurado: no listo, con instrucciones
    *crate::handlers::snapshots::TEST_COMMAND.write().unwrap() = Some("/nonexistent/labnas-snapshot".into());
    let r = s.get("/api/snapshots", &admin).await.json();
    assert_eq!(r["ready"], false);

    let fake = s.root.join("fake-snapshot.sh");
    std::fs::write(&fake, format!(r#"#!/bin/sh
set -e
VOL="{vol}"
case "$1" in
  paths) echo "$VOL" ;;
  create) n=$(ls "$VOL/.snapshots" 2>/dev/null | wc -l); mkdir -p "$VOL/.snapshots"; name="labnas-20261118-00000$n"; mkdir "$VOL/.snapshots/$name"; echo "$name" ;;
  list) ls "$VOL/.snapshots" 2>/dev/null || true ;;
  delete) rmdir "$VOL/.snapshots/$3" ;;
esac
"#, vol = vol.display())).unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    *crate::handlers::snapshots::TEST_COMMAND.write().unwrap() = Some(fake.to_string_lossy().to_string());

    assert_eq!(s.get("/api/snapshots", &op).await.status, 403, "solo admin");
    assert_eq!(s.put("/api/snapshots/config", &admin, json!({ "interval_hours": 6, "keep": 0 })).await.status, 400);
    assert_eq!(s.put("/api/snapshots/config", &admin, json!({ "interval_hours": 6, "keep": 2 })).await.status, 200);

    for _ in 0..3 {
        let r = s.post("/api/snapshots/now", &admin, json!({})).await;
        assert_eq!(r.status, 200, "{}", r.text);
    }
    let r = s.get("/api/snapshots", &admin).await.json();
    assert_eq!(r["ready"], true);
    assert_eq!(r["config"]["interval_hours"], 6);
    let snaps: Vec<&str> = r["paths"][0]["snapshots"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(snaps, vec!["labnas-20261118-000002", "labnas-20261118-000001"], "se conservan los 2 mas nuevos");

    *crate::handlers::snapshots::TEST_COMMAND.write().unwrap() = None;
}
