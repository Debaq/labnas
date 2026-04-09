use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct CupsPrinter {
    pub name: String,
    pub description: String,
    pub is_default: bool,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CupsPrintJob {
    pub id: String,
    pub printer: String,
    pub title: String,
    pub state: String,
    pub size: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrinterOption {
    pub key: String,
    pub display_name: String,
    pub default_value: String,
    pub values: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PrintFileRequest {
    pub path: String,
    pub printer: String,
    pub copies: Option<u32>,
    pub pages: Option<String>,
    #[serde(default)]
    pub options: HashMap<String, String>,
}

// ── Costos y estadísticas por impresora ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CupsPrinterConfig {
    pub name: String,
    #[serde(default)]
    pub costs: PrinterCosts,
    #[serde(default)]
    pub stats: PrinterStats,
    /// Stats por usuario: username -> PrinterStats
    #[serde(default)]
    pub user_stats: BTreeMap<String, PrinterStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrinterCosts {
    /// Costo de tinta/toner por página
    #[serde(default)]
    pub ink_per_page: f64,
    /// Costo papel normal (carta/A4)
    #[serde(default)]
    pub paper_carta: f64,
    /// Costo papel oficio (legal)
    #[serde(default)]
    pub paper_oficio: f64,
    /// Costo papel especial (foto, etiquetas, etc.)
    #[serde(default)]
    pub paper_special: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrinterStats {
    #[serde(default)]
    pub total_jobs: u64,
    #[serde(default)]
    pub total_pages: u64,
    #[serde(default)]
    pub pages_carta: u64,
    #[serde(default)]
    pub pages_oficio: u64,
    #[serde(default)]
    pub pages_special: u64,
}

/// Respuesta combinada de costos + stats + costo estimado
#[derive(Debug, Clone, Serialize)]
pub struct PrinterStatsResponse {
    pub costs: PrinterCosts,
    pub stats: PrinterStats,
    pub estimated_cost: f64,
}

/// Stats de un usuario en una impresora con costo calculado
#[derive(Debug, Clone, Serialize)]
pub struct UserPrinterStats {
    pub printer: String,
    pub stats: PrinterStats,
    pub estimated_cost: f64,
}

/// Costos de un usuario (totales + desglose por impresora)
#[derive(Debug, Clone, Serialize)]
pub struct UserCostsResponse {
    pub username: String,
    pub total_cost: f64,
    pub total_jobs: u64,
    pub total_pages: u64,
    pub printers: Vec<UserPrinterStats>,
}

/// Respuesta completa: todos los usuarios o solo el actual
#[derive(Debug, Clone, Serialize)]
pub struct AllUserCostsResponse {
    pub users: Vec<UserCostsResponse>,
    pub general_cost: f64,
    pub general_jobs: u64,
    pub general_pages: u64,
}

// ── Impresion duplex manual ──

/// Respuesta al preparar un archivo para impresion duplex
#[derive(Debug, Clone, Serialize)]
pub struct DuplexPrepareResponse {
    pub temp_id: String,
    pub filename: String,
    pub page_count: u64,
}

/// Solicitud para ejecutar un paso de impresion duplex
#[derive(Debug, Deserialize)]
pub struct DuplexPrintStepRequest {
    pub temp_id: String,
    pub printer: String,
    pub copies: Option<u32>,
    /// "odd" o "even"
    pub page_set: String,
    /// "reverse" para paginas pares
    pub output_order: Option<String>,
    #[serde(default)]
    pub options: HashMap<String, String>,
}
