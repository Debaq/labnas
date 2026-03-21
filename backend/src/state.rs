use crate::config::LabNasConfig;
use crate::models::email::EmailMessage;
use crate::models::network::NetworkHost;
use crate::models::notifications::{UserPermissions, UserRole};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
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

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub username: String,
    pub role: UserRole,
    pub permissions: UserPermissions,
    pub created_at: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub scanned_hosts: Arc<Mutex<Vec<NetworkHost>>>,
    pub start_time: Instant,
    pub config: Arc<Mutex<LabNasConfig>>,
    pub http_client: reqwest::Client,
    pub shutdown: Arc<Notify>,
    pub activity_log: Arc<Mutex<Vec<ActivityEvent>>>,
    pub sessions: Arc<Mutex<HashMap<String, SessionInfo>>>,
    pub link_codes: Arc<Mutex<HashMap<String, LinkCode>>>,
    pub share_links: Arc<Mutex<HashMap<String, ShareLink>>>,
    pub tg_terminals: Arc<Mutex<HashMap<i64, TgTerminal>>>,
    pub email_inbox: Arc<Mutex<HashMap<String, Vec<EmailMessage>>>>,
    pub mdns_service: Arc<Mutex<Option<mdns_sd::ServiceDaemon>>>,
}

pub struct TgTerminal {
    pub stdin: tokio::process::ChildStdin,
    pub output_rx: tokio::sync::mpsc::Receiver<String>,
    pub child: tokio::process::Child,
    pub created_at: Instant,
}

#[derive(Debug, Clone)]
pub struct ShareLink {
    pub file_path: String,
    pub file_name: String,
    pub created_at: Instant,
    pub expires_secs: u64,
}

#[derive(Debug, Clone)]
pub struct LinkCode {
    pub username: String,
    pub created_at: Instant,
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
