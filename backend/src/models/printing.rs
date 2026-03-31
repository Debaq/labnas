use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
