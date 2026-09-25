//! Alta, edicion, secciones, orden y prueba de home

use super::*;

// =====================
// CRUD básico
// =====================

pub async fn list_printers(State(state): State<AppState>) -> Result<Json<Vec<Printer3DConfig>>, (StatusCode, String)> {
    let printers = db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, ip, port, printer_type, labnas_decrypt(api_key), camera_url, power_watts, electricity_cost_kwh, section_id, \"order\" FROM printers3d ORDER BY \"order\""
        ).map_err(|e| format!("DB: {}", e))?;
        let rows = stmt.query_map([], row_to_printer)
            .map_err(|e| format!("DB: {}", e))?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row.map_err(|e| format!("DB row: {}", e))?);
        }
        Ok(list)
    }).await?;
    Ok(Json(printers))
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
        power_watts: None,
        electricity_cost_kwh: None,
        section_id: None,
        order: 0,
    };

    let p = printer.clone();
    let pt = printer_type_str(&p.printer_type).to_string();
    db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO printers3d (id, name, ip, port, printer_type, api_key, camera_url, power_watts, electricity_cost_kwh, section_id, \"order\") VALUES (?1, ?2, ?3, ?4, ?5, labnas_encrypt(?6), ?7, ?8, ?9, ?10, ?11)",
            params![p.id, p.name, p.ip, p.port, pt, p.api_key, p.camera_url, p.power_watts, p.electricity_cost_kwh, p.section_id, p.order],
        ).map_err(|e| format!("DB: {}", e))?;
        Ok(())
    }).await?;

    state
        .log_activity("Impresoras 3D", &format!("Agregada: {}", printer.name), "sistema")
        .await;

    Ok((StatusCode::CREATED, Json(printer)))
}

pub async fn update_printer(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePrinter3DRequest>,
) -> Result<Json<Printer3DConfig>, (StatusCode, String)> {
    let updated = db_op_status(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        // Fetch current
        let mut printer: Printer3DConfig = conn.query_row(
            "SELECT id, name, ip, port, printer_type, labnas_decrypt(api_key), camera_url, power_watts, electricity_cost_kwh, section_id, \"order\" FROM printers3d WHERE id = ?1",
            params![id],
            row_to_printer,
        ).optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
            .ok_or((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()))?;

        if let Some(name) = req.name { printer.name = name; }
        if let Some(ip) = req.ip { printer.ip = ip; }
        if let Some(port) = req.port { printer.port = port; }
        if let Some(printer_type) = req.printer_type { printer.printer_type = printer_type; }
        if let Some(api_key) = req.api_key { printer.api_key = api_key; }
        if let Some(camera_url) = req.camera_url { printer.camera_url = camera_url; }
        if let Some(section_id) = req.section_id { printer.section_id = section_id; }
        if let Some(power_watts) = req.power_watts { printer.power_watts = power_watts; }
        if let Some(electricity_cost_kwh) = req.electricity_cost_kwh { printer.electricity_cost_kwh = electricity_cost_kwh; }

        let pt = printer_type_str(&printer.printer_type).to_string();
        conn.execute(
            "UPDATE printers3d SET name=?1, ip=?2, port=?3, printer_type=?4, api_key=labnas_encrypt(?5), camera_url=?6, power_watts=?7, electricity_cost_kwh=?8, section_id=?9, \"order\"=?10 WHERE id=?11",
            params![printer.name, printer.ip, printer.port, pt, printer.api_key, printer.camera_url, printer.power_watts, printer.electricity_cost_kwh, printer.section_id, printer.order, printer.id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        Ok(printer)
    }).await?;

    Ok(Json(updated))
}

pub async fn delete_printer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let name = db_op_status(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        let name: Option<String> = conn.query_row(
            "SELECT name FROM printers3d WHERE id = ?1",
            params![&id],
            |row| row.get(0),
        ).optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        let name = name.ok_or((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()))?;

        conn.execute("DELETE FROM printers3d WHERE id = ?1", params![&id])
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        Ok(name)
    }).await?;

    state
        .log_activity("Impresoras 3D", &format!("Eliminada: {}", name), "sistema")
        .await;

    Ok(StatusCode::NO_CONTENT)
}

// =====================
// Secciones
// =====================

pub async fn list_sections(State(state): State<AppState>) -> Result<Json<Vec<Printer3DSection>>, (StatusCode, String)> {
    let sections = db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, \"order\" FROM printer3d_sections ORDER BY \"order\""
        ).map_err(|e| format!("DB: {}", e))?;
        let rows = stmt.query_map([], |row| {
            Ok(Printer3DSection {
                id: row.get(0)?,
                name: row.get(1)?,
                order: row.get(2)?,
            })
        }).map_err(|e| format!("DB: {}", e))?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row.map_err(|e| format!("DB row: {}", e))?);
        }
        Ok(list)
    }).await?;
    Ok(Json(sections))
}

pub async fn add_section(
    State(state): State<AppState>,
    Json(req): Json<AddSectionRequest>,
) -> Result<(StatusCode, Json<Printer3DSection>), (StatusCode, String)> {
    let section = db_op(&state.db, move |conn| {
        let max_order: i32 = conn.query_row(
            "SELECT COALESCE(MAX(\"order\"), -1) FROM printer3d_sections",
            [],
            |row| row.get(0),
        ).map_err(|e| format!("DB: {}", e))?;
        let section = Printer3DSection {
            id: uuid::Uuid::new_v4().to_string(),
            name: req.name,
            order: max_order + 1,
        };
        conn.execute(
            "INSERT INTO printer3d_sections (id, name, \"order\") VALUES (?1, ?2, ?3)",
            params![section.id, section.name, section.order],
        ).map_err(|e| format!("DB: {}", e))?;
        Ok(section)
    }).await?;
    Ok((StatusCode::CREATED, Json(section)))
}

pub async fn update_section(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSectionRequest>,
) -> Result<Json<Printer3DSection>, (StatusCode, String)> {
    let updated = db_op_status(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        let mut section: Printer3DSection = conn.query_row(
            "SELECT id, name, \"order\" FROM printer3d_sections WHERE id = ?1",
            params![id],
            |row| Ok(Printer3DSection { id: row.get(0)?, name: row.get(1)?, order: row.get(2)? }),
        ).optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?
            .ok_or((StatusCode::NOT_FOUND, "Seccion no encontrada".to_string()))?;

        if let Some(name) = req.name { section.name = name; }
        if let Some(order) = req.order { section.order = order; }

        conn.execute(
            "UPDATE printer3d_sections SET name=?1, \"order\"=?2 WHERE id=?3",
            params![section.name, section.order, section.id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;

        Ok(section)
    }).await?;
    Ok(Json(updated))
}

pub async fn delete_section(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    db_op_status(&state.db, move |conn| {
        let changes = conn.execute("DELETE FROM printer3d_sections WHERE id = ?1", params![&id])
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;
        if changes == 0 {
            return Err((StatusCode::NOT_FOUND, "Seccion no encontrada".to_string()));
        }
        // FK ON DELETE SET NULL ya maneja las impresoras de esta seccion
        Ok(())
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reorder_printer(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ReorderPrinterRequest>,
) -> Result<Json<Printer3DConfig>, (StatusCode, String)> {
    let updated = db_op_status(&state.db, move |conn| {
        let changes = conn.execute(
            "UPDATE printers3d SET section_id=?1, \"order\"=?2 WHERE id=?3",
            params![req.section_id, req.order, &id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))?;
        if changes == 0 {
            return Err((StatusCode::NOT_FOUND, "Impresora no encontrada".to_string()));
        }
        conn.query_row(
            "SELECT id, name, ip, port, printer_type, labnas_decrypt(api_key), camera_url, power_watts, electricity_cost_kwh, section_id, \"order\" FROM printers3d WHERE id = ?1",
            params![&id],
            row_to_printer,
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB: {}", e)))
    }).await?;
    Ok(Json(updated))
}

pub async fn reorder_sections(
    State(state): State<AppState>,
    Json(order): Json<Vec<String>>,
) -> Result<StatusCode, (StatusCode, String)> {
    db_op(&state.db, move |conn| {
        for (i, id) in order.iter().enumerate() {
            conn.execute(
                "UPDATE printer3d_sections SET \"order\"=?1 WHERE id=?2",
                params![i as i32, id],
            ).map_err(|e| format!("DB: {}", e))?;
        }
        Ok(())
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

// =====================
// Test Home (para impresoras detectadas)
// =====================

pub async fn test_home(
    State(state): State<AppState>,
    Json(req): Json<TestHomeRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let client = &state.http_client;
    let base = format!("http://{}:{}", req.ip, req.port);

    match req.printer_type {
        Printer3DType::OctoPrint => {
            let mut r = client
                .post(format!("{}/api/printer/printhead", base))
                .json(&serde_json::json!({"command": "home", "axes": ["x","y","z"]}))
                .timeout(Duration::from_secs(10));
            if let Some(key) = &req.api_key {
                r = r.header("X-Api-Key", key);
            }
            let resp = r.send().await
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
                .json(&serde_json::json!({"script": "G28"}))
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
                &req.ip,
                serde_json::json!({"method": "set", "params": {"autohome": "X Y Z"}}),
            )
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
        Printer3DType::FlashForge => {
            flashforge_command(&req.ip, "G28")
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        }
    }

    Ok((StatusCode::OK, "Home enviado".to_string()))
}
