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
}

#[derive(Debug, Deserialize)]
pub struct SensorDataPayload {
    pub mac: String,
    #[serde(default)]
    pub bat: Option<i32>,
    #[serde(default)]
    pub rssi: Option<i32>,
    pub readings: Vec<SensorReadingEntry>,
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

/// Mapa de key numérico del protocolo binario a nombre string
pub fn reading_key_name(code: u8) -> &'static str {
    match code {
        0x01 => "temperature",
        0x02 => "humidity",
        0x03 => "pressure",
        0x04 => "co2",
        0x05 => "tvoc",
        0x06 => "pm2_5",
        0x07 => "pm10",
        0x08 => "light",
        0x09 => "uv_index",
        0x0A => "noise",
        0x0B => "soil_moisture",
        0x0C => "water_level",
        0x0D => "ph",
        0x0E => "conductivity",
        0x0F => "dissolved_o2",
        0x10 => "wind_speed",
        0x11 => "wind_direction",
        0x12 => "rain",
        0x13 => "weight",
        0x14 => "current",
        0x15 => "voltage",
        0x16 => "power",
        0x17 => "energy",
        0x18 => "distance",
        0x19 => "flow_rate",
        _ => "custom",
    }
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
