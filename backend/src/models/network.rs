use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHost {
    pub ip: String,
    pub hostname: Option<String>,
    pub is_alive: bool,
    pub last_seen: DateTime<Utc>,
    pub response_time_ms: Option<f64>,
}
