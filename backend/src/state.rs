use crate::config::LabNasConfig;
use crate::models::network::NetworkHost;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Notify};

#[derive(Clone)]
pub struct AppState {
    pub scanned_hosts: Arc<Mutex<Vec<NetworkHost>>>,
    pub start_time: Instant,
    pub config: Arc<Mutex<LabNasConfig>>,
    pub http_client: reqwest::Client,
    pub shutdown: Arc<Notify>,
}
