use crate::db::DbPool;
use crate::handlers::music::MusicState;
use crate::models::network::NetworkHost;
use crate::models::notifications::UserRole;
use crate::models::sensors::SensorLatest;
use chrono::Utc;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct ActivityEvent {
    pub id: i64,
    pub timestamp: String,
    pub username: String,
    pub action: String,
    pub details: String,
}

/// Accion de la auditoria para las lecturas y descargas
pub const ACCESS_ACTION: &str = "Acceso";
const ACCESS_DEDUP: std::time::Duration = std::time::Duration::from_secs(10 * 60);

fn access_is_new(user: &str, details: &str) -> bool {
    static SEEN: std::sync::LazyLock<std::sync::Mutex<HashMap<(String, String), Instant>>> =
        std::sync::LazyLock::new(Default::default);
    let Ok(mut seen) = SEEN.lock() else { return true };
    if seen.len() > 5000 {
        seen.retain(|_, t| t.elapsed() < ACCESS_DEDUP);
    }
    let key = (user.to_string(), details.to_string());
    match seen.get(&key) {
        Some(t) if t.elapsed() < ACCESS_DEDUP => false,
        _ => {
            seen.insert(key, Instant::now());
            true
        }
    }
}

/// Duracion de una sesion web
pub const SESSION_TTL_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub username: String,
    pub role: UserRole,
    /// Nivel por modulo, con las excepciones de la persona ya aplicadas
    pub grants: crate::roles::Grants,
    /// Fecha de termino de la cuenta (unix), si tiene
    pub account_expires: Option<i64>,
    /// Unix epoch (segundos); persistido en la tabla `sessions`
    pub created_at: i64,
}

impl SessionInfo {
    pub fn new(username: String, role: UserRole, grants: crate::roles::Grants, account_expires: Option<i64>) -> Self {
        Self { username, role, grants, account_expires, created_at: now_unix() }
    }

    pub fn is_expired(&self) -> bool {
        now_unix() - self.created_at > SESSION_TTL_SECS || crate::roles::is_expired(self.account_expires)
    }

    pub fn is_admin(&self) -> bool {
        self.grants.admin
    }

    pub fn can(&self, module: &str, level: crate::roles::Level) -> bool {
        self.grants.can(module, level)
    }
}

pub fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Intentos fallidos de login por usuario (anti fuerza bruta)
#[derive(Debug, Clone)]
pub struct LoginFailures {
    pub count: u32,
    pub last: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub scanned_hosts: Arc<Mutex<Vec<NetworkHost>>>,
    pub start_time: Instant,
    pub db: DbPool,
    pub http_client: reqwest::Client,
    /// Apagado ordenado: al cancelarse se detienen ambos listeners (3001 y 80)
    pub shutdown: tokio_util::sync::CancellationToken,
    pub sessions: Arc<Mutex<HashMap<String, SessionInfo>>>,
    pub link_codes: Arc<Mutex<HashMap<String, LinkCode>>>,
    pub share_links: Arc<Mutex<HashMap<String, ShareLink>>>,
    pub tg_terminals: Arc<Mutex<HashMap<i64, TgTerminal>>>,
    pub mdns_service: Arc<Mutex<Option<mdns_sd::ServiceDaemon>>>,
    pub music: Arc<Mutex<MusicState>>,
    pub music_process: Arc<Mutex<Option<tokio::process::Child>>>,
    pub update_cache: Arc<Mutex<UpdateCache>>,
    pub sensors: Arc<Mutex<SensorState>>,
    pub enabled_modules: Arc<Mutex<HashSet<String>>>,
    pub login_failures: Arc<Mutex<HashMap<String, LoginFailures>>>,
    /// Tras actualizar: al terminar el apagado ordenado, el proceso se re-ejecuta
    pub restart_requested: Arc<AtomicBool>,
    /// Bus de eventos en tiempo real (WebSocket /api/live)
    pub events: crate::events::EventBus,
    /// Tareas de fondo de los modulos encendidos
    pub module_tasks: crate::module_tasks::ModuleTasks,
}

#[derive(Debug, Clone, Default)]
pub struct SensorState {
    pub latest: HashMap<String, SensorLatest>,
    pub receiver_connected: bool,
    pub receiver_port: Option<String>,
    pub receiver_error: Option<String>,
}

/// Cache de la ultima consulta de actualizacion a GitHub
#[derive(Debug, Clone, Default)]
pub struct UpdateCache {
    pub latest_tag: Option<String>,
    pub download_url: Option<String>,
    pub checked_at: Option<Instant>,
}

pub struct TgTerminal {
    pub stdin: tokio::process::ChildStdin,
    pub output_rx: tokio::sync::mpsc::Receiver<String>,
    pub child: tokio::process::Child,
    pub created_at: Instant,
}

#[derive(Debug, Clone)]
pub struct ShareLink {
    /// Usuario que lo creo: solo el (o un admin) lo ve y lo revoca
    pub owner: String,
    pub file_path: String,
    pub file_name: String,
    pub created_at: Instant,
    pub expires_secs: u64,
}

impl ShareLink {
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() > self.expires_secs
    }
}

#[derive(Debug, Clone)]
pub struct LinkCode {
    pub username: String,
    pub created_at: Instant,
}

impl AppState {
    /// Registro de accesos (idea 72): quien vio o descargo que. El mismo acceso de la misma
    /// persona se anota una vez cada 10 minutos (un cliente WebDAV lee muchas veces).
    pub async fn log_access(&self, details: &str, user: &str) {
        if !access_is_new(user, details) {
            return;
        }
        self.log_activity(ACCESS_ACTION, details, user).await;
    }

    /// Registra un evento en la auditoria (tabla `audit_log`). Nunca falla hacia el caller.
    pub async fn log_activity(&self, action: &str, details: &str, user: &str) {
        let (action, details, user) = (action.to_string(), details.to_string(), user.to_string());
        let res = crate::db::db_op(&self.db, move |conn| {
            conn.execute(
                "INSERT INTO audit_log (timestamp, username, action, details) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![Utc::now().to_rfc3339(), user, action, details],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
        .await;
        if let Err((_, e)) = res {
            eprintln!("[Auditoria] Error registrando evento: {}", e);
        }
    }
}
