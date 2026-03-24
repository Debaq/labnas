use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MailProtocol {
    #[default]
    Imap,
    Pop3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FilterAction {
    Prioritario,  // siempre notificar, marcar urgente
    Normal,       // clasificar con IA normalmente
    Silencioso,   // clasificar pero no notificar
    Ignorar,      // no procesar, descartarlo
}

impl Default for FilterAction {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailFilter {
    pub pattern: String,       // ej: "@alumnos.uach.cl", "servicios@uach.cl"
    pub action: FilterAction,
    pub label: String,         // ej: "Estudiantes", "Spam interno"
    #[serde(default)]
    pub auto_tag: Option<String>, // tag para la tarea si se crea
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAccount {
    pub username: String,
    #[serde(alias = "imap_host")]
    pub host: String,
    #[serde(alias = "imap_port")]
    pub port: u16,
    #[serde(default)]
    pub protocol: MailProtocol,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub filters: Vec<EmailFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub uid: u32,
    pub from: String,
    pub subject: String,
    pub date: String,
    pub body_preview: String,
    pub ai_classification: Option<String>,
    pub ai_summary: Option<String>,
    pub ai_action: Option<String>,
    #[serde(default)]
    pub filter_label: Option<String>,     // qué filtro le pegó
    #[serde(default)]
    pub filter_action: Option<FilterAction>,
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
