use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime: String,
    pub ip: Option<String>,
    pub upload_limit_mb: u32,
    /// "bridge" si el pedido llego por el puente PHP (la UI oculta terminal y admin)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<&'static str>,
    /// Por el puente: bytes maximos por pedido (el frontend sube por partes de este tamaño)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_request: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub used_space: u64,
    pub file_system: String,
    pub is_removable: bool,
}

#[derive(Debug, Serialize)]
pub struct SystemInfoResponse {
    pub hostname: String,
    pub local_ip: String,
    pub os: String,
    pub kernel: String,
    pub total_memory: u64,
    pub used_memory: u64,
    pub cpu_count: usize,
    pub uptime_secs: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AutostartStatus {
    pub installed: bool,
    pub enabled: bool,
    pub install_cmd: String,
    pub uninstall_cmd: String,
}
