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

fn restic_available() -> bool {
    std::process::Command::new("restic").arg("version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn restic_body(name: &str, source: &str, repo: &str, password: Option<&str>) -> Value {
    let mut b = job_body(name, source, repo, 3);
    b["kind"] = json!("restic");
    if let Some(p) = password {
        b["restic_password"] = json!(p);
    }
    b
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn validaciones_restic() {
    let s = Server::start().await;
    let admin = s.admin().await;
    std::fs::create_dir_all(s.home.join("Docs")).unwrap();
    let docs = s.path("Docs");

    let cases = [
        (restic_body("x", &docs, "sftp:bk@nas2:/srv/labnas", None), 400),                 // sin contraseña
        (restic_body("x", &docs, "sftp:bk@nas2:/srv/labnas", Some("corta")), 400),        // contraseña corta
        (restic_body("x", &docs, "relativa/repo", Some("una-clave-larga")), 400),
        (restic_body("x", &docs, "--password-command=evil", Some("una-clave-larga")), 400),
        (restic_body("x", &docs, &s.path("Docs/repo"), Some("una-clave-larga")), 400),     // dentro del origen
    ];
    for (body, expected) in cases {
        let r = s.post("/api/backups", &admin, body.clone()).await;
        assert_eq!(r.status, expected, "{}", body);
    }
    let mut env_malo = restic_body("x", &docs, "b2:bucket:labnas", Some("una-clave-larga"));
    env_malo["restic_env"] = json!("LD_PRELOAD=/tmp/x.so");
    assert_eq!(s.post("/api/backups", &admin, env_malo).await.status, 400);

    // Remoto valido: la contraseña y las credenciales quedan cifradas y no vuelven al cliente
    let mut ok = restic_body("B2", &docs, "b2:bucket:labnas", Some("una-clave-larga"));
    ok["restic_env"] = json!("B2_ACCOUNT_ID=id\nB2_ACCOUNT_KEY=secreta");
    let r = s.post("/api/backups", &admin, ok).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["kind"], "restic");
    assert_eq!(r.json()["has_password"], true);
    assert!(!r.text.contains("una-clave-larga") && !r.text.contains("secreta"));
    let (pass, env): (String, String) = s.state.db.get().unwrap()
        .query_row("SELECT restic_password, restic_env FROM backup_jobs WHERE kind = 'restic'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert!(pass.starts_with("enc:v1:") && env.starts_with("enc:v1:"));

    // Editar sin mandar la contraseña la conserva
    let id = r.json()["id"].as_str().unwrap().to_string();
    let mut edit = restic_body("B2 renombrado", &docs, "b2:bucket:labnas", None);
    edit["keep"] = json!(10);
    let r = s.put(&format!("/api/backups/{}", id), &admin, edit).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["has_password"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn respaldo_restic_y_restauracion() {
    if !restic_available() {
        eprintln!("restic no disponible; test omitido");
        return;
    }
    let s = Server::start().await;
    let admin = s.admin().await;
    std::fs::create_dir_all(s.home.join("Docs")).unwrap();
    std::fs::write(s.home.join("Docs/tesis.txt"), "capitulo 1").unwrap();
    let repo = s.root.join("disco/restic").to_string_lossy().to_string();

    let job = s.post("/api/backups", &admin, restic_body("Docs", &s.path("Docs"), &repo, Some("una-clave-larga"))).await.json();
    let id = job["id"].as_str().unwrap().to_string();
    assert_eq!(s.post(&format!("/api/backups/{}/run", id), &admin, json!({})).await.status, 202);
    wait_until(60_000, || async { matches!(last_status(&s, &admin, &id).await.as_deref(), Some("ok") | Some("warning") | Some("error")) }).await;
    let info = s.get("/api/backups", &admin).await.json();
    let j = info["jobs"].as_array().unwrap().iter().find(|j| j["id"] == id).unwrap().clone();
    assert_eq!(j["last_status"], "ok", "{}", j["last_message"]);

    let snaps = s.get(&format!("/api/backups/{}/snapshots", id), &admin).await.json();
    let snap = snaps[0].as_str().expect("un snapshot").to_string();

    // El repositorio esta cifrado: el texto no aparece en disco
    let mut found = false;
    for e in walk(std::path::Path::new(&repo)) {
        if std::fs::read(&e).map(|b| b.windows(10).any(|w| w == b"capitulo 1")).unwrap_or(false) {
            found = true;
        }
    }
    assert!(!found, "el contenido no debe quedar legible en el repositorio");

    std::fs::write(s.home.join("Docs/tesis.txt"), "borrado por error").unwrap();
    let r = s.post(&format!("/api/backups/{}/restore", id), &admin, json!({ "snapshot": snap })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let restored: Vec<_> = walk(&s.home.join("Docs/.labnas-restore")).into_iter().filter(|p| p.ends_with("tesis.txt")).collect();
    assert_eq!(std::fs::read_to_string(&restored[0]).unwrap(), "capitulo 1");
    assert_eq!(s.post(&format!("/api/backups/{}/restore", id), &admin, json!({ "snapshot": "--target=/" })).await.status, 400);
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}
