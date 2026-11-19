//! Impresora de red IPP: token por usuario y protocolo, sin llegar a CUPS

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use serde_json::json;
use std::net::SocketAddr;
use tower::ServiceExt;

use super::harness::Server;
use crate::handlers::printing::ipp_codec::{self as codec, kw, mime, uri, Value, Writer};

const PRINTER: &str = "Impresora-Prueba";

struct IppResp {
    http: u16,
    status: u16,
    msg: Option<codec::Request>,
}

/// Manda una peticion IPP al router del puerto IPP (en memoria)
async fn ipp(s: &Server, path: &str, body: Vec<u8>) -> IppResp {
    let app = crate::handlers::printing::ipp_router(s.state.clone())
        .layer(MockConnectInfo(SocketAddr::from(([192, 168, 1, 50], 50000))));
    let req = axum::http::Request::post(path)
        .header("content-type", "application/ipp")
        .header("host", "labnas:8631")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let http = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    // La respuesta tiene la misma forma que una peticion: el "op" es el estado
    let msg = codec::parse_request(&bytes).ok().map(|(r, _)| r);
    IppResp { http, status: msg.as_ref().map(|m| m.op).unwrap_or(0xFFFF), msg }
}

fn request(op: u16, extra: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::new((2, 0), op, 1);
    w.attr("printer-uri", &[uri(&format!("ipp://labnas:8631/ipp/x/{}", PRINTER))]);
    w.attr("requesting-user-name", &[codec::name("pc-windows")]);
    extra(&mut w);
    w.finish()
}

fn printer_attr<'a>(r: &'a codec::Request, attr: &str) -> Option<&'a Vec<Value>> {
    r.groups
        .iter()
        .filter(|g| g.tag == codec::tag::PRINTER)
        .flat_map(|g| g.attrs.iter())
        .find(|a| a.name == attr)
        .map(|a| &a.values)
}

fn job_attr(r: &codec::Request, attr: &str) -> Option<Value> {
    r.groups
        .iter()
        .filter(|g| g.tag == codec::tag::JOB)
        .flat_map(|g| g.attrs.iter())
        .find(|a| a.name == attr)
        .and_then(|a| a.values.first().cloned())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn impresora_de_red_ipp() {
    let s = Server::start().await;
    crate::handlers::printing::TEST_PRINTERS.lock().unwrap().push(PRINTER.to_string());
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let obs = s.user(&admin, "obs", "observador").await;
    let no_print = json!({ "role": "observador", "permissions": { "terminal": false, "impresion": false, "archivos_escritura": false } });
    assert_eq!(s.post("/api/auth/users/obs/role", &admin, no_print.clone()).await.status, 200);

    // Sin permiso de impresion no hay token
    let info = s.get("/api/printing/ipp", &obs).await.json();
    assert_eq!(info["can_print"], false);
    assert_eq!(info["token"], serde_json::Value::Null);
    assert_eq!(s.post("/api/printing/ipp/token", &obs, json!({})).await.status, 403);

    // Con permiso: token de 32 hex, estable al consultar, distinto al regenerar
    let r = s.post("/api/printing/ipp/token", &op, json!({})).await;
    assert_eq!(r.status, 200);
    let token = r.json()["token"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 32);
    assert_eq!(r.json()["port"], 8631);
    assert_eq!(s.get("/api/printing/ipp", &op).await.json()["token"], token.as_str());
    let path = format!("/ipp/{}/{}", token, PRINTER);

    // Get-Printer-Attributes: lo minimo que pide el driver generico de Windows
    let r = ipp(&s, &path, request(codec::op::GET_PRINTER_ATTRIBUTES, |_| {})).await;
    assert_eq!((r.http, r.status), (200, codec::status::OK));
    let m = r.msg.unwrap();
    assert_eq!(printer_attr(&m, "printer-name").unwrap()[0].as_str(), Some(PRINTER));
    let formats: Vec<_> = printer_attr(&m, "document-format-supported").unwrap().iter().filter_map(Value::as_str).collect();
    assert!(formats.contains(&"application/pdf") && formats.contains(&"image/pwg-raster"));
    assert!(printer_attr(&m, "media-col-database").is_none(), "solo si se pide por nombre");
    let r = ipp(&s, &path, request(codec::op::GET_PRINTER_ATTRIBUTES, |w| {
        w.attr("requested-attributes", &[kw("all"), kw("media-col-database")]);
    }))
    .await;
    let m = r.msg.unwrap();
    assert!(!printer_attr(&m, "media-col-database").unwrap().is_empty());
    assert_eq!(printer_attr(&m, "ipp-features-supported").unwrap()[0].as_str(), Some("ipp-everywhere"));

    // requested-attributes filtra
    let r = ipp(&s, &path, request(codec::op::GET_PRINTER_ATTRIBUTES, |w| {
        w.attr("requested-attributes", &[kw("printer-state")]);
    }))
    .await;
    let m = r.msg.unwrap();
    assert!(printer_attr(&m, "printer-state").is_some());
    assert!(printer_attr(&m, "printer-name").is_none());

    // Token o impresora desconocidos: 404 sin detalles
    assert_eq!(ipp(&s, &format!("/ipp/{}/{}", "0".repeat(32), PRINTER), request(codec::op::GET_PRINTER_ATTRIBUTES, |_| {})).await.http, 404);
    assert_eq!(ipp(&s, &format!("/ipp/{}/NoExiste", token), request(codec::op::GET_PRINTER_ATTRIBUTES, |_| {})).await.http, 404);
    assert_eq!(ipp(&s, &path, b"basura".to_vec()).await.http, 400);

    // Formato no soportado
    let r = ipp(&s, &path, request(codec::op::VALIDATE_JOB, |w| {
        w.attr("document-format", &[mime("image/urf")]);
    }))
    .await;
    assert_eq!(r.status, codec::status::FORMAT_NOT_SUPPORTED);
    assert_eq!(ipp(&s, &path, request(codec::op::VALIDATE_JOB, |_| {})).await.status, codec::status::OK);

    // Create-Job queda pendiente; se consulta, aparece en Get-Jobs y se cancela
    let r = ipp(&s, &path, request(codec::op::CREATE_JOB, |w| {
        w.attr("job-name", &[codec::name("Informe.pdf")]);
    }))
    .await;
    assert_eq!(r.status, codec::status::OK);
    let m = r.msg.unwrap();
    let job_id = job_attr(&m, "job-id").and_then(|v| v.as_int()).unwrap();
    assert_eq!(job_attr(&m, "job-state"), Some(Value::Enum(3)));
    assert_eq!(job_attr(&m, "job-originating-user-name").unwrap().as_str(), Some("op"), "a nombre del usuario de LabNAS");

    let with_job = |w: &mut Writer| w.attr("job-id", &[Value::Int(job_id)]);
    let r = ipp(&s, &path, request(codec::op::GET_JOB_ATTRIBUTES, with_job)).await;
    assert_eq!(job_attr(&r.msg.unwrap(), "job-name").unwrap().as_str(), Some("Informe.pdf"));
    let r = ipp(&s, &path, request(codec::op::GET_JOBS, |_| {})).await;
    assert_eq!(r.msg.unwrap().groups.iter().filter(|g| g.tag == codec::tag::JOB).count(), 1);

    // Otro usuario no ve ni cancela el trabajo ajeno
    let adm_token = s.post("/api/printing/ipp/token", &admin, json!({})).await.json()["token"].as_str().unwrap().to_string();
    let adm_path = format!("/ipp/{}/{}", adm_token, PRINTER);
    assert_eq!(ipp(&s, &adm_path, request(codec::op::CANCEL_JOB, with_job)).await.status, codec::status::NOT_FOUND);

    assert_eq!(ipp(&s, &path, request(codec::op::CANCEL_JOB, with_job)).await.status, codec::status::OK);
    let r = ipp(&s, &path, request(codec::op::GET_JOB_ATTRIBUTES, with_job)).await;
    assert_eq!(job_attr(&r.msg.unwrap(), "job-state"), Some(Value::Enum(7)));
    assert_eq!(ipp(&s, &path, request(codec::op::CANCEL_JOB, with_job)).await.status, codec::status::NOT_POSSIBLE);

    // Operacion desconocida
    assert_eq!(ipp(&s, &path, request(0x0010, |_| {})).await.status, codec::status::OPERATION_NOT_SUPPORTED);

    // Modulo apagado: el puerto no responde nada
    assert_eq!(s.put("/api/modules/printing", &admin, json!({ "enabled": false })).await.status, 200);
    assert_eq!(ipp(&s, &path, request(codec::op::GET_PRINTER_ATTRIBUTES, |_| {})).await.http, 404);
    s.put("/api/modules/printing", &admin, json!({ "enabled": true })).await;

    // Al revocar, la URL vieja deja de funcionar
    assert_eq!(s.delete("/api/printing/ipp/token", &op).await.status, 204);
    assert_eq!(ipp(&s, &path, request(codec::op::GET_PRINTER_ATTRIBUTES, |_| {})).await.http, 404);
    assert_eq!(s.get("/api/printing/ipp", &op).await.json()["token"], serde_json::Value::Null);

    // Le quitan el permiso de impresion: el token responde prohibido
    let r = s.post("/api/printing/ipp/token", &op, json!({})).await.json();
    let token2 = r["token"].as_str().unwrap().to_string();
    assert_ne!(token2, token);
    s.post("/api/auth/users/op/role", &admin, no_print).await;
    let path2 = format!("/ipp/{}/{}", token2, PRINTER);
    assert_eq!(ipp(&s, &path2, request(codec::op::GET_PRINTER_ATTRIBUTES, |_| {})).await.status, codec::status::FORBIDDEN);

    crate::handlers::printing::TEST_PRINTERS.lock().unwrap().clear();
}
