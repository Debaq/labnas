//! Escaner desde la web: sesiones de paginas, PDF/JPG y guardado en carpeta (sin SANE real)

use serde_json::json;

use super::harness::Server;
use crate::handlers::printing::{ScanDevice, TEST_SCANNERS, TEST_SCAN_OUTPUT};

const DEV: &str = "brother3:bus2;dev1";

fn pnm() -> Vec<u8> {
    // 4x2 gris
    [b"P5\n4 2\n255\n".as_slice(), &[0, 64, 128, 255, 255, 128, 64, 0]].concat()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn escaneo_desde_la_web() {
    let s = Server::start().await;
    *TEST_SCANNERS.lock().unwrap() = vec![ScanDevice { id: DEV.into(), name: "Brother DCP-165C".into() }];
    *TEST_SCAN_OUTPUT.lock().unwrap() = Some(pnm());
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let obs = s.user(&admin, "obs", "observador").await;
    let no_print = json!({ "role": "observador", "overrides": { "printing": "ver" } });
    s.post("/api/auth/users/obs/role", &admin, no_print).await;

    // Sin permiso de impresion no se escanea ni se listan escaneres
    assert_eq!(s.get("/api/printing/scan/devices", &obs).await.status, 403);
    assert_eq!(s.post("/api/printing/scan/pages", &obs, json!({ "device": DEV })).await.status, 403);

    let r = s.get("/api/printing/scan/devices", &op).await.json();
    assert_eq!(r["available"], true);
    assert_eq!(r["devices"][0]["id"], DEV);
    let enc = urlencoding::encode(DEV).into_owned();
    let o = s.get(&format!("/api/printing/scan/options?device={}", enc), &op).await.json();
    assert_eq!(o["default_resolution"], 150);

    // Escaner desconocido o valor fuera de la lista
    assert_eq!(s.post("/api/printing/scan/pages", &op, json!({ "device": "net:otro" })).await.status, 404);
    assert_eq!(s.post("/api/printing/scan/pages", &op, json!({ "device": DEV, "mode": "--help" })).await.status, 400);
    assert_eq!(s.post("/api/printing/scan/pages", &op, json!({ "device": DEV, "resolution": 9600 })).await.status, 400);

    // Dos paginas en la misma sesion
    let r = s.post("/api/printing/scan/pages", &op, json!({ "device": DEV, "mode": "Gray", "resolution": 300 })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let id = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(r.json()["pages"][0]["width"], 4);
    assert_eq!(r.json()["pages"][0]["gray"], true);
    let r = s.post("/api/printing/scan/pages", &op, json!({ "session": id, "device": DEV })).await.json();
    assert_eq!(r["pages"].as_array().unwrap().len(), 2);

    // La sesion es solo de quien escaneo
    assert_eq!(s.get(&format!("/api/printing/scan/sessions/{}", id), &admin).await.status, 404);
    assert_eq!(s.post("/api/printing/scan/pages", &admin, json!({ "session": id, "device": DEV })).await.status, 404);

    let img = s.get(&format!("/api/printing/scan/sessions/{}/pages/0", id), &op).await;
    assert_eq!(img.header("content-type"), "image/jpeg");

    let pdf = s.get(&format!("/api/printing/scan/sessions/{}/download?format=pdf&name=Factura%2042", id), &op).await;
    assert_eq!(pdf.header("content-type"), "application/pdf");
    assert!(pdf.text.starts_with("%PDF-1.4") && pdf.text.contains("/Count 2"));
    assert!(pdf.header("content-disposition").contains("Factura%2042.pdf"));
    let zip = s.get(&format!("/api/printing/scan/sessions/{}/download?format=jpg", id), &op).await;
    assert_eq!(zip.header("content-type"), "application/zip");

    // Guardar en una carpeta del NAS (sin pisar archivos)
    std::fs::create_dir_all(s.home.join("Escaneos")).unwrap();
    let folder = s.path("Escaneos");
    let body = json!({ "folder": folder, "name": "acta", "format": "pdf" });
    let r = s.post(&format!("/api/printing/scan/sessions/{}/save", id), &op, body.clone()).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert!(s.home.join("Escaneos/acta.pdf").exists());
    s.post(&format!("/api/printing/scan/sessions/{}/save", id), &op, body).await;
    assert!(s.home.join("Escaneos/acta (2).pdf").exists());
    let fuera = json!({ "folder": "/etc", "format": "pdf" });
    assert_eq!(s.post(&format!("/api/printing/scan/sessions/{}/save", id), &op, fuera).await.status, 403);

    // Quitar una pagina y cerrar la sesion
    let r = s.delete(&format!("/api/printing/scan/sessions/{}/pages/0", id), &op).await.json();
    assert_eq!(r["pages"].as_array().unwrap().len(), 1);
    assert_eq!(s.delete(&format!("/api/printing/scan/sessions/{}", id), &op).await.status, 204);
    assert_eq!(s.get(&format!("/api/printing/scan/sessions/{}", id), &op).await.status, 404);

    TEST_SCANNERS.lock().unwrap().clear();
    *TEST_SCAN_OUTPUT.lock().unwrap() = None;
}
