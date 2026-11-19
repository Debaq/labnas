//! Snapshot de camara

use super::*;

// =====================
// Snapshot de cámara
// =====================

/// URL de snapshot de la impresora (configurada o la habitual segun el tipo)
pub(super) fn camera_url_for(printer: &Printer3DConfig) -> Option<String> {
    if let Some(url) = printer.camera_url.as_ref().filter(|u| !u.trim().is_empty()) {
        return Some(url.clone());
    }
    match printer.printer_type {
        Printer3DType::OctoPrint => Some(format!("{}/webcam/?action=snapshot", printer_base(printer))),
        Printer3DType::Moonraker | Printer3DType::CrealityStock => Some(format!("http://{}:8080/?action=snapshot", printer.ip)),
        Printer3DType::FlashForge => None,
    }
}

/// Descarga un snapshot: (content-type, bytes)
pub(super) async fn fetch_snapshot(
    client: &reqwest::Client,
    printer: &Printer3DConfig,
) -> Result<(String, axum::body::Bytes), (StatusCode, String)> {
    let camera_url = camera_url_for(printer).ok_or((
        StatusCode::NOT_IMPLEMENTED,
        "Sin camara. Configure URL de camara manualmente si tiene una.".to_string(),
    ))?;

    let resp = client
        .get(&camera_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error obteniendo imagen: {}", e)))?;

    if !resp.status().is_success() {
        return Err((StatusCode::BAD_GATEWAY, "No se pudo obtener snapshot de la cámara".to_string()));
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error leyendo imagen: {}", e)))?;
    Ok((content_type, bytes))
}

pub async fn camera_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let (content_type, bytes) = fetch_snapshot(&state.http_client, &printer).await?;

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, content_type),
            (axum::http::header::CACHE_CONTROL, "no-cache, no-store".to_string()),
        ],
        bytes,
    ))
}

/// URL de video continuo (MJPEG) a partir de la de snapshot: mjpg-streamer, que usan
/// OctoPrint y Moonraker, sirve el stream en `?action=stream`
pub(crate) fn stream_url_from_snapshot(url: &str) -> String {
    url.replace("action=snapshot", "action=stream")
}

/// Camaras de las impresoras 3D para el modulo de camaras: (id impresora, nombre, URL del stream)
pub async fn printer_stream_cameras(state: &AppState) -> Vec<(String, String, String)> {
    let printers = db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, ip, port, printer_type, labnas_decrypt(api_key), camera_url, power_watts, electricity_cost_kwh, section_id, \"order\" FROM printers3d ORDER BY \"order\"")
            .map_err(|e| format!("DB: {}", e))?;
        let rows = stmt.query_map([], row_to_printer).map_err(|e| format!("DB: {}", e))?;
        Ok(rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
    })
    .await
    .unwrap_or_default();
    printers
        .into_iter()
        .filter_map(|p| camera_url_for(&p).map(|u| (p.id.clone(), p.name.clone(), stream_url_from_snapshot(&u))))
        .collect()
}
