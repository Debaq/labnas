use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAccount {
    pub username: String,   // web user que lo configuro
    pub imap_host: String,  // ej: "outlook.office365.com"
    pub imap_port: u16,     // 993
    pub email: String,      // correo
    pub password: String,   // app password
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub uid: u32,
    pub from: String,
    pub subject: String,
    pub date: String,
    pub body_preview: String,          // primeros 500 chars del body
    pub ai_classification: Option<String>, // urgente/tarea/informativo/spam
    pub ai_summary: Option<String>,
    pub ai_action: Option<String>,     // accion sugerida
    pub processed: bool,
    pub task_created: bool,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmailConfig {
    #[serde(default)]
    pub accounts: Vec<EmailAccount>,
    #[serde(default)]
    pub groq_api_key: Option<String>,
}
