//! Deteccion automatica de impresoras en la red

use super::*;

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
