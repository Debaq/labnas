use crate::config::LabNasConfig;
use crate::models::network::NetworkHost;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Notify};

const MAX_ACTIVITY_LOG: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct ActivityEvent {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub details: String,
    pub user: String,
}

#[derive(Clone)]
pub struct AppState {
    pub scanned_hosts: Arc<Mutex<Vec<NetworkHost>>>,
    pub start_time: Instant,
    pub config: Arc<Mutex<LabNasConfig>>,
    pub http_client: reqwest::Client,
    pub shutdown: Arc<Notify>,
    pub activity_log: Arc<Mutex<Vec<ActivityEvent>>>,
}

impl AppState {
    pub async fn log_activity(&self, action: &str, details: &str, user: &str) {
        let mut log = self.activity_log.lock().await;
        log.push(ActivityEvent {
            timestamp: Utc::now(),
            action: action.to_string(),
            details: details.to_string(),
            user: user.to_string(),
        });
        // Keep only last N events
        if log.len() > MAX_ACTIVITY_LOG {
            let drain = log.len() - MAX_ACTIVITY_LOG;
            log.drain(..drain);
        }
    }
}
