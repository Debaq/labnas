//! Modulo de camaras de punta a punta con una fuente sintetica de ffmpeg
//! (testsrc + tono de 440 Hz en lugar de webcam y microfono)

use serde_json::json;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::harness::{wait_until, Server};
use crate::handlers::cameras::hub::TEST_INPUT;

async fn count_detections(s: &Server, token: &str, q: &str) -> usize {
    s.get(&format!("/api/cameras/detections?{}", q), token).await.json().as_array().unwrap().len()
}

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

    // Muchas vistas que entran y salen (la grilla refresca cada segundo) mientras otra
    // sigue mirando: antes cada aviso podia cortar una lectura a mitad de cuadro, el
    // flujo se desincronizaba y ffmpeg moria con "Broken pipe"
    for _ in 0..40 {
        let v = s.client().get(format!("{}/api/cameras/{}/stream", s.base, cam)).bearer_auth(&op).send().await.unwrap();
        drop(v);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let mut more = 0;
    while more < 20_000 {
        let chunk = tokio::time::timeout(Duration::from_secs(10), live.chunk()).await.unwrap().unwrap().expect("la vista sigue recibiendo cuadros");
        more += chunk.len();
    }
    let list = s.get("/api/cameras", &op).await.json();
    let me = list["cameras"].as_array().unwrap().iter().find(|c| c["id"] == cam.as_str()).unwrap().clone();
    assert!(me["error"].is_null(), "sin error de ffmpeg: {}", me["error"]);

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deteccion_con_ia() {
    use crate::handlers::cameras::detect::TEST_DETECTIONS;
    use crate::handlers::cameras::vision::Detection;

    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let obs = s.user(&admin, "obs", "observador").await;
    assert_eq!(s.put("/api/modules/cameras", &admin, json!({ "enabled": true })).await.status, 200);
    let r = s.post("/api/cameras", &admin, json!({ "name": "Sala", "kind": "usb", "device": "/dev/video0", "resolution": "640x480" })).await;
    let cam = r.json()["id"].as_str().unwrap().to_string();

    // Validacion y permisos
    let regla = json!({
        "camera_id": cam, "kind": "personas", "min_score": 0.5, "interval_s": 1, "cooldown_s": 30,
        "schedule": null, "actions": { "photo": true, "telegram": true, "clip_s": 3 }, "enabled": true,
    });
    assert_eq!(s.post("/api/cameras/detectors", &op, regla.clone()).await.status, 403);
    let mut mala = regla.clone();
    mala["kind"] = json!("objetos");
    assert_eq!(s.post("/api/cameras/detectors", &admin, mala).await.status, 400, "objetos sin clases");
    let mut otra = regla.clone();
    otra["camera_id"] = json!("no-existe");
    assert_eq!(s.post("/api/cameras/detectors", &admin, otra).await.status, 404);
    // Es vigilancia: solo el admin (y quien autorice) ve reglas e historial
    assert_eq!(s.get("/api/cameras/detectors", &obs).await.status, 403);
    assert_eq!(s.get("/api/cameras/detectors", &op).await.status, 403);
    assert_eq!(s.get("/api/cameras/detections", &op).await.status, 403);
    assert_eq!(s.put("/api/cameras/detectors/watchers", &op, json!({ "users": ["op"] })).await.status, 403);
    assert_eq!(s.put("/api/cameras/detectors/watchers", &admin, json!({ "users": ["nadie"] })).await.status, 400);
    assert_ne!(s.get("/api/cameras/detecting-now", &admin).await.status, 200, "ya no hay aviso publico");

    if !ffmpeg_ok() {
        eprintln!("ffmpeg no instalado: se omite la parte de video");
        return;
    }
    TEST_INPUT.store(true, Ordering::SeqCst);
    std::fs::create_dir_all(s.home.join("Grabaciones")).unwrap();
    assert_eq!(s.put("/api/cameras/settings", &admin, json!({ "folder": s.path("Grabaciones"), "max_minutes": 1 })).await.status, 200);
    *TEST_DETECTIONS.lock().unwrap() =
        Some(vec![Detection { label: "persona".into(), score: 0.87, x: 10.0, y: 10.0, w: 50.0, h: 80.0, class: 0 }]);
    // El gestor de tareas queda inactivo en los tests: el supervisor se lanza a mano
    let supervisor = tokio::spawn(crate::handlers::cameras::detect::detection_loop(s.state.clone()));

    // Probar sin guardar: devuelve la foto marcada
    let r = s.post("/api/cameras/detectors/test", &admin, regla.clone()).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["detections"][0]["label"], "persona");
    assert!(r.json()["image"].as_str().unwrap().len() > 1000);

    // Regla activa: todos ven el aviso y se dispara con sus acciones
    let r = s.post("/api/cameras/detectors", &admin, regla.clone()).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let rule_id = r.json()["id"].as_str().unwrap().to_string();
    wait_until(20_000, || async { !s.get("/api/cameras/detections", &admin).await.json().as_array().unwrap().is_empty() }).await;
    let det = s.get("/api/cameras/detections", &admin).await.json()[0].clone();
    assert_eq!((det["camera_name"].as_str(), det["kind"].as_str()), (Some("Sala"), Some("personas")));
    assert_eq!(det["labels"], "persona (87%)");
    // Se sabe que regla disparo y que el aviso no llego (no hay bot en los tests)
    assert_eq!(det["detector_id"], rule_id.as_str());
    assert_eq!(det["tg_sent"], 0);
    assert!(det["tg_error"].as_str().unwrap().contains("bot"), "{}", det["tg_error"]);
    // Filtros y paginacion
    assert_eq!(count_detections(&s, &admin, &format!("rule={}", rule_id)).await, 1);
    assert_eq!(count_detections(&s, &admin, &"rule=otra".into()).await, 0);
    assert_eq!(count_detections(&s, &admin, &format!("camera={}", cam)).await, 1);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert_eq!(count_detections(&s, &admin, &format!("day={}", today)).await, 1);
    assert_eq!(count_detections(&s, &admin, &"day=2020-01-01".into()).await, 0);
    assert_eq!(count_detections(&s, &admin, &format!("before={}", urlencoding::encode(det["at"].as_str().unwrap()))).await, 0);
    assert_eq!(s.get("/api/cameras/detections?day=ayer", &admin).await.status, 400);
    // El operador no ve ni descarga lo que hizo la IA
    let recs_op = s.get("/api/cameras/recordings", &op).await.json();
    assert!(recs_op.as_array().unwrap().iter().all(|r| !r["username"].as_str().unwrap().starts_with("IA (")));
    let foto_id = det["photo_id"].as_str().unwrap().to_string();
    assert_eq!(s.get(&format!("/api/cameras/recordings/{}/download", foto_id), &op).await.status, 403);
    // Hasta que el admin lo autoriza
    assert_eq!(s.put("/api/cameras/detectors/watchers", &admin, json!({ "users": ["op"] })).await.status, 200);
    assert_eq!(s.get("/api/cameras/detections", &op).await.status, 200);
    assert_eq!(s.get(&format!("/api/cameras/recordings/{}/download", foto_id), &op).await.status, 200);
    // Vista previa: en el navegador (queda en la auditoria) y miniatura
    let inline = s.get(&format!("/api/cameras/recordings/{}/download?view=inline", foto_id), &op).await;
    assert!(inline.header("content-disposition").starts_with("inline"));
    let thumb = s.get(&format!("/api/cameras/recordings/{}/download?view=thumb", foto_id), &op).await;
    assert_eq!(thumb.status, 200);
    assert_eq!(thumb.header("content-type"), "image/jpeg");
    assert!(!s.get("/api/audit?q=Vio", &admin).await.json().as_array().unwrap().is_empty());
    assert!(s.get("/api/cameras/detectors", &op).await.json()["watchers"].as_array().unwrap().is_empty(), "la lista solo la ve el admin");
    assert_eq!(s.get("/api/cameras/detectors", &admin).await.json()["watchers"], json!(["op"]));
    let recs = s.get("/api/cameras/recordings", &op).await.json();
    let foto = recs.as_array().unwrap().iter().find(|r| r["id"] == det["photo_id"]).expect("foto registrada");
    assert_eq!(foto["username"], "IA (personas)");
    assert!(std::path::Path::new(foto["path"].as_str().unwrap()).exists());
    assert!(recs.as_array().unwrap().iter().any(|r| r["id"] == det["clip_id"] && r["kind"] == "video"), "clip iniciado");
    let audit = s.get("/api/audit?q=detecto", &admin).await.json();
    assert!(audit[0]["details"].as_str().unwrap().contains("persona (87%)"));

    // Pausa entre avisos: sigue viendo la persona, pero no repite en 30 s
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(s.get("/api/cameras/detections", &op).await.json().as_array().unwrap().len(), 1);
    let rules = s.get("/api/cameras/detectors", &op).await.json();
    assert_eq!(rules["rules"][0]["active_now"], true);
    assert_eq!(rules["rules"][0]["last_labels"], "persona (87%)");

    // Pausarla: deja de vigilar
    let mut pausada = regla.clone();
    pausada["enabled"] = json!(false);
    assert_eq!(s.put(&format!("/api/cameras/detectors/{}", rule_id), &admin, pausada).await.status, 200);
    let rules = s.get("/api/cameras/detectors", &admin).await.json();
    assert_eq!((rules["rules"][0]["enabled"].as_bool(), rules["rules"][0]["active_now"].as_bool()), (Some(false), Some(false)));

    // Movimiento (sin IA): el video sintetico cambia en cada cuadro
    *TEST_DETECTIONS.lock().unwrap() = None;
    let mov = json!({
        "camera_id": cam, "kind": "movimiento", "min_score": 0.02, "interval_s": 1, "cooldown_s": 60,
        "schedule": null, "actions": { "photo": true, "telegram": false, "clip_s": 0 }, "enabled": true,
    });
    let r = s.post("/api/cameras/detectors/test", &admin, mov.clone()).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["detections"][0]["label"], "movimiento");
    assert_eq!(s.post("/api/cameras/detectors", &admin, mov).await.status, 200);
    wait_until(20_000, || async {
        s.get("/api/cameras/detections", &op).await.json().as_array().unwrap().iter().any(|d| d["kind"] == "movimiento")
    })
    .await;

    // Borrar la regla conserva el historial
    assert_eq!(s.delete(&format!("/api/cameras/detectors/{}", rule_id), &admin).await.status, 204);
    assert_eq!(s.get("/api/cameras/detections", &op).await.json().as_array().unwrap().len(), 2);

    supervisor.abort();
    assert_eq!(s.put("/api/modules/cameras", &admin, json!({ "enabled": false })).await.status, 200);
    TEST_INPUT.store(false, Ordering::SeqCst);
}
