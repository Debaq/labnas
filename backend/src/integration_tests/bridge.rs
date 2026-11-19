//! Puente PHP de punta a punta: conector real con `php -S`, enlace por codigo,
//! pedidos de "internet" atendidos por LabNAS y politica de denegacion.
//! Se salta si no hay `php` (en el CI se instala).

use serde_json::{json, Value};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use super::harness::{wait_until, Server};

struct Php(Child);
impl Drop for Php {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn php_available() -> bool {
    Command::new("php").arg("-v").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn puente_php_de_punta_a_punta() {
    if !php_available() {
        eprintln!("php no instalado: se omite el test del puente");
        return;
    }
    let s = Server::start().await;
    let admin = s.admin().await;

    // "Hosting": public_html con el conector y su frontend; datos fuera de la web
    let hosting = s.root.join("hosting");
    let public = hosting.join("public_html");
    copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../bridge"), &public);
    std::fs::create_dir_all(public.join("web/assets")).unwrap();
    std::fs::write(public.join("web/index.html"), "<html>LabNAS</html>").unwrap();
    std::fs::write(public.join("web/assets/app.js"), "console.log(1)").unwrap();

    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _php = Php(
        Command::new("php")
            .args(["-S", &format!("127.0.0.1:{}", port), "dev-router.php"])
            .current_dir(&public)
            // el servidor integrado atiende un pedido a la vez salvo que tenga workers (PHP >= 7.4)
            .env("PHP_CLI_SERVER_WORKERS", "8")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("php -S"),
    );
    let bridge = format!("http://127.0.0.1:{}", port);
    let web = reqwest::Client::new();
    wait_until(10_000, || async { web.get(format!("{}/instalar.php", bridge)).send().await.is_ok() }).await;

    // Instalador: rechaza una carpeta dentro de la web
    let html = web
        .post(format!("{}/instalar.php", bridge))
        .form(&[("data_dir", public.join("datos").to_string_lossy().to_string())])
        .send().await.unwrap().text().await.unwrap();
    assert!(html.contains("fuera de public_html"), "{}", html);
    let html = web
        .post(format!("{}/instalar.php", bridge))
        .form(&[("data_dir", hosting.join("labnas-data").to_string_lossy().to_string())])
        .send().await.unwrap().text().await.unwrap();
    let code = html.split("class=\"code\">").nth(1).and_then(|r| r.split('<').next()).expect("codigo en el instalador").to_string();
    assert_eq!(code.len(), 19, "XXXX-XXXX-XXXX-XXXX: {}", code);

    // Antes de enlazar, la API del puente no atiende
    assert_eq!(web.get(format!("{}/api/health", bridge)).send().await.unwrap().status(), 503);

    // Enlace desde LabNAS
    let r = s.post("/api/bridge/link", &admin, json!({ "url": bridge, "code": "AAAA-BBBB-CCCC-DDDD" })).await;
    assert_eq!(r.status, 400, "codigo incorrecto: {}", r.text);
    let r = s.post("/api/bridge/link", &admin, json!({ "url": bridge, "code": code })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["linked"], true);
    // El codigo era de un solo uso
    let again = web.post(format!("{}/_labnas/register", bridge)).body(json!({"code": code, "node_id": "0".repeat(32), "key": "0".repeat(64)}).to_string()).send().await.unwrap();
    assert_eq!(again.status(), 409);

    // Los pollers usan el router completo, como en main.rs
    let app = crate::build_api(&s.state, 50);
    tokio::spawn(crate::bridge::bridge_loop(s.state.clone(), app));

    // Navegador por internet: frontend servido por el conector
    let page = web.get(format!("{}/archivos", bridge)).send().await.unwrap();
    assert_eq!(page.text().await.unwrap(), "<html>LabNAS</html>", "rutas de la SPA -> index.html");
    let js = web.get(format!("{}/assets/app.js", bridge)).send().await.unwrap();
    assert!(js.headers()["cache-control"].to_str().unwrap().contains("immutable"));

    // API a traves de LabNAS
    let mut health = Value::Null;
    for _ in 0..40 {
        let r = web.get(format!("{}/api/health", bridge)).send().await.unwrap();
        if r.status() == 200 {
            health = r.json().await.unwrap();
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(health["via"], "bridge", "{}", health);
    assert!(health["ip"].is_null(), "no revela la IP de la LAN");

    let login: Value = web
        .post(format!("{}/api/auth/login", bridge))
        .json(&json!({ "username": "admin", "password": "adminadmin" }))
        .send().await.unwrap().json().await.unwrap();
    let token = login["token"].as_str().expect("login por el puente").to_string();

    std::fs::write(s.home.join("informe.txt"), "hola desde el laboratorio").unwrap();
    let r = web.get(format!("{}/api/files/download?path={}", bridge, urlencoding::encode(&s.path("informe.txt"))))
        .bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.text().await.unwrap(), "hola desde el laboratorio");

    // Subida (cuerpo binario por el relay)
    let form = reqwest::multipart::Form::new()
        .text("path", s.path(""))
        .part("file", reqwest::multipart::Part::bytes(vec![7u8; 300_000]).file_name("grande.bin"));
    let r = web.post(format!("{}/api/files/upload", bridge)).bearer_auth(&token).multipart(form).send().await.unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());
    assert_eq!(std::fs::read(s.home.join("grande.bin")).unwrap().len(), 300_000);

    // Politica: ni admin, ni terminal, ni registro, aunque sea el admin
    for (method, path) in [("GET", "/api/auth/users"), ("GET", "/api/terminal"), ("POST", "/api/live/ticket"), ("GET", "/api/bridge"), ("POST", "/api/auth/register")] {
        let r = web.request(method.parse().unwrap(), format!("{}{}", bridge, path)).bearer_auth(&token).send().await.unwrap();
        assert_eq!(r.status(), 403, "{} {}", method, path);
    }
    // Sin token sigue pidiendo login
    assert_eq!(web.get(format!("{}/api/files", bridge)).send().await.unwrap().status(), 401);

    // El canal LabNAS <-> puente exige firma
    let r = web.post(format!("{}/_labnas/poll", bridge)).send().await.unwrap();
    assert_eq!(r.status(), 401);

    // Queda en la auditoria con la IP del cliente
    let n: i64 = s.state.db.get().unwrap()
        .query_row("SELECT COUNT(*) FROM audit_log WHERE action = 'login_puente'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);

    // Estado visible en la UI
    let st = s.get("/api/bridge", &admin).await.json();
    assert_eq!(st["status"]["connected"], true, "{}", st);
    assert!(st["status"]["served"].as_u64().unwrap() >= 5);

    // Desenlazar corta el acceso
    assert_eq!(s.delete("/api/bridge", &admin).await.status, 204);
}
