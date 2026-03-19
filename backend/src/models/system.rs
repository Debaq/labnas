use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime: String,
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
