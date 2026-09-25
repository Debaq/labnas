//! Snapshot de camara

use super::*;

// =====================
// Snapshot de cámara
// =====================

pub async fn camera_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    // Determinar URL de la cámara
    let camera_url = if let Some(ref url) = printer.camera_url {
        url.clone()
    } else {
        match printer.printer_type {
            Printer3DType::OctoPrint => format!("{}/webcam/?action=snapshot", base),
            Printer3DType::Moonraker | Printer3DType::CrealityStock => {
                format!("http://{}:8080/?action=snapshot", printer.ip)
            }
            Printer3DType::FlashForge => {
                return Err((
                    StatusCode::NOT_IMPLEMENTED,
                    "Sin camara. Configure URL de camara manualmente si tiene una.".to_string(),
                ));
            }
        }
    };

    let resp = client
        .get(&camera_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error obteniendo imagen: {}", e)))?;

    if !resp.status().is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "No se pudo obtener snapshot de la cámara".to_string(),
        ));
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

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, content_type),
            (
                axum::http::header::CACHE_CONTROL,
                "no-cache, no-store".to_string(),
            ),
        ],
        bytes,
    ))
}
