use serde_json::{json, Value};

use super::harness::{wait_until, Server};

fn rsync_available() -> bool {
    std::process::Command::new("rsync").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn job_body(name: &str, source: &str, dest: &str, keep: u32) -> Value {
    json!({ "name": name, "source": source, "destination": dest, "hour": 3, "minute": 0, "keep": keep })
}

async fn last_status(s: &Server, admin: &str, id: &str) -> Option<String> {
    let info = s.get("/api/backups", admin).await.json();
    info["jobs"].as_array()?.iter().find(|j| j["id"] == id)?["last_status"].as_str().map(String::from)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn validaciones_de_respaldo() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    std::fs::create_dir_all(s.home.join("Docs")).unwrap();
    let disco = s.root.join("disco").to_string_lossy().to_string();

    assert_eq!(s.get("/api/backups", &op).await.status, 403, "solo admin");
    let cases = [
        (job_body("x", "/etc", &disco, 3), 403),
        (job_body("x", &s.path(""), &s.path("Docs/bk"), 3), 400),
        (job_body("x", &s.path("Docs"), &s.path(".labnas/bk"), 3), 400),
        (job_body("x", &s.path("Docs"), &disco, 0), 400),
        (job_body("", &s.path("Docs"), &disco, 3), 400),
    ];
    for (body, expected) in cases {
        let r = s.post("/api/backups", &admin, body.clone()).await;
        assert_eq!(r.status, expected, "{}", body);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn respaldo_manual_y_error() {
    if !rsync_available() {
        eprintln!("rsync no disponible; test omitido");
        return;
    }
    let s = Server::start().await;
    let admin = s.admin().await;
    std::fs::create_dir_all(s.home.join("Docs")).unwrap();
    std::fs::write(s.home.join("Docs/a.txt"), "hola").unwrap();
    let disco = s.root.join("disco");
    std::fs::create_dir_all(&disco).unwrap();

    let job = s.post("/api/backups", &admin, job_body("Docs", &s.path("Docs"), &disco.join("docs").to_string_lossy(), 2)).await.json();
    let id = job["id"].as_str().unwrap().to_string();
    assert_eq!(s.post(&format!("/api/backups/{}/run", id), &admin, json!({})).await.status, 202);
    wait_until(10_000, || async { matches!(last_status(&s, &admin, &id).await.as_deref(), Some("ok") | Some("error")) }).await;
    assert_eq!(last_status(&s, &admin, &id).await.as_deref(), Some("ok"));

    let snap = std::fs::read_link(disco.join("docs/latest")).unwrap();
    let snap_dir = disco.join("docs").join(&snap);
    assert_eq!(std::fs::read_to_string(snap_dir.join("a.txt")).unwrap(), "hola");
    assert!(snap_dir.join("_labnas/labnas.db").exists(), "incluye la base de datos");
    assert_eq!(s.get(&format!("/api/backups/{}/snapshots", id), &admin).await.json()[0], snap.to_string_lossy().as_ref());
    assert_eq!(s.get("/api/audit?q=Respaldo%20ok", &admin).await.json()[0]["action"], "Respaldo ok");

    // destino sin permiso de escritura => error (salvo que el test corra como root)
    if unsafe { libc::geteuid() } != 0 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&disco, std::fs::Permissions::from_mode(0o500)).unwrap();
        let bad = s.post("/api/backups", &admin, job_body("Malo", &s.path("Docs"), &disco.join("nuevo").to_string_lossy(), 2)).await.json();
        let bad_id = bad["id"].as_str().unwrap().to_string();
        s.post(&format!("/api/backups/{}/run", bad_id), &admin, json!({})).await;
        wait_until(10_000, || async { matches!(last_status(&s, &admin, &bad_id).await.as_deref(), Some("ok") | Some("error")) }).await;
        std::fs::set_permissions(&disco, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(last_status(&s, &admin, &bad_id).await.as_deref(), Some("error"));
    }
}
