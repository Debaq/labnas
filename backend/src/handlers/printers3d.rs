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
        // URL por defecto según el tipo
        format!("{}/webcam/?action=snapshot", base)
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
