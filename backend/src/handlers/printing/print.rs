//! Imprimir (subida o archivo del NAS) y lp

use super::*;

// --- Print upload ---

pub async fn print_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
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

    let result = run_lp_command(&printer_name, &tmp_file, copies.clone(), pages.clone(), &lp_options).await;

    if result.is_ok() {
        let username = extract_session(&state, &headers)
            .await
            .map(|(u, _)| u)
            .unwrap_or_else(|| "unknown".to_string());
        state.log_activity("Impresion", &format!("{} en {}", file_name, printer_name), &username).await;
        track_print_stats(&state, &printer_name, &tmp_file, &copies, &pages, &lp_options, &username).await;
    }

    let _ = tokio::fs::remove_dir_all(&tmp_path).await;

    result
}

pub async fn print_file_path(
    State(state): State<AppState>,
    Json(req): Json<PrintFileRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let roots = crate::storage::load_roots(&state.db).await?;
    let path = crate::storage::resolve_existing(&roots, &req.path)?;
    let path_str = path.to_string_lossy().to_string();

    if path.is_dir() {
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
        &path_str,
        req.copies.map(|c| c.to_string()),
        req.pages.clone(),
        &req.options,
    )
    .await
}

/// Obtiene la URI del dispositivo CUPS para detectar si es de red
pub(super) async fn get_printer_uri(printer: &str) -> Option<String> {
    let output = Command::new("lpstat")
        .args(["-v", printer])
        .env("LANG", "C")
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // Format: "device for PrinterName: socket://192.168.1.50:9100"
    text.split_once(": ").map(|(_, uri)| uri.trim().to_string())
}

/// Extrae IP de una URI CUPS (socket://ip:port, ipp://ip/..., http://ip/...)
pub(super) fn extract_ip_from_uri(uri: &str) -> Option<String> {
    let stripped = uri
        .strip_prefix("socket://")
        .or_else(|| uri.strip_prefix("ipp://"))
        .or_else(|| uri.strip_prefix("ipps://"))
        .or_else(|| uri.strip_prefix("http://"))
        .or_else(|| uri.strip_prefix("https://"))
        .or_else(|| uri.strip_prefix("lpd://"))?;
    // IP is before : or /
    let ip = stripped.split(&[':', '/'][..]).next()?;
    // Validate it looks like an IP
    if ip.split('.').count() == 4 && ip.split('.').all(|p| p.parse::<u8>().is_ok()) {
        Some(ip.to_string())
    } else {
        None
    }
}

/// Intenta despertar una impresora de red conectandose a puertos comunes
pub(super) async fn wake_printer(uri: &str) {
    let Some(ip) = extract_ip_from_uri(uri) else { return };

    // Intentar conectar a puerto 9100 (JetDirect) o 80 (web) para despertar
    for port in [9100u16, 80, 443, 631] {
        let addr = format!("{}:{}", ip, port);
        if let Ok(Ok(_)) = tokio::time::timeout(
            Duration::from_millis(1500),
            tokio::net::TcpStream::connect(&addr),
        )
        .await
        {
            // Conexion exitosa = impresora despierta
            return;
        }
    }
}

/// Auto-habilita una impresora si esta deshabilitada y configura retry policy
pub(super) async fn ensure_printer_ready(printer: &str) {
    // Verificar estado
    let status = Command::new("lpstat")
        .args(["-p", printer])
        .env("LANG", "C")
        .output()
        .await;

    if let Ok(output) = status {
        let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
        if text.contains("disabled") {
            // Re-habilitar
            let _ = Command::new("cupsenable").arg(printer).output().await;
        }
    }

    // Intentar despertar si es de red
    if let Some(uri) = get_printer_uri(printer).await {
        if !uri.starts_with("usb://") {
            wake_printer(&uri).await;
        }
    }

    // Asegurar que la politica de error sea retry-job (no stop-printer)
    let _ = Command::new("lpadmin")
        .args(["-p", printer, "-o", "printer-error-policy=retry-job"])
        .output()
        .await;
}

pub(super) async fn run_lp_command(
    printer: &str,
    file_path: &str,
    copies: Option<String>,
    pages: Option<String>,
    options: &HashMap<String, String>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    validate_printer_name(printer)?;

    // Auto-enable, wake, and set retry policy
    ensure_printer_ready(printer).await;

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

    // Boolean CUPS options (passed as -o key, not -o key=value)
    const BOOLEAN_LP_OPTIONS: &[&str] = &["fit-to-page"];

    // All printer-specific options
    for (key, value) in options {
        // Validate key and value: only safe characters
        let safe = |s: &str| {
            s.chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        };
        if safe(key) {
            if BOOLEAN_LP_OPTIONS.contains(&key.as_str()) {
                if value == "true" {
                    args.push("-o".to_string());
                    args.push(key.clone());
                }
            } else if safe(value) {
                args.push("-o".to_string());
                args.push(format!("{}={}", key, value));
            }
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
