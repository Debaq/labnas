use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Printer3DType {
    OctoPrint,
    Moonraker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Printer3DConfig {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub printer_type: Printer3DType,
    pub api_key: Option<String>,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectPrintersResult {
    pub ip: String,
    pub port: u16,
    pub printer_type: Printer3DType,
    pub name: Option<String>,
}
