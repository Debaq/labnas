use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChat {
    pub chat_id: i64,
    pub name: String,
    pub username: Option<String>,
    #[serde(default)]
    pub daily_enabled: bool,
    #[serde(default = "default_hour")]
    pub daily_hour: u8,
    #[serde(default)]
    pub daily_minute: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub bot_username: Option<String>,
    #[serde(default)]
    pub telegram_chats: Vec<TelegramChat>,
    // Global schedule kept for backward compat / UI toggle
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
            bot_token: None,
            bot_username: None,
            telegram_chats: Vec::new(),
            daily_enabled: false,
            daily_hour: 8,
            daily_minute: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SetBotTokenRequest {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleRequest {
    pub daily_enabled: bool,
    pub daily_hour: u8,
    pub daily_minute: u8,
}

// --- Telegram API types ---

#[derive(Debug, Deserialize)]
pub struct TgResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TgUpdate {
    pub update_id: i64,
    pub message: Option<TgMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TgMessage {
    pub chat: TgChat,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TgChat {
    pub id: i64,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub username: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TgBotInfo {
    pub username: String,
}
