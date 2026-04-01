use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryCategory {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemStatus {
    Activo,
    Mantenimiento,
    Retirado,
    Agotado,
}

impl Default for ItemStatus {
    fn default() -> Self {
        Self::Activo
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: String,
    pub category_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub quantity: f64,
    #[serde(default)]
    pub unit: String, // "unidad", "kg", "m", "ml", "g", "rollo"
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub supplier: String,
    #[serde(default)]
    pub supplier_url: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub serial_number: String,
    #[serde(default)]
    pub purchase_date: String,
    #[serde(default)]
    pub status: ItemStatus,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub model_name: String,
    // Filament-specific fields
    #[serde(default)]
    pub material_type: Option<String>, // PLA, ABS, PETG, TPU, Nylon...
    #[serde(default)]
    pub filament_diameter: Option<f64>, // 1.75, 2.85
    #[serde(default)]
    pub filament_weight: Option<f64>, // total grams
    #[serde(default)]
    pub filament_remaining: Option<f64>, // grams remaining
    #[serde(default)]
    pub filament_color: Option<String>,
    #[serde(default)]
    pub filament_density: Option<f64>, // g/cm³ (PLA=1.24, ABS=1.04, PETG=1.27)
    // Custom fields
    #[serde(default)]
    pub custom_fields: HashMap<String, String>,
    #[serde(default = "chrono::Utc::now")]
    pub created_at: DateTime<Utc>,
}

/// Print history entry for 3D printers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintHistoryEntry {
    pub id: String,
    pub printer_id: String,
    pub file_name: String,
    pub filament_id: Option<String>, // reference to inventory item
    #[serde(default)]
    pub weight_grams: f64,
    #[serde(default)]
    pub print_time_minutes: u64,
    #[serde(default)]
    pub material_cost: f64,
    #[serde(default)]
    pub electricity_cost: f64,
    #[serde(default)]
    pub total_cost: f64,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "chrono::Utc::now")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InventoryConfig {
    #[serde(default)]
    pub categories: Vec<InventoryCategory>,
    #[serde(default)]
    pub items: Vec<InventoryItem>,
    #[serde(default)]
    pub print_history: Vec<PrintHistoryEntry>,
}
