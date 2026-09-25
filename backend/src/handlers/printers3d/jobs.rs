//! Trabajos: subir gcode, controlar impresion, archivos de la impresora

use super::*;

// =====================
// Upload gcode
// =====================

pub async fn upload_gcode(
    State(state): State<AppState>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;

    let mut file_name = String::new();
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        if field.name() == Some("file") {
            file_name = field
                .file_name()
                .unwrap_or("upload.gcode")
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
        "No se proporcionó archivo".to_string(),
    ))?;

    let client = &state.http_client;
    let base = printer_base(&printer);

    match printer.printer_type {
        Printer3DType::OctoPrint => {
            let part = reqwest::multipart::Part::bytes(file_data)
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            let form = reqwest::multipart::Form::new()
                .part("file", part)
                .text("select", "false")
                .text("print", "false");

            let mut req = client
                .post(format!("{}/api/files/local", base))
                .multipart(form)
                .timeout(Duration::from_secs(120));

            if let Some(key) = &printer.api_key {
                req = req.header("X-Api-Key", key);
            }

            let resp = req.send().await.map_err(|e| {
                (
                    StatusCode::BAD_GATEWAY,
                    format!("Error enviando a OctoPrint: {}", e),
                )
            })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((
                    StatusCode::BAD_GATEWAY,
                    format!("OctoPrint respondio {}: {}", status, body),
                ));
            }
        }
        Printer3DType::Moonraker => {
            let part = reqwest::multipart::Part::bytes(file_data)
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            let form = reqwest::multipart::Form::new().part("file", part);

            let resp = client
                .post(format!("{}/server/files/upload", base))
                .multipart(form)
                .timeout(Duration::from_secs(120))
                .send()
                .await
                .map_err(|e| {
                    (
                        StatusCode::BAD_GATEWAY,
                        format!("Error enviando a Moonraker: {}", e),
                    )
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((
                    StatusCode::BAD_GATEWAY,
                    format!("Moonraker respondio {}: {}", status, body),
                ));
            }
        }
        Printer3DType::CrealityStock => {
            let part = reqwest::multipart::Part::bytes(file_data)
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            let form = reqwest::multipart::Form::new().part("file", part);

            let resp = client
                .post(format!("http://{}:80/upload/", printer.ip))
                .multipart(form)
                .timeout(Duration::from_secs(120))
                .send()
                .await
                .map_err(|e| {
                    (
                        StatusCode::BAD_GATEWAY,
                        format!("Error enviando a Creality: {}", e),
                    )
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((
                    StatusCode::BAD_GATEWAY,
                    format!("Creality respondio {}: {}", status, body),
                ));
            }
        }
        Printer3DType::FlashForge => {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::TcpStream;

            let mut stream = tokio::time::timeout(
                Duration::from_secs(5),
                TcpStream::connect(format!("{}:8899", printer.ip)),
            )
            .await
            .map_err(|_| (StatusCode::BAD_GATEWAY, "Timeout conectando FlashForge".to_string()))?
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error TCP: {}", e)))?;

            stream.write_all(b"~M601 S1\r\n").await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
            tokio::time::sleep(Duration::from_millis(300)).await;
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await;

            let upload_cmd = format!(
                "~M28 {} 0:/usr/data/gcodes/{}\r\n",
                file_data.len(),
                file_name
            );
            stream.write_all(upload_cmd.as_bytes()).await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = stream.read(&mut buf).await;

            stream.write_all(&file_data).await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

            stream.write_all(b"~M29\r\n").await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = stream.read(&mut buf).await;

            let _ = stream.write_all(b"~M602\r\n").await;
        }
    }

    state
        .log_activity(
            "Impresoras 3D",
            &format!("Archivo '{}' subido a {}", file_name, printer.name),
            "sistema",
        )
        .await;

    Ok((
        StatusCode::OK,
        format!("Archivo '{}' subido correctamente", file_name),
    ))
}


// =====================
// Control de impresión
// =====================

pub async fn control_print(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ControlPrintRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    match printer.printer_type {
        Printer3DType::OctoPrint => {
            // OctoPrint usa POST /api/job con el comando
            let octo_cmd = match req.command.as_str() {
                "start" => "start",
                "pause" => "pause",
                "resume" => "pause", // OctoPrint toggle pause
                "cancel" => "cancel",
                _ => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("Comando no soportado: {}", req.command),
                    ))
                }
            };

            let mut body = serde_json::json!({"command": octo_cmd});
            // OctoPrint usa "action" para toggle pause/resume
            if req.command == "resume" {
                body = serde_json::json!({"command": "pause", "action": "resume"});
            } else if req.command == "pause" {
                body = serde_json::json!({"command": "pause", "action": "pause"});
            }

            let resp = octoprint_request(
                client,
                reqwest::Method::POST,
                &format!("{}/api/job", base),
                &printer,
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;

            if !resp.status().is_success() {
                let st = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((StatusCode::BAD_GATEWAY, format!("OctoPrint {}: {}", st, body)));
            }
        }
        Printer3DType::Moonraker => {
            let endpoint = match req.command.as_str() {
                "start" => "/printer/print/start",
                "pause" => "/printer/print/pause",
                "resume" => "/printer/print/resume",
                "cancel" => "/printer/print/cancel",
                _ => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("Comando no soportado: {}", req.command),
                    ))
                }
            };

            let resp = client
                .post(format!("{}{}", base, endpoint))
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;

            if !resp.status().is_success() {
                let st = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((StatusCode::BAD_GATEWAY, format!("Moonraker {}: {}", st, body)));
            }
        }
        Printer3DType::CrealityStock => {
            let params = match req.command.as_str() {
                "pause" => serde_json::json!({"pause": 1}),
                "resume" => serde_json::json!({"pause": 0}),
                "cancel" => serde_json::json!({"stop": 1}),
                "start" => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "Use imprimir archivo para iniciar en Creality".to_string(),
                    ))
                }
                _ => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("Comando no soportado: {}", req.command),
                    ))
                }
            };
            creality_ws_command(
                &printer.ip,
                serde_json::json!({"method": "set", "params": params}),
            )
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
        Printer3DType::FlashForge => {
            let cmd = match req.command.as_str() {
                "start" | "resume" => "M24",
                "pause" => "M25",
                "cancel" => "M26",
                _ => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("Comando no soportado: {}", req.command),
                    ))
                }
            };
            flashforge_command(&printer.ip, cmd)
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
    }

    state
        .log_activity(
            "Impresoras 3D",
            &format!("{}: {}", printer.name, req.command),
            "sistema",
        )
        .await;

    Ok((StatusCode::OK, format!("Comando '{}' enviado", req.command)))
}


// =====================
// Archivos en la impresora
// =====================

pub async fn list_printer_files(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<PrinterFileInfo>>, (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    let files = match printer.printer_type {
        Printer3DType::OctoPrint => {
            let resp = octoprint_request(
                client,
                reqwest::Method::GET,
                &format!("{}/api/files", base),
                &printer,
            )
            .send()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;

            if !resp.status().is_success() {
                return Err((StatusCode::BAD_GATEWAY, "Error obteniendo archivos".to_string()));
            }

            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error JSON: {}", e)))?;

            json["files"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .filter(|f| f["type"].as_str() == Some("machinecode"))
                .map(|f| PrinterFileInfo {
                    name: f["name"].as_str().unwrap_or("").to_string(),
                    size: f["size"].as_u64(),
                    date: f["date"].as_u64(),
                })
                .collect()
        }
        Printer3DType::Moonraker => {
            let resp = client
                .get(format!("{}/server/files/list", base))
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;

            if !resp.status().is_success() {
                return Err((StatusCode::BAD_GATEWAY, "Error obteniendo archivos".to_string()));
            }

            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error JSON: {}", e)))?;

            json["result"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .map(|f| PrinterFileInfo {
                    name: f["filename"]
                        .as_str()
                        .or_else(|| f["path"].as_str())
                        .unwrap_or("")
                        .to_string(),
                    size: f["size"].as_u64(),
                    date: f["modified"].as_f64().map(|v| v as u64),
                })
                .collect()
        }
        Printer3DType::CrealityStock => {
            match creality_ws_command(
                &printer.ip,
                serde_json::json!({"method": "get", "params": {"reqGcodeFile": 1}}),
            )
            .await
            {
                Ok(data) => data["gcodeFiles"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .map(|f| PrinterFileInfo {
                        name: f["name"].as_str().unwrap_or("").to_string(),
                        size: f["size"].as_u64(),
                        date: f["date"].as_u64(),
                    })
                    .collect(),
                Err(_) => Vec::new(),
            }
        }
        Printer3DType::FlashForge => {
            match flashforge_command(&printer.ip, "M661").await {
                Ok(response) => response
                    .split("::")
                    .filter_map(|part| {
                        if let Some(pos) = part.find("/usr/data/gcodes/") {
                            let path = &part[pos..];
                            let clean =
                                path.trim_matches(|c: char| c.is_control() || c == '\0');
                            if clean.ends_with(".gcode")
                                || clean.ends_with(".gx")
                                || clean.ends_with(".3mf")
                            {
                                let name =
                                    clean.rsplit('/').next().unwrap_or(clean);
                                return Some(PrinterFileInfo {
                                    name: name.to_string(),
                                    size: None,
                                    date: None,
                                });
                            }
                        }
                        None
                    })
                    .collect(),
                Err(_) => Vec::new(),
            }
        }
    };

    Ok(Json(files))
}

pub async fn print_file(
    State(state): State<AppState>,
    Path((id, filename)): Path<(String, String)>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    match printer.printer_type {
        Printer3DType::OctoPrint => {
            let resp = octoprint_request(
                client,
                reqwest::Method::POST,
                &format!("{}/api/files/local/{}", base, filename),
                &printer,
            )
            .json(&serde_json::json!({"command": "select", "print": true}))
            .send()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;

            if !resp.status().is_success() {
                let st = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((StatusCode::BAD_GATEWAY, format!("OctoPrint {}: {}", st, body)));
            }
        }
        Printer3DType::Moonraker => {
            let resp = client
                .post(format!("{}/printer/print/start", base))
                .json(&serde_json::json!({"filename": filename}))
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;

            if !resp.status().is_success() {
                let st = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((StatusCode::BAD_GATEWAY, format!("Moonraker {}: {}", st, body)));
            }
        }
        Printer3DType::CrealityStock => {
            creality_ws_command(
                &printer.ip,
                serde_json::json!({
                    "method": "set",
                    "params": {"opGcodeFile": format!("printprt:/usr/data/{}", filename)}
                }),
            )
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
        Printer3DType::FlashForge => {
            let select_cmd = format!("M23 0:/usr/data/gcodes/{}", filename);
            flashforge_commands(&printer.ip, &[&select_cmd, "M24"])
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
    }

    state
        .log_activity(
            "Impresoras 3D",
            &format!("Imprimiendo '{}' en {}", filename, printer.name),
            "sistema",
        )
        .await;

    Ok((
        StatusCode::OK,
        format!("Imprimiendo '{}'", filename),
    ))
}

pub async fn delete_printer_file(
    State(state): State<AppState>,
    Path((id, filename)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    match printer.printer_type {
        Printer3DType::OctoPrint => {
            let resp = octoprint_request(
                client,
                reqwest::Method::DELETE,
                &format!("{}/api/files/local/{}", base, filename),
                &printer,
            )
            .send()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;

            if !resp.status().is_success() {
                let st = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((StatusCode::BAD_GATEWAY, format!("OctoPrint {}: {}", st, body)));
            }
        }
        Printer3DType::Moonraker => {
            let resp = client
                .delete(format!("{}/server/files/gcodes/{}", base, filename))
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;

            if !resp.status().is_success() {
                let st = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((StatusCode::BAD_GATEWAY, format!("Moonraker {}: {}", st, body)));
            }
        }
        Printer3DType::CrealityStock => {
            creality_ws_command(
                &printer.ip,
                serde_json::json!({
                    "method": "set",
                    "params": {"opGcodeFile": format!("deleteprt:/usr/data/{}", filename)}
                }),
            )
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
        Printer3DType::FlashForge => {
            return Err((
                StatusCode::NOT_IMPLEMENTED,
                "Eliminar archivos no soportado en FlashForge stock".to_string(),
            ));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}
