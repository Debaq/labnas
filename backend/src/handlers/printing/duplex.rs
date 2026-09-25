//! Asistente de doble cara manual

use super::*;

/// Valida que un temp_id tenga formato UUID seguro
pub(super) fn validate_temp_id(id: &str) -> Result<(), (StatusCode, String)> {
    if id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "ID temporal invalido".to_string(),
        ));
    }
    Ok(())
}

/// POST /api/printing/duplex/prepare — Sube archivo y devuelve temp_id + conteo de paginas
pub async fn duplex_prepare(
    mut multipart: Multipart,
) -> Result<Json<DuplexPrepareResponse>, (StatusCode, String)> {
    let mut file_name = String::new();
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            file_name = field
                .file_name()
                .unwrap_or("document")
                .to_string();
            file_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                    .to_vec(),
            );
        }
    }

    let file_data = file_data.ok_or((
        StatusCode::BAD_REQUEST,
        "No se proporciono archivo".to_string(),
    ))?;

    if !is_printable_file(&file_name) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Formato no soportado: '{}'. Usa PDF, imagenes (PNG/JPG) o texto plano.",
                file_name
            ),
        ));
    }

    let temp_id = uuid::Uuid::new_v4().to_string();
    let tmp_path = format!("/tmp/labnas-duplex-{}", temp_id);
    let tmp_file = format!("{}/{}", tmp_path, file_name);

    tokio::fs::create_dir_all(&tmp_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tokio::fs::write(&tmp_file, &file_data)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let page_count = count_file_pages(&tmp_file).await;

    Ok(Json(DuplexPrepareResponse {
        temp_id,
        filename: file_name,
        page_count,
    }))
}

/// POST /api/printing/duplex/print — Ejecuta un paso de impresion (impares o pares)
pub async fn duplex_print_step(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DuplexPrintStepRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    validate_temp_id(&req.temp_id)?;
    validate_printer_name(&req.printer)?;

    // Buscar archivo en el directorio temporal
    let tmp_path = format!("/tmp/labnas-duplex-{}", req.temp_id);
    let dir = tokio::fs::read_dir(&tmp_path)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                "Archivo temporal no encontrado. Puede haber expirado.".to_string(),
            )
        })?;

    let mut dir = dir;
    let mut file_path = String::new();
    let mut file_name = String::new();
    while let Ok(Some(entry)) = dir.next_entry().await {
        if entry.path().is_file() {
            file_path = entry.path().to_string_lossy().to_string();
            file_name = entry.file_name().to_string_lossy().to_string();
            break;
        }
    }

    if file_path.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            "Archivo temporal no encontrado".to_string(),
        ));
    }

    // Validar page_set
    if req.page_set != "odd" && req.page_set != "even" {
        return Err((
            StatusCode::BAD_REQUEST,
            "page_set debe ser 'odd' o 'even'".to_string(),
        ));
    }

    // Construir opciones para lp
    let mut lp_options = req.options.clone();
    lp_options.insert("page-set".to_string(), req.page_set.clone());
    if let Some(ref order) = req.output_order {
        if order == "reverse" {
            lp_options.insert("outputorder".to_string(), "reverse".to_string());
        }
    }

    let copies_str = req.copies.map(|c| c.to_string());
    let result = run_lp_command(&req.printer, &file_path, copies_str.clone(), None, &lp_options).await;

    if result.is_ok() {
        let username = extract_session(&state, &headers)
            .await
            .map(|(u, _)| u)
            .unwrap_or_else(|| "unknown".to_string());

        let side = if req.page_set == "odd" { "impares" } else { "pares" };
        state
            .log_activity(
                "Impresion duplex",
                &format!("{} ({}) en {}", file_name, side, req.printer),
                &username,
            )
            .await;

        // Registrar stats: solo las paginas de este paso
        track_print_stats(
            &state,
            &req.printer,
            &file_path,
            &copies_str,
            &None, // sin rango de paginas especifico (page-set lo maneja CUPS)
            &lp_options,
            &username,
        )
        .await;
    }

    result
}

/// DELETE /api/printing/duplex/{temp_id} — Limpia archivos temporales
pub async fn duplex_cleanup(
    Path(temp_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    validate_temp_id(&temp_id)?;

    let tmp_path = format!("/tmp/labnas-duplex-{}", temp_id);
    let _ = tokio::fs::remove_dir_all(&tmp_path).await;

    Ok(StatusCode::NO_CONTENT)
}
