use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use rusqlite::params;
use std::io::BufRead;
use std::time::Duration;

use crate::models::sensors::*;
use crate::state::AppState;

// ═══════════════════════════════════════
// Dispositivos
// ═══════════════════════════════════════

pub async fn list_devices(
    State(state): State<AppState>,
) -> Json<Vec<SensorDevice>> {
    let devices = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, mac, device_type, connection, status, battery, rssi, last_seen, config, created_at
             FROM sensor_devices ORDER BY created_at DESC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(SensorDevice {
                id: row.get(0)?,
                name: row.get(1)?,
                mac: row.get(2)?,
                device_type: row.get(3)?,
                connection: row.get(4)?,
                status: row.get(5)?,
                battery: row.get(6)?,
                rssi: row.get(7)?,
                last_seen: row.get(8)?,
                config: row.get(9)?,
                created_at: row.get(10)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await.unwrap_or_default();
    Json(devices)
}

pub async fn register_device(
    State(state): State<AppState>,
    Json(req): Json<RegisterDeviceReq>,
) -> Result<Json<SensorDevice>, (StatusCode, String)> {
    let now = Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let mac = normalize_mac(&req.mac);
    let device = SensorDevice {
        id: id.clone(),
        name: req.name.trim().to_string(),
        mac: mac.clone(),
        device_type: req.device_type,
        connection: req.connection,
        status: "accepted".to_string(),
        battery: None,
        rssi: None,
        last_seen: None,
        config: "{}".to_string(),
        created_at: now.clone(),
    };
    let d = device.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO sensor_devices (id, name, mac, device_type, connection, status, config, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![d.id, d.name, d.mac, d.device_type, d.connection, d.status, d.config, d.created_at],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(())
    }).await?;

    state.log_activity("Sensores", &format!("Dispositivo registrado: {} ({})", device.name, device.mac), "admin").await;
    Ok(Json(device))
}

pub async fn delete_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let id_clone = id.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute("DELETE FROM sensor_devices WHERE id = ?1", params![id_clone])
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Dispositivo no encontrado".to_string()));
        }
        Ok(())
    }).await?;

    let mut sensors = state.sensors.lock().await;
    sensors.latest.remove(&id);
    drop(sensors);

    state.log_activity("Sensores", &format!("Dispositivo eliminado: {}", id), "admin").await;
    Ok(StatusCode::OK)
}

pub async fn update_device_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<DeviceStatusReq>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !matches!(req.status.as_str(), "pending" | "accepted" | "rejected") {
        return Err((StatusCode::BAD_REQUEST, "Estado invalido. Usar: pending, accepted, rejected".to_string()));
    }
    let id_clone = id.clone();
    let status = req.status.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute(
            "UPDATE sensor_devices SET status = ?1 WHERE id = ?2",
            params![status, id_clone],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Dispositivo no encontrado".to_string()));
        }
        Ok(())
    }).await?;

    state.log_activity("Sensores", &format!("Estado de dispositivo {} -> {}", id, req.status), "admin").await;
    Ok(StatusCode::OK)
}

pub async fn update_device_name(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    let name = req.get("name")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Falta campo 'name'".to_string()))?
        .trim().to_string();
    let id_clone = id.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute(
            "UPDATE sensor_devices SET name = ?1 WHERE id = ?2",
            params![name, id_clone],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Dispositivo no encontrado".to_string()));
        }
        Ok(())
    }).await?;
    Ok(StatusCode::OK)
}

// ═══════════════════════════════════════
// Ingesta de datos
// ═══════════════════════════════════════

pub async fn ingest_data(
    State(state): State<AppState>,
    Json(payload): Json<SensorDataPayload>,
) -> Result<StatusCode, (StatusCode, String)> {
    process_sensor_data(&state, payload).await
}

pub async fn process_sensor_data(
    state: &AppState,
    payload: SensorDataPayload,
) -> Result<StatusCode, (StatusCode, String)> {
    let mac = normalize_mac(&payload.mac);
    let now = Utc::now().to_rfc3339();

    // Buscar o auto-registrar dispositivo
    let mac_clone = mac.clone();
    let now_clone = now.clone();
    let device_id = crate::db::db_op_status(&state.db, move |conn| {
        let existing: Option<String> = conn.query_row(
            "SELECT id FROM sensor_devices WHERE mac = ?1",
            params![mac_clone],
            |row| row.get::<_, String>(0),
        ).ok();

        match existing {
            Some(id) => Ok(id),
            None => {
                let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
                conn.execute(
                    "INSERT INTO sensor_devices (id, name, mac, connection, status, created_at)
                     VALUES (?1, '', ?2, 'espnow', 'pending', ?3)",
                    params![id, mac_clone, now_clone],
                ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                Ok(id)
            }
        }
    }).await?;

    // Actualizar battery, rssi, last_seen
    let mac_update = mac.clone();
    let bat = payload.bat;
    let rssi = payload.rssi;
    let now_update = now.clone();
    let _ = crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE sensor_devices SET battery = ?1, rssi = ?2, last_seen = ?3 WHERE mac = ?4",
            params![bat, rssi, now_update, mac_update],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }).await;

    // Insertar readings
    let readings_clone = payload.readings.clone();
    let dev_id = device_id.clone();
    let now_readings = now.clone();
    let _ = crate::db::db_op(&state.db, move |conn| {
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        for r in &readings_clone {
            let unit = reading_key_unit(&r.key);
            tx.execute(
                "INSERT INTO sensor_readings (device_id, key, value, unit, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![dev_id, r.key, r.val, unit, now_readings],
            ).map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }).await;

    // Actualizar cache en memoria
    {
        let mut sensors = state.sensors.lock().await;
        let entry = sensors.latest.entry(device_id.clone()).or_default();
        entry.online = true;
        for r in &payload.readings {
            entry.readings.insert(r.key.clone(), r.val);
        }
    }

    // Verificar alertas (solo dispositivos accepted)
    let dev_id_alert = device_id.clone();
    let device_status: Option<String> = crate::db::db_op(&state.db, move |conn| {
        conn.query_row(
            "SELECT status FROM sensor_devices WHERE id = ?1",
            params![dev_id_alert],
            |row| row.get::<_, String>(0),
        ).map_err(|e| e.to_string())
    }).await.ok();

    if device_status.as_deref() == Some("accepted") {
        check_alerts(state, &device_id, &payload.readings).await;
    }

    Ok(StatusCode::OK)
}

async fn check_alerts(state: &AppState, device_id: &str, readings: &[SensorReadingEntry]) {
    let dev_id = device_id.to_string();
    let alerts: Vec<SensorAlert> = crate::db::db_op(&state.db, move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, device_id, key, condition, threshold, enabled, cooldown_minutes, last_triggered
             FROM sensor_alerts WHERE device_id = ?1 AND enabled = 1"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![dev_id], |row| {
            Ok(SensorAlert {
                id: row.get(0)?,
                device_id: row.get(1)?,
                key: row.get(2)?,
                condition: row.get(3)?,
                threshold: row.get(4)?,
                enabled: row.get(5)?,
                cooldown_minutes: row.get(6)?,
                last_triggered: row.get(7)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await.unwrap_or_default();

    let now = Utc::now();

    for alert in &alerts {
        let Some(reading) = readings.iter().find(|r| r.key == alert.key) else {
            continue;
        };

        let triggered = match alert.condition.as_str() {
            "above" => reading.val > alert.threshold,
            "below" => reading.val < alert.threshold,
            _ => false,
        };

        if !triggered {
            continue;
        }

        // Verificar cooldown
        if let Some(last) = &alert.last_triggered {
            if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last) {
                let cooldown = chrono::Duration::minutes(alert.cooldown_minutes as i64);
                if now.signed_duration_since(last_time) < cooldown {
                    continue;
                }
            }
        }

        // Actualizar last_triggered
        let alert_id = alert.id.clone();
        let now_str = now.to_rfc3339();
        let _ = crate::db::db_op(&state.db, move |conn| {
            conn.execute(
                "UPDATE sensor_alerts SET last_triggered = ?1 WHERE id = ?2",
                params![now_str, alert_id],
            ).map_err(|e| e.to_string())?;
            Ok(())
        }).await;

        // Obtener nombre del dispositivo
        let dev_id = device_id.to_string();
        let device_name: String = crate::db::db_op(&state.db, move |conn| {
            conn.query_row(
                "SELECT name FROM sensor_devices WHERE id = ?1",
                params![dev_id],
                |row| row.get::<_, String>(0),
            ).map_err(|e| e.to_string())
        }).await.unwrap_or_else(|_| device_id.to_string());

        let display_name = if device_name.is_empty() { device_id.to_string() } else { device_name };
        let unit = reading_key_unit(&alert.key);
        let cond_text = if alert.condition == "above" { "supera" } else { "bajo" };
        let msg = format!(
            "Alerta sensor: *{}*\n{}: {:.1}{} ({} umbral {:.1}{})",
            display_name, alert.key, reading.val, unit, cond_text, alert.threshold, unit
        );

        // Enviar por Telegram
        send_sensor_alert(state, &msg).await;

        state.log_activity(
            "Sensores",
            &format!("Alerta: {} {} {:.1} (umbral {:.1})", alert.key, alert.condition, reading.val, alert.threshold),
            &display_name,
        ).await;
    }
}

async fn send_sensor_alert(state: &AppState, msg: &str) {
    let conn = match crate::db::get_conn(&state.db) {
        Ok(c) => c,
        Err(_) => return,
    };

    let token: Option<String> = conn.query_row(
        "SELECT bot_token FROM notification_config WHERE id = 1",
        [],
        |row| row.get::<_, Option<String>>(0),
    ).ok().flatten();

    let Some(ref token) = token else { return };

    let chats: Vec<i64> = (|| -> Result<Vec<i64>, rusqlite::Error> {
        let mut stmt = conn.prepare("SELECT chat_id, role FROM telegram_chats")?;
        let rows = stmt.query_map([], |row: &rusqlite::Row| {
            let role: String = row.get(1)?;
            if role != "pendiente" {
                Ok(Some(row.get::<_, i64>(0)?))
            } else {
                Ok(None)
            }
        })?;
        Ok(rows.filter_map(|r: Result<Option<i64>, _>| r.ok().flatten()).collect())
    })().unwrap_or_default();

    drop(conn);

    for chat_id in chats {
        let _ = crate::handlers::notifications::send_tg_public(
            &state.http_client,
            token,
            chat_id,
            msg,
        ).await;
    }
}

// ═══════════════════════════════════════
// Lecturas históricas
// ═══════════════════════════════════════

pub async fn get_readings(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ReadingsQuery>,
) -> Result<Json<Vec<SensorReading>>, (StatusCode, String)> {
    let limit = q.limit.min(10000);
    let readings = crate::db::db_op_status(&state.db, move |conn| {
        let mut sql = String::from(
            "SELECT id, device_id, key, value, unit, timestamp FROM sensor_readings WHERE device_id = ?1"
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(id.clone())];

        if let Some(ref key) = q.key {
            sql.push_str(" AND key = ?");
            param_values.push(Box::new(key.clone()));
        }
        if let Some(ref from) = q.from {
            sql.push_str(" AND timestamp >= ?");
            param_values.push(Box::new(from.clone()));
        }
        if let Some(ref to) = q.to {
            sql.push_str(" AND timestamp <= ?");
            param_values.push(Box::new(to.clone()));
        }
        sql.push_str(" ORDER BY timestamp DESC LIMIT ?");
        param_values.push(Box::new(limit));

        let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let rows = stmt.query_map(params_ref.as_slice(), |row| {
            Ok(SensorReading {
                id: row.get(0)?,
                device_id: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                unit: row.get(4)?,
                timestamp: row.get(5)?,
            })
        }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?);
        }
        Ok(result)
    }).await?;
    Ok(Json(readings))
}

pub async fn get_latest(
    State(state): State<AppState>,
) -> Json<Vec<SensorLatest>> {
    let devices: Vec<SensorDevice> = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, mac, device_type, connection, status, battery, rssi, last_seen, config, created_at
             FROM sensor_devices ORDER BY created_at DESC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(SensorDevice {
                id: row.get(0)?,
                name: row.get(1)?,
                mac: row.get(2)?,
                device_type: row.get(3)?,
                connection: row.get(4)?,
                status: row.get(5)?,
                battery: row.get(6)?,
                rssi: row.get(7)?,
                last_seen: row.get(8)?,
                config: row.get(9)?,
                created_at: row.get(10)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await.unwrap_or_default();

    let sensors = state.sensors.lock().await;
    let now = Utc::now();

    let mut result = Vec::new();
    for device in devices {
        let cached = sensors.latest.get(&device.id);
        let online = device.last_seen.as_ref().map(|ls| {
            chrono::DateTime::parse_from_rfc3339(ls)
                .map(|t| now.signed_duration_since(t) < chrono::Duration::minutes(5))
                .unwrap_or(false)
        }).unwrap_or(false);

        result.push(SensorLatest {
            readings: cached.map(|c| c.readings.clone()).unwrap_or_default(),
            device: Some(device),
            online,
        });
    }

    Json(result)
}

// ═══════════════════════════════════════
// Alertas
// ═══════════════════════════════════════

pub async fn list_alerts(
    State(state): State<AppState>,
) -> Json<Vec<SensorAlert>> {
    let alerts = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, device_id, key, condition, threshold, enabled, cooldown_minutes, last_triggered
             FROM sensor_alerts ORDER BY device_id"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(SensorAlert {
                id: row.get(0)?,
                device_id: row.get(1)?,
                key: row.get(2)?,
                condition: row.get(3)?,
                threshold: row.get(4)?,
                enabled: row.get(5)?,
                cooldown_minutes: row.get(6)?,
                last_triggered: row.get(7)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await.unwrap_or_default();
    Json(alerts)
}

pub async fn create_alert(
    State(state): State<AppState>,
    Json(req): Json<CreateAlertReq>,
) -> Result<Json<SensorAlert>, (StatusCode, String)> {
    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let alert = SensorAlert {
        id: id.clone(),
        device_id: req.device_id,
        key: req.key,
        condition: req.condition,
        threshold: req.threshold,
        enabled: true,
        cooldown_minutes: req.cooldown_minutes,
        last_triggered: None,
    };
    let a = alert.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO sensor_alerts (id, device_id, key, condition, threshold, enabled, cooldown_minutes)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
            params![a.id, a.device_id, a.key, a.condition, a.threshold, a.cooldown_minutes],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(())
    }).await?;
    Ok(Json(alert))
}

pub async fn update_alert(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAlertReq>,
) -> Result<StatusCode, (StatusCode, String)> {
    let id_clone = id.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        let mut sets = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref condition) = req.condition {
            sets.push("condition = ?");
            values.push(Box::new(condition.clone()));
        }
        if let Some(threshold) = req.threshold {
            sets.push("threshold = ?");
            values.push(Box::new(threshold));
        }
        if let Some(enabled) = req.enabled {
            sets.push("enabled = ?");
            values.push(Box::new(enabled as i32));
        }
        if let Some(cooldown) = req.cooldown_minutes {
            sets.push("cooldown_minutes = ?");
            values.push(Box::new(cooldown));
        }

        if sets.is_empty() {
            return Ok(());
        }

        values.push(Box::new(id_clone));
        let sql = format!("UPDATE sensor_alerts SET {} WHERE id = ?", sets.join(", "));
        let params_ref: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|p| p.as_ref()).collect();
        let changed = conn.execute(&sql, params_ref.as_slice())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Alerta no encontrada".to_string()));
        }
        Ok(())
    }).await?;
    Ok(StatusCode::OK)
}

pub async fn delete_alert(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute("DELETE FROM sensor_alerts WHERE id = ?1", params![id])
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Alerta no encontrada".to_string()));
        }
        Ok(())
    }).await?;
    Ok(StatusCode::OK)
}

// ═══════════════════════════════════════
// Receptor serial
// ═══════════════════════════════════════

pub async fn configure_receiver(
    State(state): State<AppState>,
    Json(req): Json<ReceiverConfigReq>,
) -> Result<StatusCode, (StatusCode, String)> {
    let port = req.port.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('sensor_serial_port', ?1)",
            params![port],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(())
    }).await?;

    let mut sensors = state.sensors.lock().await;
    sensors.receiver_port = Some(req.port);
    sensors.receiver_error = None;

    Ok(StatusCode::OK)
}

pub async fn receiver_status(
    State(state): State<AppState>,
) -> Json<ReceiverStatus> {
    let sensors = state.sensors.lock().await;
    Json(ReceiverStatus {
        connected: sensors.receiver_connected,
        port: sensors.receiver_port.clone(),
        error: sensors.receiver_error.clone(),
    })
}

// ═══════════════════════════════════════
// Background loops
// ═══════════════════════════════════════

pub async fn serial_listener_loop(state: AppState) {
    loop {
        let port_path = {
            let conn = match crate::db::get_conn(&state.db) {
                Ok(c) => c,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };
            crate::db::get_setting(&conn, "sensor_serial_port")
        };

        let Some(port_path) = port_path else {
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        };

        let port = serialport::new(&port_path, 115200)
            .timeout(Duration::from_secs(5))
            .open();

        match port {
            Ok(port) => {
                {
                    let mut sensors = state.sensors.lock().await;
                    sensors.receiver_connected = true;
                    sensors.receiver_port = Some(port_path.clone());
                    sensors.receiver_error = None;
                }
                println!("[Sensores] Receptor conectado en {}", port_path);

                let reader = std::io::BufReader::new(port);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            let line = line.trim().to_string();
                            if line.is_empty() {
                                continue;
                            }

                            if let Ok(status) = serde_json::from_str::<serde_json::Value>(&line) {
                                if status.get("status").is_some() {
                                    println!("[Sensores] Receptor: {}", line);
                                    continue;
                                }
                            }

                            match serde_json::from_str::<SensorDataPayload>(&line) {
                                Ok(payload) => {
                                    if let Err(e) = process_sensor_data(&state, payload).await {
                                        eprintln!("[Sensores] Error procesando dato: {}", e.1);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[Sensores] JSON invalido: {} - {}", e, line);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[Sensores] Error lectura serial: {}", e);
                            break;
                        }
                    }
                }

                {
                    let mut sensors = state.sensors.lock().await;
                    sensors.receiver_connected = false;
                    sensors.receiver_error = Some("Desconectado".to_string());
                }
                eprintln!("[Sensores] Receptor desconectado de {}", port_path);
            }
            Err(e) => {
                let mut sensors = state.sensors.lock().await;
                sensors.receiver_connected = false;
                sensors.receiver_error = Some(format!("Error: {}", e));
            }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn sensor_monitor_loop(state: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;

        let retention_days = {
            let conn = match crate::db::get_conn(&state.db) {
                Ok(c) => c,
                Err(_) => continue,
            };
            crate::db::get_setting_u32(&conn, "sensor_retention_days", 30)
        };

        let cutoff = (Utc::now() - chrono::Duration::days(retention_days as i64)).to_rfc3339();
        let cutoff_clone = cutoff.clone();
        let deleted = crate::db::db_op(&state.db, move |conn| {
            conn.execute(
                "DELETE FROM sensor_readings WHERE timestamp < ?1",
                params![cutoff_clone],
            ).map_err(|e| e.to_string())
        }).await.unwrap_or(0);

        if deleted > 0 {
            println!("[Sensores] Limpieza: {} lecturas eliminadas (>{} dias)", deleted, retention_days);
        }

        let mut sensors = state.sensors.lock().await;
        let now = Utc::now();
        for entry in sensors.latest.values_mut() {
            if let Some(ref device) = entry.device {
                if let Some(ref ls) = device.last_seen {
                    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(ls) {
                        if now.signed_duration_since(t) > chrono::Duration::minutes(10) {
                            entry.online = false;
                        }
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════
// Helpers
// ═══════════════════════════════════════

fn normalize_mac(mac: &str) -> String {
    mac.to_uppercase()
        .replace('-', ":")
        .trim()
        .to_string()
}
