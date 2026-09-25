//! Musica: reproductor compartido (mpv + yt-dlp), cola, radio y playlists.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::io::AsyncWriteExt;

use crate::state::AppState;


mod player;
mod playback;
mod screens;
mod discovery;
mod playlists;

// API publica (handlers y loops) y visibilidad entre submodulos (`use super::*`)
#[allow(unused_imports)]
pub use {player::*, playback::*, screens::*, discovery::*, playlists::*};

const MPV_SOCKET: &str = "/tmp/labnas-mpv-socket";

// --- Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub thumbnail: String,
    pub duration: u32,
    #[serde(default)]
    pub added_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub thumbnail: String,
    pub played_by: String,
    #[serde(default)]
    pub duration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MusicState {
    pub current: Option<MusicTrack>,
    pub queue: Vec<MusicTrack>,
    pub started_by: Option<String>,
    pub history: Vec<HistoryEntry>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default = "default_volume")]
    pub volume: u8,
    #[serde(default)]
    pub repeat: RepeatMode,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub video: bool,
    #[serde(default)]
    pub video_screen: Option<u8>,
    /// Segundos transcurridos desde que empezo la cancion actual
    #[serde(default)]
    pub elapsed: u32,
    /// Timestamp (epoch secs) de cuando empezo a reproducir
    #[serde(skip)]
    pub playback_started_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

// --- Playlist types (persisted in config) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub created_by: String,
    pub tracks: Vec<PlaylistTrack>,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistTrack {
    pub id: String, // YouTube ID
    pub title: String,
    pub artist: String,
    pub thumbnail: String,
    pub duration: u32,
    #[serde(default)]
    pub added_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaylistsConfig {
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn default_volume() -> u8 { 80 }

const MAX_HISTORY: usize = 50;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Debug, Deserialize)]
pub struct PlayRequest {
    pub id: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
pub struct QueueRemoveRequest {
    pub index: usize,
}

#[derive(Debug, Deserialize)]
pub struct SetVolumeRequest {
    pub volume: u8,
}

#[derive(Debug, Deserialize)]
pub struct QueueMoveRequest {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Deserialize)]
pub struct SetVideoRequest {
    pub video: bool,
    #[serde(default)]
    pub screen: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct SetLastfmKeyRequest {
    pub key: String,
}

#[derive(Debug, Deserialize)]
pub struct RadioRequest {
    pub artist: String,
    pub track: String,
}

#[derive(Debug, Deserialize)]
pub struct LuckyRequest {
    pub artist: String,
    pub track: String,
}

#[derive(Debug, Serialize)]
pub struct ScreenInfo {
    pub index: u8,
    pub connector: String,
    pub name: String,
    pub connected: bool,
}

// yt-dlp JSON output
#[derive(Debug, Deserialize)]
struct YtDlpEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    uploader: String,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    thumbnail: String,
    #[serde(default)]
    thumbnails: Vec<YtDlpThumb>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YtDlpThumb {
    #[serde(default)]
    url: String,
}

fn extract_track_info(entry: &YtDlpEntry) -> (String, String) {
    let thumb = if !entry.thumbnail.is_empty() {
        entry.thumbnail.clone()
    } else {
        entry.thumbnails.last().map(|t| t.url.clone()).unwrap_or_default()
    };
    let artist = if !entry.uploader.is_empty() {
        entry.uploader.clone()
    } else {
        entry.channel.clone()
    };
    (thumb, artist)
}

fn extract_username(
    sessions: &std::collections::HashMap<String, crate::state::SessionInfo>,
    headers: &axum::http::HeaderMap,
) -> String {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| sessions.get(t).map(|s| s.username.clone()))
        .unwrap_or_else(|| "alguien".to_string())
}
