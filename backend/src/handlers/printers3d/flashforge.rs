//! FlashForge (protocolo TCP de texto)

use super::*;

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

pub(super) fn parse_flashforge_temps(response: &str) -> PrinterTemps {
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

pub(super) async fn fetch_flashforge_status(
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
