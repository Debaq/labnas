use crate::models::auth::WebUser;
use crate::models::email::EmailConfig;
use crate::models::network::KnownDevice;
use crate::models::notes::Note;
use crate::models::notifications::NotificationConfig;
use crate::models::printers3d::{Printer3DConfig, Printer3DSection};
use crate::handlers::music::PlaylistsConfig;
use crate::models::inventory::InventoryConfig;
use crate::models::portfolio::PortfolioConfig;
use crate::models::printing::CupsPrinterConfig;
use crate::models::tasks::TasksConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LabNasConfig {
    #[serde(default)]
    pub printers3d: Vec<Printer3DConfig>,
    #[serde(default)]
    pub printer3d_sections: Vec<Printer3DSection>,
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
    #[serde(default)]
    pub email: EmailConfig,
    #[serde(default)]
    pub branding: LabBranding,
    #[serde(default)]
    pub mdns_enabled: bool,
    #[serde(default = "default_mdns_hostname")]
    pub mdns_hostname: String,
    #[serde(default)]
    pub services: Vec<LabService>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lastfm_api_key: Option<String>,
    #[serde(default = "default_mpv_args")]
    pub mpv_extra_args: Vec<String>,
    /// Limite de subida en MB (default 50)
    #[serde(default = "default_upload_limit_mb")]
    pub upload_limit_mb: u32,
    #[serde(default)]
    pub cups_printers: Vec<CupsPrinterConfig>,
    #[serde(default)]
    pub inventory: InventoryConfig,
    #[serde(default)]
    pub playlists: PlaylistsConfig,
    #[serde(default)]
    pub portfolio: PortfolioConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabService {
    pub name: String,
    pub port: u16,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
}

fn default_mpv_args() -> Vec<String> {
    vec!["--af=lavfi=[loudnorm=I=-14:TP=-1:LRA=11]".to_string()]
}

fn default_upload_limit_mb() -> u32 {
    50
}

fn default_mdns_hostname() -> String {
    "labnas".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabBranding {
    #[serde(default = "default_lab_name")]
    pub lab_name: String,
    #[serde(default)]
    pub institution: String,
    #[serde(default)]
    pub logo_url: String,
    #[serde(default)]
    pub mission: String,
    #[serde(default)]
    pub vision: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub contact_email: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub accent_color: String,
}

fn default_lab_name() -> String {
    "LabNAS".to_string()
}

impl Default for LabBranding {
    fn default() -> Self {
        Self {
            lab_name: "LabNAS".to_string(),
            institution: String::new(),
            logo_url: String::new(),
            mission: String::new(),
            vision: String::new(),
            website: String::new(),
            contact_email: String::new(),
            location: String::new(),
            accent_color: String::new(),
        }
    }
}

/// ¿El proceso corre como root?
pub fn is_root() -> bool {
    // SAFETY: geteuid no tiene precondiciones
    unsafe { libc::geteuid() == 0 }
}

/// Usuario con sesion grafica/tty activa (distinto de root), segun `who`
pub fn detect_session_user() -> Option<String> {
    let output = std::process::Command::new("who").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("(:0)") || line.contains("tty") {
            let user = line.split_whitespace().next()?;
            if user != "root" {
                return Some(user.to_string());
            }
        }
    }
    None
}

/// Resuelve el home real del usuario dueño de la instalación.
/// Deriva desde la ubicación del binario para ser consistente
/// sin importar si se ejecuta con sudo, systemd o directamente.
/// Home fijado por los tests de integracion (cada servidor de prueba usa el suyo)
#[cfg(test)]
pub static TEST_HOME: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

pub fn resolve_home() -> String {
    #[cfg(test)]
    if let Some(h) = TEST_HOME.read().ok().and_then(|h| h.clone()) {
        return h;
    }

    // 1. Env var explícita
    if let Ok(h) = std::env::var("LABNAS_HOME") {
        return h;
    }

    // 2. Derivar desde la ruta del binario:
    //    Si el binario está en /home/nick/labnas/labnas-backend
    //    el home es /home/nick
    if let Ok(exe) = crate::updater::exe_path() {
        {
            let mut path = exe.as_path();
            // Subir hasta encontrar /home/usuario
            while let Some(parent) = path.parent() {
                if parent.parent().map(|p| p == std::path::Path::new("/home")).unwrap_or(false) {
                    return parent.to_string_lossy().to_string();
                }
                path = parent;
            }
        }
    }

    // 3. Fallback clásico
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        return format!("/home/{}", sudo_user);
    }
    std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
}

pub fn config_path() -> PathBuf {
    // Override explícito
    if let Ok(p) = std::env::var("LABNAS_CONFIG") {
        return PathBuf::from(p);
    }

    // ~/.labnas/config.json (home resuelto desde ubicación del binario)
    PathBuf::from(resolve_home()).join(".labnas").join("config.json")
}

