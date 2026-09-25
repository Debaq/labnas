//! Precalentar, home, jog y gcode manual

use super::*;

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
