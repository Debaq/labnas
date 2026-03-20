use crate::models::auth::WebUser;
use crate::models::network::KnownDevice;
use crate::models::notes::Note;
use crate::models::notifications::NotificationConfig;
use crate::models::printers3d::Printer3DConfig;
use crate::models::tasks::TasksConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LabNasConfig {
    #[serde(default)]
    pub printers3d: Vec<Printer3DConfig>,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub known_devices: Vec<KnownDevice>,
    #[serde(default)]
    pub web_users: Vec<WebUser>,
    #[serde(default)]
    pub tasks: TasksConfig,
    #[serde(default)]
    pub notes: Vec<Note>,
}

pub fn config_path() -> PathBuf {
    // 1. Explicit env var
    if let Ok(p) = std::env::var("LABNAS_CONFIG") {
        return PathBuf::from(p);
    }

    // 2. Resolve real user's home (even under sudo)
    let home = if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        // Running with sudo: use the original user's home
        format!("/home/{}", sudo_user)
    } else {
        std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
    };

    PathBuf::from(home).join(".labnas").join("config.json")
}

pub async fn load_config() -> LabNasConfig {
    let path = config_path();
    println!("[LabNAS] Config: {}", path.display());
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => {
            let config: LabNasConfig = serde_json::from_str(&contents).unwrap_or_default();
            println!("[LabNAS] Config cargada ({} impresoras 3D, {} chats Telegram, {} dispositivos conocidos, {} usuarios web)",
                config.printers3d.len(),
                config.notifications.telegram_chats.len(),
                config.known_devices.len(),
                config.web_users.len(),
            );
            config
        }
        Err(_) => {
            println!("[LabNAS] No se encontro config, usando valores por defecto");
            LabNasConfig::default()
        }
    }
}

pub async fn save_config(config: &LabNasConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Error creando directorio config: {}", e))?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| format!("Error guardando config en {}: {}", path.display(), e))
}
