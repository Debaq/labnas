use crate::models::network::KnownDevice;
use crate::models::notifications::NotificationConfig;
use crate::models::printers3d::Printer3DConfig;
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
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home).join(".labnas").join("config.json")
}

pub async fn load_config() -> LabNasConfig {
    let path = config_path();
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => LabNasConfig::default(),
    }
}

pub async fn save_config(config: &LabNasConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| e.to_string())
}
