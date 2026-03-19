use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHost {
    pub ip: String,
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub vendor: Option<String>,
    pub is_alive: bool,
    pub is_known: bool,
    pub label: Option<String>,
    pub last_seen: DateTime<Utc>,
    pub response_time_ms: Option<f64>,
}

// Persisted in config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownDevice {
    pub mac: String,
    pub label: String,
}

#[derive(Debug, Deserialize)]
pub struct LabelRequest {
    pub label: String,
}
