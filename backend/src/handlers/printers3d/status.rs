//! Estado de impresoras (OctoPrint / Moonraker y despacho por tipo)

use super::*;

// =====================
// Status
// =====================

pub async fn printer_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Printer3DStatus>, (StatusCode, String)> {
    let printer = find_printer(&state, &id).await?;
    fetch_status(&state.http_client, &printer).await
}

pub(super) async fn fetch_status(
    client: &reqwest::Client,
    printer: &Printer3DConfig,
) -> Result<Json<Printer3DStatus>, (StatusCode, String)> {
    let base = printer_base(printer);
    match printer.printer_type {
        Printer3DType::OctoPrint => fetch_octoprint_status(client, &base, printer).await,
        Printer3DType::Moonraker => fetch_moonraker_status(client, &base, printer).await,
        Printer3DType::CrealityStock => fetch_creality_status(&printer.ip, printer).await,
        Printer3DType::FlashForge => fetch_flashforge_status(&printer.ip, printer).await,
    }
}

pub(super) async fn fetch_octoprint_status(
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

pub(super) async fn fetch_moonraker_status(
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
