use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};
use std::time::Duration;

use crate::config::save_config;
use crate::models::printers3d::*;
use crate::state::AppState;

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
    };

    let mut config = state.config.lock().await;
    config.printers3d.push(printer.clone());
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok((StatusCode::CREATED, Json(printer)))
}

pub async fn delete_printer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.printers3d.len();
    config.printers3d.retain(|p| p.id != id);

    if config.printers3d.len() == before {
        return Err((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()));
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn printer_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Printer3DStatus>, (StatusCode, String)> {
    let config = state.config.lock().await;
    let printer = config
        .printers3d
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()))?;
    drop(config);

    let client = &state.http_client;
    let base = format!("http://{}:{}", printer.ip, printer.port);

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
    let mut req = client
        .get(format!("{}/api/printer", base))
        .timeout(Duration::from_secs(5));
    if let Some(key) = &printer.api_key {
        req = req.header("X-Api-Key", key);
    }

    let printer_resp = req.send().await;
    let online = printer_resp.as_ref().map(|r| r.status().is_success()).unwrap_or(false);

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

    let mut job_req = client
        .get(format!("{}/api/job", base))
        .timeout(Duration::from_secs(5));
    if let Some(key) = &printer.api_key {
        job_req = job_req.header("X-Api-Key", key);
    }

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

pub async fn upload_gcode(
    State(state): State<AppState>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let config = state.config.lock().await;
    let printer = config
        .printers3d
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()))?;
    drop(config);

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
    let base = format!("http://{}:{}", printer.ip, printer.port);

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

            let resp = req
                .send()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error enviando a OctoPrint: {}", e)))?;

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
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error enviando a Moonraker: {}", e)))?;

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

    Ok((
        StatusCode::OK,
        format!("Archivo '{}' subido correctamente", file_name),
    ))
}

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

            // OctoPrint on ports 80 and 5000
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

            // Moonraker on port 7125
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
