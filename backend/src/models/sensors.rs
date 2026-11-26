use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorDevice {
    pub id: String,
    pub name: String,
    pub mac: String,
    pub device_type: String,
    pub connection: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rssi: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    pub config: String,
    pub created_at: String,
    /// Tiene token propio (se exige en cada envio)
    pub has_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    pub id: i64,
    pub device_id: String,
    pub key: String,
    pub value: f64,
    pub unit: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorAlert {
    pub id: String,
    pub device_id: String,
    pub key: String,
    pub condition: String,
    pub threshold: f64,
    pub enabled: bool,
    pub cooldown_minutes: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_triggered: Option<String>,
    /// Critica: abre un incidente y escala por la lista en vez de avisar a todos
    #[serde(default)]
    pub critical: bool,
    /// Personas a avisar, en orden (cuentas web)
    #[serde(default)]
    pub escalation: Vec<String>,
    /// Minutos sin confirmar antes de avisar al siguiente
    #[serde(default = "default_escalate")]
    pub escalate_minutes: i32,
}

fn default_escalate() -> i32 {
    10
}

/// Columnas de `sensor_alerts` en el orden de `row_to_alert`
pub const ALERT_COLS: &str = "id, device_id, key, condition, threshold, enabled, cooldown_minutes, last_triggered, critical, escalation, escalate_minutes";

pub fn row_to_alert(row: &rusqlite::Row) -> rusqlite::Result<SensorAlert> {
    let escalation: String = row.get(9)?;
    Ok(SensorAlert {
        id: row.get(0)?,
        device_id: row.get(1)?,
        key: row.get(2)?,
        condition: row.get(3)?,
        threshold: row.get(4)?,
        enabled: row.get(5)?,
        cooldown_minutes: row.get(6)?,
        last_triggered: row.get(7)?,
        critical: row.get::<_, i64>(8)? != 0,
        escalation: serde_json::from_str(&escalation).unwrap_or_default(),
        escalate_minutes: row.get(10)?,
    })
}

#[derive(Debug, Deserialize)]
pub struct SensorDataPayload {
    pub mac: String,
    #[serde(default)]
    pub bat: Option<i32>,
    #[serde(default)]
    pub rssi: Option<i32>,
    pub readings: Vec<SensorReadingEntry>,
    /// Token del dispositivo (alternativa al header X-Sensor-Token)
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SensorReadingEntry {
    pub key: String,
    pub val: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SensorLatest {
    pub device: Option<SensorDevice>,
    pub readings: HashMap<String, f64>,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiverStatus {
    pub connected: bool,
    pub port: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterDeviceReq {
    pub name: String,
    pub mac: String,
    #[serde(default)]
    pub device_type: String,
    #[serde(default = "default_connection")]
    pub connection: String,
}

fn default_connection() -> String {
    "wifi".to_string()
}

#[derive(Debug, Deserialize)]
pub struct DeviceStatusReq {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAlertReq {
    pub device_id: String,
    pub key: String,
    #[serde(default = "default_condition")]
    pub condition: String,
    pub threshold: f64,
    #[serde(default = "default_cooldown")]
    pub cooldown_minutes: i32,
    #[serde(default)]
    pub critical: bool,
    #[serde(default)]
    pub escalation: Vec<String>,
    #[serde(default = "default_escalate")]
    pub escalate_minutes: i32,
}

fn default_condition() -> String {
    "above".to_string()
}

fn default_cooldown() -> i32 {
    30
}

#[derive(Debug, Deserialize)]
pub struct UpdateAlertReq {
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub threshold: Option<f64>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub cooldown_minutes: Option<i32>,
    #[serde(default)]
    pub critical: Option<bool>,
    #[serde(default)]
    pub escalation: Option<Vec<String>>,
    #[serde(default)]
    pub escalate_minutes: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ReceiverConfigReq {
    pub port: String,
}

#[derive(Debug, Deserialize)]
pub struct ReadingsQuery {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    1000
}

/// Unidad por defecto para cada key
pub fn reading_key_unit(key: &str) -> &'static str {
    match key {
        "temperature" => "°C",
        "humidity" | "soil_moisture" => "%",
        "pressure" => "hPa",
        "co2" => "ppm",
        "tvoc" => "ppb",
        "pm2_5" | "pm10" => "µg/m³",
        "light" => "lux",
        "uv_index" => "",
        "noise" => "dB",
        "water_level" | "distance" => "cm",
        "ph" => "",
        "conductivity" => "µS/cm",
        "dissolved_o2" => "mg/L",
        "wind_speed" => "m/s",
        "wind_direction" => "°",
        "rain" => "mm",
        "weight" => "g",
        "current" => "A",
        "voltage" => "V",
        "power" => "W",
        "energy" => "Wh",
        "flow_rate" => "L/min",
        _ => "",
    }
}
