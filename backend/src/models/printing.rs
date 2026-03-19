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
