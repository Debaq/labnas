use serde::{Deserialize, Serialize};

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

#[derive(Debug, Deserialize)]
pub struct PrintFileRequest {
    pub path: String,
    pub printer: String,
    pub copies: Option<u32>,
    pub orientation: Option<String>,
    pub double_sided: Option<bool>,
    pub pages: Option<String>,
}
