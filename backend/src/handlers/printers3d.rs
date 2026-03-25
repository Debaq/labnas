use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::time::Duration;

use crate::config::save_config;
use crate::models::printers3d::*;
use crate::state::AppState;

// =====================
// Helpers
// =====================

// =====================
// Creality Stock (WebSocket en puerto 9999)
// =====================

pub async fn creality_ws_command(
    ip: &str,
    msg: serde_json::Value,
) -> Result<serde_json::Value, String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    let url = format!("ws://{}:9999", ip);
    let (mut ws, _) = tokio::time::timeout(
        Duration::from_secs(5),
        connect_async(&url),
    )
    .await
    .map_err(|_| "Timeout conectando WebSocket Creality".to_string())?
    .map_err(|e| format!("Error WebSocket Creality: {}", e))?;

    ws.send(Message::Text(msg.to_string().into()))
        .await
        .map_err(|e| format!("Error enviando WS: {}", e))?;

    let response = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(Ok(msg)) = ws.next().await {
            if let Message::Text(text) = msg {
                let text_str: String = text.into();
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text_str) {
                    return Ok(json);
                }
            }
        }
        Err("Sin respuesta del WebSocket Creality".to_string())
    })
    .await
    .map_err(|_| "Timeout esperando respuesta WS Creality".to_string())?;

    let _ = ws.close(None).await;
    response
}

async fn fetch_creality_status(
    ip: &str,
    printer: &Printer3DConfig,
) -> Result<Json<Printer3DStatus>, (StatusCode, String)> {
    let msg = serde_json::json!({"method": "get", "params": {"ReqPrinterPara": 1}});
    let data = match creality_ws_command(ip, msg).await {
        Ok(d) => d,
        Err(_) => {
            return Ok(Json(Printer3DStatus {
                id: printer.id.clone(),
                online: false,
                temperatures: None,
                current_job: None,
            }));
        }
    };

    let temperatures = Some(PrinterTemps {
        hotend_actual: data["nozzleTemp"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| data["nozzleTemp"].as_f64())
            .unwrap_or(0.0),
        hotend_target: data["targetNozzleTemp"]
            .as_f64()
            .unwrap_or(0.0),
        bed_actual: data["bedTemp0"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| data["bedTemp0"].as_f64())
            .unwrap_or(0.0),
        bed_target: data["targetBedTemp0"]
            .as_f64()
            .unwrap_or(0.0),
    });

    let file_name = data["printFileName"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let state_code = data["state"].as_i64().unwrap_or(0);
    let state_str = match state_code {
        1 => "printing",
        2 => "paused",
        _ => "standby",
    }
    .to_string();

    let current_job = if !file_name.is_empty() && state_code > 0 {
        Some(PrintJob {
            file_name,
            progress: data["printProgress"].as_f64().unwrap_or(0.0),
            time_elapsed: data["printJobTime"].as_u64(),
            time_remaining: data["printLeftTime"].as_u64(),
            state: state_str,
        })
    } else {
        None
    };

    Ok(Json(Printer3DStatus {
        id: printer.id.clone(),
        online: true,
        temperatures,
        current_job,
    }))
}

// =====================
// FlashForge (TCP socket en puerto 8899)
// =====================

pub async fn flashforge_command(ip: &str, command: &str) -> Result<String, String> {
    flashforge_commands(ip, &[command])
        .await
        .map(|mut v| v.pop().unwrap_or_default())
}

pub async fn flashforge_commands(ip: &str, commands: &[&str]) -> Result<Vec<String>, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect(format!("{}:8899", ip)),
    )
    .await
    .map_err(|_| "Timeout conectando TCP FlashForge".to_string())?
    .map_err(|e| format!("Error TCP FlashForge: {}", e))?;

    // Open session
    stream
        .write_all(b"~M601 S1\r\n")
        .await
        .map_err(|e| format!("Error M601: {}", e))?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut buf = vec![0u8; 4096];
    let _ = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await;

    let mut responses = Vec::new();
    for cmd in commands {
        stream
            .write_all(format!("~{}\r\n", cmd).as_bytes())
            .await
            .map_err(|e| format!("Error enviando {}: {}", cmd, e))?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let mut resp = vec![0u8; 8192];
        match tokio::time::timeout(Duration::from_secs(3), stream.read(&mut resp)).await {
            Ok(Ok(n)) => {
                responses.push(String::from_utf8_lossy(&resp[..n]).to_string());
            }
            _ => {
                responses.push(String::new());
            }
        }
    }

    let _ = stream.write_all(b"~M602\r\n").await;
    Ok(responses)
}

fn parse_flashforge_temps(response: &str) -> PrinterTemps {
    let mut hotend_actual = 0.0;
    let mut hotend_target = 0.0;
    let mut bed_actual = 0.0;
    let mut bed_target = 0.0;

    for part in response.split_whitespace() {
        if let Some(temps) = part.strip_prefix("T0:") {
            let parts: Vec<&str> = temps.split('/').collect();
            if parts.len() == 2 {
                hotend_actual = parts[0].parse().unwrap_or(0.0);
                hotend_target = parts[1].parse().unwrap_or(0.0);
            }
        } else if let Some(temps) = part.strip_prefix("B:") {
            let parts: Vec<&str> = temps.split('/').collect();
            if parts.len() == 2 {
                bed_actual = parts[0].parse().unwrap_or(0.0);
                bed_target = parts[1].parse().unwrap_or(0.0);
            }
        }
    }

    PrinterTemps {
        hotend_actual,
        hotend_target,
        bed_actual,
        bed_target,
    }
}

async fn fetch_flashforge_status(
    ip: &str,
    printer: &Printer3DConfig,
) -> Result<Json<Printer3DStatus>, (StatusCode, String)> {
    let responses = match flashforge_commands(ip, &["M105", "M119", "M27"]).await {
        Ok(r) if r.len() == 3 => r,
        _ => {
            return Ok(Json(Printer3DStatus {
                id: printer.id.clone(),
                online: false,
                temperatures: None,
                current_job: None,
            }));
        }
    };

    let temperatures = parse_flashforge_temps(&responses[0]);

    let mut machine_status = "READY".to_string();
    let mut current_file = String::new();
    for line in responses[1].lines() {
        if let Some(status) = line.strip_prefix("MachineStatus:") {
            machine_status = status.trim().to_string();
        }
        if let Some(file) = line.strip_prefix("CurrentFile:") {
            let f = file.trim();
            if !f.is_empty() {
                current_file = f.to_string();
            }
        }
    }

    let mut progress_current: u64 = 0;
    let mut progress_total: u64 = 0;
    for line in responses[2].lines() {
        if line.contains("SD printing byte") {
            if let Some(bytes_part) = line.split("byte ").nth(1) {
                let parts: Vec<&str> = bytes_part.split('/').collect();
                if parts.len() == 2 {
                    progress_current = parts[0].trim().parse().unwrap_or(0);
                    progress_total = parts[1].trim().parse().unwrap_or(0);
                }
            }
        }
    }

    let is_printing = machine_status.contains("BUILDING") || machine_status == "PAUSED";
    let state_str = if machine_status.contains("BUILDING") {
        "printing".to_string()
    } else if machine_status == "PAUSED" {
        "paused".to_string()
    } else {
        "standby".to_string()
    };

    let current_job = if is_printing {
        let progress = if progress_total > 0 {
            (progress_current as f64 / progress_total as f64) * 100.0
        } else {
            0.0
        };
        Some(PrintJob {
            file_name: current_file,
            progress,
            time_elapsed: None,
            time_remaining: None,
            state: state_str,
        })
    } else {
        None
    };

    Ok(Json(Printer3DStatus {
        id: printer.id.clone(),
        online: true,
        temperatures: Some(temperatures),
        current_job,
    }))
}

/// Busca una impresora por ID en la config y devuelve una copia
async fn find_printer(
    state: &AppState,
    id: &str,
) -> Result<Printer3DConfig, (StatusCode, String)> {
    let config = state.config.lock().await;
    config
        .printers3d
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()))
}

/// Construye la URL base de la impresora
fn printer_base(printer: &Printer3DConfig) -> String {
    format!("http://{}:{}", printer.ip, printer.port)
}

/// Crea un request builder con la API key de OctoPrint si existe
fn octoprint_request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    printer: &Printer3DConfig,
) -> reqwest::RequestBuilder {
    let mut req = client.request(method, url).timeout(Duration::from_secs(10));
    if let Some(key) = &printer.api_key {
        req = req.header("X-Api-Key", key);
    }
    req
}

// =====================
// CRUD básico
// =====================

pub async fn list_printers(State(state): State<AppState>) -> Json<Vec<Printer3DConfig>> {
    let config = state.config.lock().await;
    Json(config.printers3d.clone())
}

pub async fn add_printer(
    State(state): State<AppState>,
    Json(req): Json<AddPrinter3DRequest>,
) -> Result<(StatusCode, Json<Printer3DConfig>), (StatusCode, String)> {
    let printer = Printer3DConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        ip: req.ip,
        port: req.port,
        printer_type: req.printer_type,
        api_key: req.api_key,
        camera_url: req.camera_url,
    };

    let mut config = state.config.lock().await;
    config.printers3d.push(printer.clone());
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    state
        .log_activity("Impresoras 3D", &format!("Agregada: {}", printer.name), "sistema")
        .await;

    Ok((StatusCode::CREATED, Json(printer)))
}

pub async fn delete_printer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.printers3d.len();
    let name = config
        .printers3d
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone());
    config.printers3d.retain(|p| p.id != id);

    if config.printers3d.len() == before {
        return Err((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()));
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if let Some(name) = name {
        drop(config);
        state
            .log_activity("Impresoras 3D", &format!("Eliminada: {}", name), "sistema")
            .await;
    }

    Ok(StatusCode::NO_CONTENT)
}

// =====================
// Status
// =====================

pub async fn printer_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Printer3DStatus>, (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    match printer.printer_type {
        Printer3DType::OctoPrint => fetch_octoprint_status(client, &base, &printer).await,
        Printer3DType::Moonraker => fetch_moonraker_status(client, &base, &printer).await,
        Printer3DType::CrealityStock => fetch_creality_status(&printer.ip, &printer).await,
        Printer3DType::FlashForge => fetch_flashforge_status(&printer.ip, &printer).await,
    }
}

async fn fetch_octoprint_status(
    client: &reqwest::Client,
    base: &str,
    printer: &Printer3DConfig,
) -> Result<Json<Printer3DStatus>, (StatusCode, String)> {
    let req = octoprint_request(
        client,
        reqwest::Method::GET,
        &format!("{}/api/printer", base),
        printer,
    );

    let printer_resp = req.send().await;
    let online = printer_resp
        .as_ref()
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    if !online {
        return Ok(Json(Printer3DStatus {
            id: printer.id.clone(),
            online: false,
            temperatures: None,
            current_job: None,
        }));
    }

    let temperatures = if let Ok(resp) = printer_resp {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            let temp = &json["temperature"];
            Some(PrinterTemps {
                hotend_actual: temp["tool0"]["actual"].as_f64().unwrap_or(0.0),
                hotend_target: temp["tool0"]["target"].as_f64().unwrap_or(0.0),
                bed_actual: temp["bed"]["actual"].as_f64().unwrap_or(0.0),
                bed_target: temp["bed"]["target"].as_f64().unwrap_or(0.0),
            })
        } else {
            None
        }
    } else {
        None
    };

    let job_req = octoprint_request(
        client,
        reqwest::Method::GET,
        &format!("{}/api/job", base),
        printer,
    );

    let current_job = if let Ok(resp) = job_req.send().await {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            let state_str = json["state"].as_str().unwrap_or("Unknown").to_string();
            let file_name = json["job"]["file"]["name"]
                .as_str()
                .unwrap_or("")
                .to_string();
            if !file_name.is_empty() {
                Some(PrintJob {
                    file_name,
                    progress: json["progress"]["completion"].as_f64().unwrap_or(0.0),
                    time_elapsed: json["progress"]["printTime"].as_u64(),
                    time_remaining: json["progress"]["printTimeLeft"].as_u64(),
                    state: state_str,
                })
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(Json(Printer3DStatus {
        id: printer.id.clone(),
        online: true,
        temperatures,
        current_job,
    }))
}

async fn fetch_moonraker_status(
    client: &reqwest::Client,
    base: &str,
    printer: &Printer3DConfig,
) -> Result<Json<Printer3DStatus>, (StatusCode, String)> {
    let info_resp = client
        .get(format!("{}/printer/info", base))
        .timeout(Duration::from_secs(5))
        .send()
        .await;

    let online = info_resp
        .as_ref()
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    if !online {
        return Ok(Json(Printer3DStatus {
            id: printer.id.clone(),
            online: false,
            temperatures: None,
            current_job: None,
        }));
    }

    let temperatures = if let Ok(resp) = client
        .get(format!(
            "{}/printer/objects/query?heater_bed&extruder",
            base
        ))
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            let status = &json["result"]["status"];
            Some(PrinterTemps {
                hotend_actual: status["extruder"]["temperature"]
                    .as_f64()
                    .unwrap_or(0.0),
                hotend_target: status["extruder"]["target"].as_f64().unwrap_or(0.0),
                bed_actual: status["heater_bed"]["temperature"]
                    .as_f64()
                    .unwrap_or(0.0),
                bed_target: status["heater_bed"]["target"].as_f64().unwrap_or(0.0),
            })
        } else {
            None
        }
    } else {
        None
    };

    let current_job = if let Ok(resp) = client
        .get(format!(
            "{}/printer/objects/query?print_stats&virtual_sdcard",
            base
        ))
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            let status = &json["result"]["status"];
            let file_name = status["print_stats"]["filename"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let state_str = status["print_stats"]["state"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            if !file_name.is_empty() {
                let progress = status["virtual_sdcard"]["progress"]
                    .as_f64()
                    .unwrap_or(0.0)
                    * 100.0;
                Some(PrintJob {
                    file_name,
                    progress,
                    time_elapsed: status["print_stats"]["total_duration"].as_u64(),
                    time_remaining: None,
                    state: state_str,
                })
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(Json(Printer3DStatus {
        id: printer.id.clone(),
        online: true,
        temperatures,
        current_job,
    }))
}

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
// Detect printers
// =====================

pub async fn detect_printers(
    State(state): State<AppState>,
) -> Result<Json<Vec<DetectPrintersResult>>, (StatusCode, String)> {
    let hosts = state.scanned_hosts.lock().await;
    let alive_ips: Vec<String> = hosts
        .iter()
        .filter(|h| h.is_alive)
        .map(|h| h.ip.clone())
        .collect();
    drop(hosts);

    if alive_ips.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let client = state.http_client.clone();
    let mut handles = Vec::new();

    for ip in alive_ips {
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let mut found = Vec::new();

            // OctoPrint en puertos 80 y 5000
            for port in [80u16, 5000] {
                if let Ok(resp) = client
                    .get(format!("http://{}:{}/api/version", ip, port))
                    .timeout(Duration::from_secs(3))
                    .send()
                    .await
                {
                    if resp.status().is_success() || resp.status().as_u16() == 403 {
                        let name = if resp.status().is_success() {
                            resp.json::<serde_json::Value>()
                                .await
                                .ok()
                                .and_then(|v| v["text"].as_str().map(|s| s.to_string()))
                        } else {
                            None
                        };
                        found.push(DetectPrintersResult {
                            ip: ip.clone(),
                            port,
                            printer_type: Printer3DType::OctoPrint,
                            name,
                        });
                    }
                }
            }

            // Moonraker en puerto 7125
            if let Ok(resp) = client
                .get(format!("http://{}:7125/printer/info", ip))
                .timeout(Duration::from_secs(3))
                .send()
                .await
            {
                if resp.status().is_success() {
                    let name = resp
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|v| {
                            v["result"]["hostname"]
                                .as_str()
                                .map(|s| s.to_string())
                        });
                    found.push(DetectPrintersResult {
                        ip: ip.clone(),
                        port: 7125,
                        printer_type: Printer3DType::Moonraker,
                        name,
                    });
                }
            }

            // Creality Stock: /info en puerto 80 devuelve {"model":"K1",...}
            if let Ok(resp) = client
                .get(format!("http://{}:80/info", ip))
                .timeout(Duration::from_secs(3))
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if json.get("model").is_some() {
                            let name = json["model"]
                                .as_str()
                                .map(|s| format!("Creality {}", s));
                            found.push(DetectPrintersResult {
                                ip: ip.clone(),
                                port: 9999,
                                printer_type: Printer3DType::CrealityStock,
                                name,
                            });
                        }
                    }
                }
            }

            // FlashForge: TCP puerto 8899 con M601
            {
                let ip2 = ip.clone();
                if let Ok(Ok(resp)) = tokio::time::timeout(
                    Duration::from_secs(3),
                    flashforge_command(&ip2, "M115"),
                )
                .await
                {
                    if resp.contains("Machine Type:") {
                        let name = resp
                            .lines()
                            .find(|l| l.starts_with("Machine Name:"))
                            .map(|l| {
                                l.trim_start_matches("Machine Name:")
                                    .trim()
                                    .to_string()
                            });
                        found.push(DetectPrintersResult {
                            ip: ip2,
                            port: 8899,
                            printer_type: Printer3DType::FlashForge,
                            name,
                        });
                    }
                }
            }

            found
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(printers) = handle.await {
            results.extend(printers);
        }
    }

    Ok(Json(results))
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
// Precalentar
// =====================

pub async fn preheat(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PreheatRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    match printer.printer_type {
        Printer3DType::OctoPrint => {
            // Hotend
            let resp = octoprint_request(
                client,
                reqwest::Method::POST,
                &format!("{}/api/printer/tool", base),
                &printer,
            )
            .json(&serde_json::json!({
                "command": "target",
                "targets": {"tool0": req.hotend}
            }))
            .send()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error hotend: {}", e)))?;

            if !resp.status().is_success() {
                let st = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((StatusCode::BAD_GATEWAY, format!("OctoPrint hotend {}: {}", st, body)));
            }

            // Cama
            let resp = octoprint_request(
                client,
                reqwest::Method::POST,
                &format!("{}/api/printer/bed", base),
                &printer,
            )
            .json(&serde_json::json!({
                "command": "target",
                "target": req.bed
            }))
            .send()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error cama: {}", e)))?;

            if !resp.status().is_success() {
                let st = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err((StatusCode::BAD_GATEWAY, format!("OctoPrint cama {}: {}", st, body)));
            }
        }
        Printer3DType::Moonraker => {
            let gcode = format!("M104 S{}\nM140 S{}", req.hotend as i64, req.bed as i64);
            let resp = client
                .post(format!("{}/printer/gcode/script", base))
                .json(&serde_json::json!({"script": gcode}))
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
                serde_json::json!({"method": "set", "params": {"nozzleTempControl": req.hotend as i64}}),
            )
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
            creality_ws_command(
                &printer.ip,
                serde_json::json!({"method": "set", "params": {"bedTempControl": {"num": 0, "val": req.bed as i64}}}),
            )
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
        Printer3DType::FlashForge => {
            let cmd1 = format!("M104 S{}", req.hotend as i64);
            let cmd2 = format!("M140 S{}", req.bed as i64);
            flashforge_commands(&printer.ip, &[&cmd1, &cmd2])
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
    }

    state
        .log_activity(
            "Impresoras 3D",
            &format!(
                "Precalentar {}: hotend={}°C, cama={}°C",
                printer.name, req.hotend, req.bed
            ),
            "sistema",
        )
        .await;

    Ok((
        StatusCode::OK,
        format!(
            "Precalentando hotend a {}°C y cama a {}°C",
            req.hotend, req.bed
        ),
    ))
}

// =====================
// Home ejes
// =====================

pub async fn home_axes(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<HomeRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    match printer.printer_type {
        Printer3DType::OctoPrint => {
            let axes = if req.axes.is_empty() {
                vec!["x".to_string(), "y".to_string(), "z".to_string()]
            } else {
                req.axes.clone()
            };

            let resp = octoprint_request(
                client,
                reqwest::Method::POST,
                &format!("{}/api/printer/printhead", base),
                &printer,
            )
            .json(&serde_json::json!({
                "command": "home",
                "axes": axes
            }))
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
            let gcode = if req.axes.is_empty() {
                "G28".to_string()
            } else {
                let axes_str = req
                    .axes
                    .iter()
                    .map(|a| a.to_uppercase())
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("G28 {}", axes_str)
            };

            let resp = client
                .post(format!("{}/printer/gcode/script", base))
                .json(&serde_json::json!({"script": gcode}))
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
            let axes_str = if req.axes.is_empty() {
                "X Y Z".to_string()
            } else {
                req.axes
                    .iter()
                    .map(|a| a.to_uppercase())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            creality_ws_command(
                &printer.ip,
                serde_json::json!({"method": "set", "params": {"autohome": axes_str}}),
            )
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
        Printer3DType::FlashForge => {
            let gcode = if req.axes.is_empty() {
                "G28".to_string()
            } else {
                format!(
                    "G28 {}",
                    req.axes
                        .iter()
                        .map(|a| a.to_uppercase())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };
            flashforge_command(&printer.ip, &gcode)
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
    }

    Ok((StatusCode::OK, "Home enviado".to_string()))
}

// =====================
// Jog (mover ejes)
// =====================

pub async fn jog(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<JogRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    match printer.printer_type {
        Printer3DType::OctoPrint => {
            let resp = octoprint_request(
                client,
                reqwest::Method::POST,
                &format!("{}/api/printer/printhead", base),
                &printer,
            )
            .json(&serde_json::json!({
                "command": "jog",
                "x": req.x,
                "y": req.y,
                "z": req.z
            }))
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
            // G91: modo relativo, G1: mover, G90: volver a absoluto
            let mut moves = Vec::new();
            if req.x != 0.0 {
                moves.push(format!("X{}", req.x));
            }
            if req.y != 0.0 {
                moves.push(format!("Y{}", req.y));
            }
            if req.z != 0.0 {
                moves.push(format!("Z{}", req.z));
            }

            if moves.is_empty() {
                return Ok((StatusCode::OK, "Sin movimiento".to_string()));
            }

            let gcode = format!("G91\nG1 {} F3000\nG90", moves.join(" "));
            let resp = client
                .post(format!("{}/printer/gcode/script", base))
                .json(&serde_json::json!({"script": gcode}))
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
            if req.x != 0.0 {
                let dir = format!("X{} F3000", req.x);
                creality_ws_command(
                    &printer.ip,
                    serde_json::json!({"method": "set", "params": {"setPosition": dir}}),
                )
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
            }
            if req.y != 0.0 {
                let dir = format!("Y{} F3000", req.y);
                creality_ws_command(
                    &printer.ip,
                    serde_json::json!({"method": "set", "params": {"setPosition": dir}}),
                )
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
            }
            if req.z != 0.0 {
                let dir = format!("Z{} F600", req.z);
                creality_ws_command(
                    &printer.ip,
                    serde_json::json!({"method": "set", "params": {"setPosition": dir}}),
                )
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
            }
        }
        Printer3DType::FlashForge => {
            let mut moves = Vec::new();
            if req.x != 0.0 {
                moves.push(format!("X{}", req.x));
            }
            if req.y != 0.0 {
                moves.push(format!("Y{}", req.y));
            }
            if req.z != 0.0 {
                moves.push(format!("Z{}", req.z));
            }
            if !moves.is_empty() {
                let move_cmd = format!("G1 {} F3000", moves.join(" "));
                flashforge_commands(&printer.ip, &["G91", &move_cmd, "G90"])
                    .await
                    .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
            }
        }
    }

    Ok((
        StatusCode::OK,
        format!("Movido X:{} Y:{} Z:{}", req.x, req.y, req.z),
    ))
}

// =====================
// Enviar G-code manual
// =====================

pub async fn send_gcode(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<GcodeRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    let client = &state.http_client;
    let base = printer_base(&printer);

    match printer.printer_type {
        Printer3DType::OctoPrint => {
            let resp = octoprint_request(
                client,
                reqwest::Method::POST,
                &format!("{}/api/printer/command", base),
                &printer,
            )
            .json(&serde_json::json!({"command": req.command}))
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
                .post(format!("{}/printer/gcode/script", base))
                .json(&serde_json::json!({"script": req.command}))
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
                serde_json::json!({"method": "set", "params": {"gcodeCmd": req.command}}),
            )
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
        Printer3DType::FlashForge => {
            let lines: Vec<&str> = req
                .command
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect();
            flashforge_commands(&printer.ip, &lines)
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
    }

    Ok((StatusCode::OK, format!("G-code enviado: {}", req.command)))
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

// =====================
// Monitor loop - notificaciones de fin de impresión
// =====================

/// Estado previo de cada impresora para detectar cambios
struct PrinterMonitorState {
    was_printing: bool,
    file_name: String,
    start_time: Option<std::time::Instant>,
}

pub async fn printer_monitor_loop(state: AppState) {
    let mut printer_states: std::collections::HashMap<String, PrinterMonitorState> =
        std::collections::HashMap::new();

    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        let config = state.config.lock().await;
        let printers = config.printers3d.clone();
        let token = config.notifications.bot_token.clone();
        let chats = config.notifications.telegram_chats.clone();
        drop(config);

        let Some(token) = token else {
            continue;
        };

        if chats.is_empty() || printers.is_empty() {
            continue;
        }

        for printer in &printers {
            let client = &state.http_client;
            let base = printer_base(printer);

            // Obtener estado actual
            let status = match printer.printer_type {
                Printer3DType::OctoPrint => {
                    fetch_octoprint_status(client, &base, printer).await
                }
                Printer3DType::Moonraker => {
                    fetch_moonraker_status(client, &base, printer).await
                }
                Printer3DType::CrealityStock => {
                    fetch_creality_status(&printer.ip, printer).await
                }
                Printer3DType::FlashForge => {
                    fetch_flashforge_status(&printer.ip, printer).await
                }
            };

            let Ok(Json(status)) = status else {
                continue;
            };

            if !status.online {
                continue;
            }

            let is_printing = status
                .current_job
                .as_ref()
                .map(|j| {
                    let s = j.state.to_lowercase();
                    s.contains("printing") || s == "printing"
                })
                .unwrap_or(false);

            let file_name = status
                .current_job
                .as_ref()
                .map(|j| j.file_name.clone())
                .unwrap_or_default();

            let has_error = status
                .current_job
                .as_ref()
                .map(|j| {
                    let s = j.state.to_lowercase();
                    s.contains("error")
                })
                .unwrap_or(false);

            let prev = printer_states.get(&printer.id);
            let was_printing = prev.map(|p| p.was_printing).unwrap_or(false);

            // Detectar transición: imprimiendo -> idle/terminado
            if was_printing && !is_printing && !has_error {
                let prev_file = prev.map(|p| p.file_name.as_str()).unwrap_or("?");
                let elapsed = prev
                    .and_then(|p| p.start_time)
                    .map(|t| {
                        let secs = t.elapsed().as_secs();
                        let h = secs / 3600;
                        let m = (secs % 3600) / 60;
                        if h > 0 {
                            format!("{}h {}m", h, m)
                        } else {
                            format!("{}m", m)
                        }
                    })
                    .unwrap_or_else(|| "?".to_string());

                let msg = format!(
                    "Impresion terminada en *{}*\nArchivo: `{}`\nTiempo: {}",
                    printer.name, prev_file, elapsed
                );

                for chat in &chats {
                    if chat.role != crate::models::notifications::UserRole::Pendiente {
                        let _ = crate::handlers::notifications::send_tg_public(
                            &state.http_client,
                            &token,
                            chat.chat_id,
                            &msg,
                        )
                        .await;
                    }
                }

                state
                    .log_activity(
                        "Impresoras 3D",
                        &format!("Impresion terminada: {} en {}", prev_file, printer.name),
                        "sistema",
                    )
                    .await;
            }

            // Detectar error
            if has_error && was_printing {
                let error_state = status
                    .current_job
                    .as_ref()
                    .map(|j| j.state.as_str())
                    .unwrap_or("Error");

                let msg = format!(
                    "Error en impresora *{}*\nEstado: {}",
                    printer.name, error_state
                );

                for chat in &chats {
                    if chat.role != crate::models::notifications::UserRole::Pendiente {
                        let _ = crate::handlers::notifications::send_tg_public(
                            &state.http_client,
                            &token,
                            chat.chat_id,
                            &msg,
                        )
                        .await;
                    }
                }
            }

            // Actualizar estado
            printer_states.insert(
                printer.id.clone(),
                PrinterMonitorState {
                    was_printing: is_printing,
                    file_name,
                    start_time: if is_printing && !was_printing {
                        Some(std::time::Instant::now())
                    } else if is_printing {
                        prev.and_then(|p| p.start_time)
                    } else {
                        None
                    },
                },
            );
        }
    }
}
