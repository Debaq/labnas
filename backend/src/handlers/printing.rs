use axum::{
    extract::{Multipart, Path},
    http::StatusCode,
    Json,
};
use std::collections::HashMap;
use tokio::process::Command;

use crate::models::printing::{CupsPrintJob, CupsPrinter, PrintFileRequest, PrinterOption};

// Formatos que CUPS imprime bien nativamente (sin conversion)
const PRINTABLE_EXTENSIONS: &[&str] = &[
    "pdf", "ps", "eps", "txt", "text", "log", "conf", "cfg", "sh", "py", "rs", "js", "ts",
    "json", "xml", "csv", "md", "c", "cpp", "h", "java", "rb", "pl", "png", "jpg", "jpeg",
    "gif", "tiff", "tif", "bmp", "svg",
];

fn is_printable_file(filename: &str) -> bool {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    PRINTABLE_EXTENSIONS.contains(&ext.as_str())
}

fn validate_printer_name(name: &str) -> Result<(), (StatusCode, String)> {
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Nombre de impresora invalido".to_string(),
        ));
    }
    Ok(())
}

pub async fn list_printers() -> Result<Json<Vec<CupsPrinter>>, (StatusCode, String)> {
    let names_output = Command::new("lpstat")
        .arg("-e")
        .env("LANG", "C")
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error ejecutando lpstat: {}. CUPS instalado?", e),
            )
        })?;

    let names_text = String::from_utf8_lossy(&names_output.stdout);
    let printer_names: Vec<String> = names_text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if printer_names.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let default_output = Command::new("lpstat")
        .arg("-d")
        .env("LANG", "C")
        .output()
        .await
        .ok();

    let default_printer = default_output
        .and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            text.split(':').nth(1).map(|s| s.trim().to_string())
        })
        .unwrap_or_default();

    let status_output = Command::new("lpstat")
        .arg("-p")
        .env("LANG", "C")
        .output()
        .await
        .ok();

    let status_text = status_output
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let mut printers = Vec::new();

    for name in &printer_names {
        let state = status_text
            .lines()
            .find(|line| line.contains(name.as_str()))
            .map(|line| {
                let lower = line.to_lowercase();
                if lower.contains("idle") {
                    "idle"
                } else if lower.contains("printing") {
                    "printing"
                } else if lower.contains("disabled") {
                    "disabled"
                } else {
                    "unknown"
                }
            })
            .unwrap_or("unknown")
            .to_string();

        let is_default = *name == default_printer;
        let description = name.replace('_', " ");

        printers.push(CupsPrinter {
            name: name.clone(),
            description,
            is_default,
            state,
        });
    }

    Ok(Json(printers))
}

// --- Printer options via lpoptions -p <name> -l ---

pub async fn printer_options(
    Path(name): Path<String>,
) -> Result<Json<Vec<PrinterOption>>, (StatusCode, String)> {
    validate_printer_name(&name)?;

    let output = Command::new("lpoptions")
        .args(["-p", &name, "-l"])
        .env("LANG", "C")
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error ejecutando lpoptions: {}", e),
            )
        })?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut options = Vec::new();

    // Format: "Key/Display Name: value1 *default value2 value3"
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Split "Key/Display Name: values..."
        let Some((key_part, values_part)) = line.split_once(':') else {
            continue;
        };

        let (key, display_name) = if let Some((k, d)) = key_part.split_once('/') {
            (k.trim().to_string(), d.trim().to_string())
        } else {
            let k = key_part.trim().to_string();
            (k.clone(), k)
        };

        let values_str = values_part.trim();
        let mut values = Vec::new();
        let mut default_value = String::new();

        for val in values_str.split_whitespace() {
            if let Some(stripped) = val.strip_prefix('*') {
                default_value = stripped.to_string();
                values.push(stripped.to_string());
            } else {
                values.push(val.to_string());
            }
        }

        if default_value.is_empty() && !values.is_empty() {
            default_value = values[0].clone();
        }

        // Skip options with only 1 value (not configurable)
        if values.len() <= 1 {
            continue;
        }

        options.push(PrinterOption {
            key,
            display_name,
            default_value,
            values,
        });
    }

    Ok(Json(options))
}

// --- Print upload ---

pub async fn print_upload(
    mut multipart: Multipart,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let mut printer: Option<String> = None;
    let mut copies: Option<String> = None;
    let mut pages: Option<String> = None;
    let mut lp_options: HashMap<String, String> = HashMap::new();
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
            continue;
        }

        let val = field
            .text()
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

        match name.as_str() {
            "printer" => printer = Some(val),
            "copies" => copies = Some(val),
            "pages" => pages = Some(val),
            other if other.starts_with("opt_") => {
                let key = other.strip_prefix("opt_").unwrap().to_string();
                if !val.is_empty() {
                    lp_options.insert(key, val);
                }
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

    if !is_printable_file(&file_name) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Formato no soportado: '{}'. Usa PDF, imagenes (PNG/JPG) o texto plano.",
                file_name
            ),
        ));
    }

    let tmp_path = format!("/tmp/labnas-print-{}", uuid::Uuid::new_v4());
    let tmp_file = format!("{}/{}", tmp_path, file_name);
    tokio::fs::create_dir_all(&tmp_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tokio::fs::write(&tmp_file, &file_data)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let result = run_lp_command(&printer_name, &tmp_file, copies, pages, &lp_options).await;

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

    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    if !is_printable_file(&filename) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Formato no soportado: '{}'. Usa PDF, imagenes (PNG/JPG) o texto plano.",
                filename
            ),
        ));
    }

    run_lp_command(
        &req.printer,
        &req.path,
        req.copies.map(|c| c.to_string()),
        req.pages.clone(),
        &req.options,
    )
    .await
}

async fn run_lp_command(
    printer: &str,
    file_path: &str,
    copies: Option<String>,
    pages: Option<String>,
    options: &HashMap<String, String>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    validate_printer_name(printer)?;

    let mut args = vec!["-d".to_string(), printer.to_string()];

    // Copies
    if let Some(n) = copies {
        if let Ok(num) = n.parse::<u32>() {
            if num > 0 && num <= 100 {
                args.push("-n".to_string());
                args.push(num.to_string());
            }
        }
    }

    // Page ranges
    if let Some(pg) = pages {
        let valid = pg
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == ',');
        if valid && !pg.is_empty() {
            args.push("-o".to_string());
            args.push(format!("page-ranges={}", pg));
        }
    }

    // All printer-specific options
    for (key, value) in options {
        // Validate key and value: only safe characters
        let safe = |s: &str| {
            s.chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        };
        if safe(key) && safe(value) {
            args.push("-o".to_string());
            args.push(format!("{}={}", key, value));
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

// --- Enable / Disable printer ---

pub async fn enable_printer(
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    validate_printer_name(&name)?;

    let output = Command::new("cupsenable")
        .arg(&name)
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error ejecutando cupsenable: {}", e),
            )
        })?;

    if output.status.success() {
        Ok(StatusCode::OK)
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", err)))
    }
}

pub async fn disable_printer(
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    validate_printer_name(&name)?;

    let output = Command::new("cupsdisable")
        .arg(&name)
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error ejecutando cupsdisable: {}", e),
            )
        })?;

    if output.status.success() {
        Ok(StatusCode::OK)
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", err)))
    }
}

pub async fn list_jobs() -> Result<Json<Vec<CupsPrintJob>>, (StatusCode, String)> {
    let output = Command::new("lpstat")
        .arg("-o")
        .env("LANG", "C")
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
        let parts: Vec<&str> = line.splitn(4, char::is_whitespace).collect();
        if parts.len() >= 3 {
            let job_id = parts[0].to_string();
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
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
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
