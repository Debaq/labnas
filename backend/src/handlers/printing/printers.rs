//! Impresoras CUPS: listado, opciones, habilitar, despertar, trabajos

use super::*;

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

/// POST /api/printing/printers/{name}/wake - Despertar impresora y re-habilitarla
pub async fn wake_printer_endpoint(
    Path(name): Path<String>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    validate_printer_name(&name)?;
    ensure_printer_ready(&name).await;
    Ok((StatusCode::OK, "Impresora despertada y habilitada".to_string()))
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

// ── Estadísticas y costos por impresora ──
