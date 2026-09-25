use serde_json::json;

use super::harness::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smart_desactivado_por_defecto_y_lectura() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;

    // desactivado: no ejecuta nada
    let r = s.get("/api/system/smart", &admin).await.json();
    assert_eq!(r["enabled"], false);
    assert_eq!(r["disks"], json!([]));
    assert_eq!(s.get("/api/system/smart", &op).await.status, 403);

    // smartctl falso (en vez de sudo + wrapper) que responde un NVMe sano
    let fake = s.root.join("fake-smartctl");
    std::fs::write(&fake, format!("#!/bin/sh\ncat <<'EOF'\n{}\nEOF\n", crate::handlers::smart::fixtures::NVME_OK)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    *crate::handlers::smart::TEST_COMMAND.write().unwrap() = Some(fake.to_string_lossy().to_string());

    let r = s.put("/api/system/smart", &admin, json!({ "enabled": true })).await;
    assert_eq!(r.status, 200);
    let r = r.json();
    assert_eq!(r["enabled"], true);
    // el entorno puede no tener discos fisicos visibles (lsblk); si los hay, se leen
    for d in r["disks"].as_array().unwrap() {
        assert_eq!(d["status"], "ok", "{}", d);
        assert_eq!(d["percentage_used"], 3);
    }
    if !r["disks"].as_array().unwrap().is_empty() {
        assert_eq!(r["ready"], true);
    }
    *crate::handlers::smart::TEST_COMMAND.write().unwrap() = None;
}
