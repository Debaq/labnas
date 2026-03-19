use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppContact {
    pub name: String,
    pub phone: String,
    pub apikey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default)]
    pub whatsapp_contacts: Vec<WhatsAppContact>,
    #[serde(default)]
    pub daily_enabled: bool,
    #[serde(default = "default_hour")]
    pub daily_hour: u8,
    #[serde(default)]
    pub daily_minute: u8,
}

fn default_hour() -> u8 {
    8
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            whatsapp_contacts: Vec::new(),
            daily_enabled: false,
            daily_hour: 8,
            daily_minute: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AddContactRequest {
    pub name: String,
    pub phone: String,
    pub apikey: String,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleRequest {
    pub daily_enabled: bool,
    pub daily_hour: u8,
    pub daily_minute: u8,
}
