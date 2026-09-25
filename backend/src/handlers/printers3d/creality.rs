//! Creality stock (WebSocket en :9999)

use super::*;

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

    ws.send(Message::Text(msg.to_string()))
        .await
        .map_err(|e| format!("Error enviando WS: {}", e))?;

    let response = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(Ok(msg)) = ws.next().await {
            if let Message::Text(text) = msg {
                let text_str: String = text;
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

pub(super) async fn fetch_creality_status(
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
