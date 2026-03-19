use axum::{
    extract::{Multipart, Path},
    http::StatusCode,
    Json,
};
use tokio::process::Command;

use crate::models::printing::{CupsPrintJob, CupsPrinter, PrintFileRequest};

pub async fn list_printers() -> Result<Json<Vec<CupsPrinter>>, (StatusCode, String)> {
    // Get printer list
    let output = Command::new("lpstat")
        .args(["-p"])
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error ejecutando lpstat: {}. CUPS instalado?", e),
            )
        })?;

    let printers_text = String::from_utf8_lossy(&output.stdout).to_string();

    // Get default printer
    let default_output = Command::new("lpstat")
        .args(["-d"])
        .output()
        .await
        .ok();

    let default_text = default_output
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let default_printer = default_text
        .strip_prefix("system default destination: ")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let mut printers = Vec::new();

    for line in printers_text.lines() {
        // Format: "printer NAME is STATE. ..."
        if let Some(rest) = line.strip_prefix("printer ") {
            let parts: Vec<&str> = rest.splitn(2, " is ").collect();
            if parts.len() == 2 {
                let name = parts[0].trim().to_string();
                let state_part = parts[1].trim();
                let state = if state_part.contains("idle") {
                    "idle".to_string()
                } else if state_part.contains("printing") {
                    "printing".to_string()
                } else if state_part.contains("disabled") {
                    "disabled".to_string()
                } else {
                    "unknown".to_string()
                };

                let is_default = name == default_printer;

                // Try to get description via lpoptions
                let desc_output = Command::new("lpoptions")
                    .args(["-p", &name, "-l"])
                    .output()
                    .await;
                let description = desc_output
                    .ok()
                    .map(|_| name.replace('_', " "))
                    .unwrap_or_else(|| name.replace('_', " "));

                printers.push(CupsPrinter {
                    name,
                    description,
                    is_default,
                    state,
                });
            }
        }
    }

    Ok(Json(printers))
}

pub async fn print_upload(
    mut multipart: Multipart,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let mut printer: Option<String> = None;
    let mut copies: Option<String> = None;
    let mut orientation: Option<String> = None;
    let mut double_sided: Option<String> = None;
    let mut pages: Option<String> = None;
    let mut file_name = String::new();
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "printer" => {
                printer = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
                )
            }
            "copies" => {
                copies = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
                )
            }
            "orientation" => {
                orientation = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
                )
            }
            "double_sided" => {
                double_sided = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
                )
            }
            "pages" => {
                pages = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
                )
            }
            "file" => {
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
            _ => {}
        }
    }

    let printer_name = printer.ok_or((
        StatusCode::BAD_REQUEST,
        "Impresora no especificada".to_string(),
    ))?;
    let file_data = file_data.ok_or((
        StatusCode::BAD_REQUEST,
        "No se proporcionó archivo".to_string(),
    ))?;

    // Save to temp file
    let tmp_path = format!("/tmp/labnas-print-{}", uuid::Uuid::new_v4());
    let tmp_file = format!("{}/{}", tmp_path, file_name);
    tokio::fs::create_dir_all(&tmp_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tokio::fs::write(&tmp_file, &file_data)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let result = run_lp_command(&printer_name, &tmp_file, copies, orientation, double_sided, pages).await;

    // Cleanup temp
    let _ = tokio::fs::remove_dir_all(&tmp_path).await;

    result
}

pub async fn print_file_path(
    Json(req): Json<PrintFileRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let path = std::path::PathBuf::from(&req.path);

    if !path.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "La ruta debe ser absoluta".to_string(),
        ));
    }

    if !path.exists() || path.is_dir() {
        return Err((
            StatusCode::NOT_FOUND,
            "Archivo no encontrado".to_string(),
        ));
    }

    run_lp_command(
        &req.printer,
        &req.path,
        req.copies.map(|c| c.to_string()),
        req.orientation.clone(),
        req.double_sided.map(|b| b.to_string()),
        req.pages.clone(),
    )
    .await
}

async fn run_lp_command(
    printer: &str,
    file_path: &str,
    copies: Option<String>,
    orientation: Option<String>,
    double_sided: Option<String>,
    pages: Option<String>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    // Validate printer name (prevent injection)
    if !printer.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err((
            StatusCode::BAD_REQUEST,
            "Nombre de impresora invalido".to_string(),
        ));
    }

    let mut args = vec!["-d".to_string(), printer.to_string()];

    if let Some(n) = copies {
        if let Ok(num) = n.parse::<u32>() {
            if num > 0 && num <= 100 {
                args.push("-n".to_string());
                args.push(num.to_string());
            }
        }
    }

    if let Some(orient) = orientation {
        if orient == "landscape" {
            args.push("-o".to_string());
            args.push("landscape".to_string());
        }
    }

    if let Some(ds) = double_sided {
        if ds == "true" {
            args.push("-o".to_string());
            args.push("sides=two-sided-long-edge".to_string());
        }
    }

    if let Some(pg) = pages {
        // Validate page range format (e.g., "1-5", "1,3,5", "1-3,7")
        let valid = pg.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ',');
        if valid && !pg.is_empty() {
            args.push("-o".to_string());
            args.push(format!("page-ranges={}", pg));
        }
    }

    args.push(file_path.to_string());

    let output = Command::new("lp")
        .args(&args)
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error ejecutando lp: {}", e),
            )
        })?;

    if output.status.success() {
        let msg = String::from_utf8_lossy(&output.stdout).to_string();
        Ok((StatusCode::OK, msg))
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error al imprimir: {}", err),
        ))
    }
}

pub async fn list_jobs() -> Result<Json<Vec<CupsPrintJob>>, (StatusCode, String)> {
    let output = Command::new("lpstat")
        .args(["-o"])
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error ejecutando lpstat: {}", e),
            )
        })?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut jobs = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Format: "PRINTER-ID USER SIZE DATE"
        let parts: Vec<&str> = line.splitn(4, char::is_whitespace).collect();
        if parts.len() >= 3 {
            let job_id = parts[0].to_string();
            // Extract printer name from job ID (e.g., "HP_LaserJet-42" -> "HP_LaserJet")
            let printer = job_id
                .rfind('-')
                .map(|i| job_id[..i].to_string())
                .unwrap_or_else(|| job_id.clone());

            jobs.push(CupsPrintJob {
                id: job_id,
                printer,
                title: parts.get(1).unwrap_or(&"").to_string(),
                state: "pending".to_string(),
                size: parts.get(2).map(|s| s.to_string()),
            });
        }
    }

    Ok(Json(jobs))
}

pub async fn cancel_job(Path(id): Path<String>) -> Result<StatusCode, (StatusCode, String)> {
    // Validate job ID format
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err((
            StatusCode::BAD_REQUEST,
            "ID de trabajo invalido".to_string(),
        ));
    }

    let output = Command::new("cancel")
        .arg(&id)
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error ejecutando cancel: {}", e),
            )
        })?;

    if output.status.success() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error cancelando trabajo: {}", err),
        ))
    }
}
