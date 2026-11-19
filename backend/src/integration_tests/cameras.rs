//! Modulo de camaras de punta a punta con una fuente sintetica de ffmpeg
//! (testsrc + tono de 440 Hz en lugar de webcam y microfono)

use serde_json::json;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::harness::{wait_until, Server};
use crate::handlers::cameras::hub::TEST_INPUT;

fn ffmpeg_ok() -> bool {
    std::process::Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn camaras_de_punta_a_punta() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let obs = s.user(&admin, "obs", "observador").await;

    // Viene apagado (privacidad): sin modulo no existe
    assert_eq!(s.get("/api/cameras", &admin).await.status, 404);
    assert_eq!(s.put("/api/modules/cameras", &admin, json!({ "enabled": true })).await.status, 200);

    // Permisos: el observador solo ve el aviso de grabacion
    assert_eq!(s.get("/api/cameras", &obs).await.status, 403);
    assert_eq!(s.get("/api/cameras/recording-now", &obs).await.json(), json!([]));
    assert_eq!(s.get("/api/cameras/detect", &op).await.status, 403);

    // Configuracion (admin) con validacion
    let usb = json!({ "name": "Sala", "kind": "usb", "device": "/dev/video0", "audio": "plughw:CARD=Camera,DEV=0", "resolution": "640x480" });
    assert_eq!(s.post("/api/cameras", &op, usb.clone()).await.status, 403);
    let r = s.post("/api/cameras", &admin, usb).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let cam = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.post("/api/cameras", &admin, json!({ "name": "X", "kind": "ip", "url": "file:///etc/passwd" })).await.status, 400);
    let ip = s.post("/api/cameras", &admin, json!({ "name": "Patio", "kind": "ip", "url": "rtsp://admin:secreta@10.0.0.9/s1", "audio": "stream" })).await;
    let ip_id = ip.json()["id"].as_str().unwrap().to_string();
    // Editar sin URL conserva la guardada
    assert_eq!(s.put(&format!("/api/cameras/{}", ip_id), &admin, json!({ "name": "Patio norte", "kind": "ip", "url": "" })).await.status, 200);

    // Camara de una impresora 3D: aparece sola, no editable
    s.post("/api/printers3d", &admin, json!({ "name": "Prusa", "ip": "10.0.0.7", "port": 7125, "printer_type": "Moonraker" })).await;

    let list = s.get("/api/cameras", &op).await.json();
    let cams = list["cameras"].as_array().unwrap();
    let patio = cams.iter().find(|c| c["id"] == ip_id.as_str()).unwrap();
    assert_eq!(patio["name"], "Patio norte");
    assert_eq!(patio["url"], "rtsp://admin:***@10.0.0.9/s1", "la clave no sale del servidor");
    let p3d = cams.iter().find(|c| c["kind"] == "printer3d").expect("camara de la impresora 3D");
    assert_eq!(p3d["editable"], false);
    assert_eq!(p3d["url"], "http://10.0.0.7:8080/?action=stream");

    // Sin carpeta no se graba
    assert_eq!(s.post(&format!("/api/cameras/{}/record", cam), &op, json!({})).await.status, 400);
    std::fs::create_dir_all(s.home.join("Grabaciones")).unwrap();
    let settings = json!({ "folder": s.path("Grabaciones"), "max_minutes": 1 });
    assert_eq!(s.put("/api/cameras/settings", &op, settings.clone()).await.status, 403);
    assert_eq!(s.put("/api/cameras/settings", &admin, settings).await.status, 200);
    assert_eq!(s.put("/api/cameras/settings", &admin, json!({ "folder": "/etc", "max_minutes": 1 })).await.status, 403);

    if !ffmpeg_ok() {
        eprintln!("ffmpeg no instalado: se omite la parte de video");
        return;
    }
    TEST_INPUT.store(true, Ordering::SeqCst);

    // Foto del cuadro actual
    let snap = s.client().get(format!("{}/api/cameras/{}/snapshot", s.base, cam)).bearer_auth(&op).send().await.unwrap();
    assert_eq!(snap.status(), 200);
    assert!(snap.bytes().await.unwrap().starts_with(&[0xFF, 0xD8]), "JPEG");

    // Vista en vivo: multipart con cuadros JPEG
    let mut live = s.client().get(format!("{}/api/cameras/{}/stream", s.base, cam)).bearer_auth(&op).send().await.unwrap();
    assert!(live.headers()["content-type"].to_str().unwrap().starts_with("multipart/x-mixed-replace"));
    let mut seen = Vec::new();
    while seen.len() < 4096 {
        let chunk = tokio::time::timeout(Duration::from_secs(10), live.chunk()).await.unwrap().unwrap().unwrap();
        seen.extend_from_slice(&chunk);
    }
    let head = String::from_utf8_lossy(&seen[..200.min(seen.len())]).to_string();
    assert!(head.contains("--frame") && head.contains("image/jpeg"), "{}", head);

    // Grabar: todos ven el aviso; no se puede grabar dos veces a la vez
    let r = s.post(&format!("/api/cameras/{}/record", cam), &op, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let path = r.json()["path"].as_str().unwrap().to_string();
    let now = s.get("/api/cameras/recording-now", &obs).await.json();
    assert_eq!(now[0]["by"], "op");
    assert_eq!(now[0]["camera"], "Sala");
    assert_eq!(s.post(&format!("/api/cameras/{}/record", cam), &admin, json!({})).await.status, 409);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(s.post(&format!("/api/cameras/{}/stop", cam), &op, json!({})).await.status, 200);
    drop(live);

    // El supervisor cierra el archivo y marca la grabacion
    wait_until(15_000, || async {
        let l = s.get("/api/cameras/recordings", &op).await.json();
        l.as_array().unwrap().iter().any(|r| r["kind"] == "video" && r["status"] == "ok")
    })
    .await;
    assert!(std::fs::metadata(&path).unwrap().len() > 1000);
    let probe = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", &path])
        .output();
    if let Ok(p) = probe {
        let kinds = String::from_utf8_lossy(&p.stdout).to_string();
        assert!(kinds.contains("video") && kinds.contains("audio"), "video con audio: {}", kinds);
    }
    assert!(s.get("/api/cameras/recording-now", &obs).await.json().as_array().unwrap().is_empty());

    // Foto guardada en la carpeta
    let r = s.post(&format!("/api/cameras/{}/photo", cam), &op, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert!(std::path::Path::new(r.json()["path"].as_str().unwrap()).exists());

    // Descarga y borrado (solo admin; el registro queda)
    let list = s.get("/api/cameras/recordings", &op).await.json();
    let video = list.as_array().unwrap().iter().find(|r| r["kind"] == "video").unwrap()["id"].as_str().unwrap().to_string();
    let dl = s.get(&format!("/api/cameras/recordings/{}/download", video), &op).await;
    assert_eq!(dl.status, 200);
    assert_eq!(dl.header("content-type"), "video/mp4");
    assert_eq!(s.delete(&format!("/api/cameras/recordings/{}", video), &op).await.status, 403);
    assert_eq!(s.delete(&format!("/api/cameras/recordings/{}", video), &admin).await.status, 204);
    assert!(!std::path::Path::new(&path).exists());
    let row = s.get("/api/cameras/recordings", &op).await.json();
    let row = row.as_array().unwrap().iter().find(|r| r["id"] == video.as_str()).unwrap().clone();
    assert_eq!(row["status"], "borrado");
    assert_eq!(row["deleted_by"], "admin");
    assert_eq!(s.get(&format!("/api/cameras/recordings/{}/download", video), &op).await.status, 410);

    // Apagar el modulo corta una grabacion en curso
    assert_eq!(s.post(&format!("/api/cameras/{}/record", cam), &op, json!({})).await.status, 200);
    assert_eq!(s.put("/api/modules/cameras", &admin, json!({ "enabled": false })).await.status, 200);
    assert!(crate::handlers::cameras::hub::active_recordings().is_empty());

    TEST_INPUT.store(false, Ordering::SeqCst);
}
