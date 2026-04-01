use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Printer3DType {
    OctoPrint,
    Moonraker,
    CrealityStock,
    FlashForge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Printer3DSection {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Printer3DConfig {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub printer_type: Printer3DType,
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_url: Option<String>,
    /// Consumo electrico en watts
    #[serde(default)]
    pub power_watts: Option<f64>,
    /// Costo electricidad $/kWh
    #[serde(default)]
    pub electricity_cost_kwh: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    #[serde(default)]
    pub order: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Printer3DStatus {
    pub id: String,
    pub online: bool,
    pub temperatures: Option<PrinterTemps>,
    pub current_job: Option<PrintJob>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrinterTemps {
    pub hotend_actual: f64,
    pub hotend_target: f64,
    pub bed_actual: f64,
    pub bed_target: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrintJob {
    pub file_name: String,
    pub progress: f64,
    pub time_elapsed: Option<u64>,
    pub time_remaining: Option<u64>,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct AddPrinter3DRequest {
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub printer_type: Printer3DType,
    pub api_key: Option<String>,
    pub camera_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePrinter3DRequest {
    pub name: Option<String>,
    pub ip: Option<String>,
    pub port: Option<u16>,
    pub printer_type: Option<Printer3DType>,
    pub api_key: Option<Option<String>>,
    pub camera_url: Option<Option<String>>,
    pub section_id: Option<Option<String>>,
    pub power_watts: Option<Option<f64>>,
    pub electricity_cost_kwh: Option<Option<f64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectPrintersResult {
    pub ip: String,
    pub port: u16,
    pub printer_type: Printer3DType,
    pub name: Option<String>,
}

// Nuevos request types para control de impresoras

#[derive(Debug, Deserialize)]
pub struct ControlPrintRequest {
    pub command: String, // "start" | "pause" | "resume" | "cancel"
}

#[derive(Debug, Deserialize)]
pub struct PreheatRequest {
    pub hotend: f64,
    pub bed: f64,
}

#[derive(Debug, Deserialize)]
pub struct HomeRequest {
    #[serde(default)]
    pub axes: Vec<String>, // ["x","y","z"] o vacio para home all
}

#[derive(Debug, Deserialize)]
pub struct JogRequest {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub z: f64,
}

#[derive(Debug, Deserialize)]
pub struct GcodeRequest {
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrinterFileInfo {
    pub name: String,
    pub size: Option<u64>,
    pub date: Option<u64>,
}

// Secciones
#[derive(Debug, Deserialize)]
pub struct AddSectionRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSectionRequest {
    pub name: Option<String>,
    pub order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderPrinterRequest {
    pub section_id: Option<String>,
    pub order: i32,
}

#[derive(Debug, Deserialize)]
pub struct TestHomeRequest {
    pub ip: String,
    pub port: u16,
    pub printer_type: Printer3DType,
    pub api_key: Option<String>,
}
