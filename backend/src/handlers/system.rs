use axum::{extract::State, http::StatusCode, Json};
use std::time::Duration;
use sysinfo::{Disks, System};
use tokio::process::Command;

use crate::models::system::{AutostartStatus, DiskInfo, HealthResponse, SystemInfoResponse};
use crate::state::AppState;

const GITHUB_REPO: &str = "Debaq/labnas";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    let v = v.strip_prefix('v').unwrap_or(v);
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() == 3 {
        Some((parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?))
    } else {
        None
    }
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

pub async fn shutdown_handler(State(state): State<AppState>) -> &'static str {
    let shutdown = state.shutdown.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        shutdown.notify_one();
    });
    "Apagando LabNAS..."
}

pub async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let uptime = state.start_time.elapsed();
    let secs = uptime.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let uptime_str = format!("{}h {}m {}s", hours, mins, secs % 60);

    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: uptime_str,
    })
}

pub async fn storage_info() -> Result<Json<Vec<DiskInfo>>, (StatusCode, String)> {
    let disks_info = tokio::task::spawn_blocking(|| {
        let disks = Disks::new_with_refreshed_list();
        let mut result = Vec::new();
        for disk in disks.list() {
            let total = disk.total_space();
            let available = disk.available_space();
            result.push(DiskInfo {
                name: disk.name().to_string_lossy().to_string(),
                mount_point: disk.mount_point().to_string_lossy().to_string(),
                total_space: total,
                available_space: available,
                used_space: total.saturating_sub(available),
                file_system: String::from_utf8_lossy(disk.file_system().as_encoded_bytes())
                    .to_string(),
                is_removable: disk.is_removable(),
            });
        }
        result
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(disks_info))
}

pub async fn system_disks() -> Result<Json<Vec<DiskInfo>>, (StatusCode, String)> {
    storage_info().await
}

pub async fn system_info_handler() -> Result<Json<SystemInfoResponse>, (StatusCode, String)> {
    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "desconocido".to_string());

    let info = tokio::task::spawn_blocking(move || {
        let mut sys = System::new_all();
        sys.refresh_all();

        SystemInfoResponse {
            hostname: System::host_name().unwrap_or_else(|| "desconocido".to_string()),
            local_ip,
            os: System::long_os_version().unwrap_or_else(|| "desconocido".to_string()),
            kernel: System::kernel_version().unwrap_or_else(|| "desconocido".to_string()),
            total_memory: sys.total_memory(),
            used_memory: sys.used_memory(),
            cpu_count: sys.cpus().len(),
            uptime_secs: System::uptime(),
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(info))
}

// --- Autostart ---

const SERVICE_PATH: &str = "/etc/systemd/system/labnas.service";

fn build_autostart_commands() -> (String, String) {
    let exe_path = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(&p).ok())
        .unwrap_or_default();
    let work_dir = exe_path
        .parent()
        .unwrap_or(std::path::Path::new("/"))
        .to_string_lossy();

    let install_cmd = format!(
        "cat > /tmp/labnas.service << 'EOF'\n\
         [Unit]\n\
         Description=LabNAS - NAS de Laboratorio\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={}\n\
         WorkingDirectory={}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         AmbientCapabilities=CAP_NET_RAW\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n\
         EOF\n\
         sudo cp /tmp/labnas.service /etc/systemd/system/ && sudo systemctl daemon-reload && sudo systemctl enable labnas && echo 'LabNAS configurado para iniciar con el sistema'",
        exe_path.display(),
        work_dir
    );

    let uninstall_cmd =
        "sudo systemctl disable labnas && sudo rm -f /etc/systemd/system/labnas.service && sudo systemctl daemon-reload && echo 'LabNAS removido del inicio'"
            .to_string();

    (install_cmd, uninstall_cmd)
}

pub async fn autostart_status() -> Json<AutostartStatus> {
    let installed = tokio::fs::metadata(SERVICE_PATH).await.is_ok();

    let enabled = if installed {
        Command::new("systemctl")
            .args(["is-enabled", "labnas"])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        false
    };

    let (install_cmd, uninstall_cmd) = build_autostart_commands();

    Json(AutostartStatus {
        installed,
        enabled,
        install_cmd,
        uninstall_cmd,
    })
}

// --- Auto-update ---

#[derive(serde::Serialize)]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub download_url: Option<String>,
}

pub async fn check_update(
    State(state): State<AppState>,
) -> Json<UpdateStatus> {
    let (latest, url) = fetch_latest_release(&state.http_client).await;
    let update_available = latest.as_ref().map(|v| is_newer_version(v, CURRENT_VERSION)).unwrap_or(false);

    Json(UpdateStatus {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest,
        update_available,
        download_url: url,
    })
}

pub async fn do_update(
    State(state): State<AppState>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let (latest, url) = fetch_latest_release(&state.http_client).await;

    let url = url.ok_or((StatusCode::NOT_FOUND, "No se encontro release".to_string()))?;
    let latest = latest.unwrap_or_default();

    // Get current binary path
    let exe_path = std::env::current_exe()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let install_dir = exe_path.parent()
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No se pudo determinar directorio".to_string()))?;

    let tmp_dir = format!("/tmp/labnas-update-{}", uuid::Uuid::new_v4());

    // Download
    let resp = state.http_client.get(&url)
        .timeout(Duration::from_secs(120))
        .send().await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Error descargando: {}", e)))?;

    if !resp.status().is_success() {
        return Err((StatusCode::BAD_REQUEST, format!("GitHub respondio {}", resp.status())));
    }

    let bytes = resp.bytes().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error leyendo: {}", e)))?;

    // Save tarball
    tokio::fs::create_dir_all(&tmp_dir).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let tarball = format!("{}/labnas.tar.gz", tmp_dir);
    tokio::fs::write(&tarball, &bytes).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Extract
    let output = Command::new("tar")
        .args(["xzf", &tarball, "-C", &tmp_dir])
        .output().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "Error extrayendo tarball".to_string()));
    }

    // Copy new files over current installation
    let extracted = format!("{}/labnas", tmp_dir);
    let copy_result = Command::new("cp")
        .args(["-rf", &format!("{}/.", extracted), &install_dir.to_string_lossy()])
        .output().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !copy_result.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "Error copiando archivos".to_string()));
    }

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    state.log_activity("Actualizacion", &format!("Actualizado a {}", latest), "sistema").await;

    // Try to restart via systemd
    let _ = Command::new("systemctl")
        .args(["restart", "labnas"])
        .output().await;

    Ok((StatusCode::OK, format!("Actualizado a {}. Reiniciando...", latest)))
}

async fn fetch_latest_release(client: &reqwest::Client) -> (Option<String>, Option<String>) {
    let url = format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO);

    let resp = client.get(&url)
        .header("User-Agent", "LabNAS")
        .timeout(Duration::from_secs(10))
        .send().await;

    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        _ => return (None, None),
    };

    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        _ => return (None, None),
    };

    let tag = json["tag_name"].as_str().map(|s| s.to_string());

    // Find the linux x86_64 asset
    let download_url = json["assets"].as_array()
        .and_then(|assets| {
            assets.iter().find(|a| {
                a["name"].as_str()
                    .map(|n| n.contains("linux") && n.contains("x86_64") && n.ends_with(".tar.gz"))
                    .unwrap_or(false)
            })
        })
        .and_then(|a| a["browser_download_url"].as_str().map(|s| s.to_string()));

    (tag, download_url)
}

pub async fn update_check_loop(state: AppState) {
    // Check every 6 hours
    loop {
        tokio::time::sleep(Duration::from_secs(6 * 3600)).await;

        let (latest, _url) = fetch_latest_release(&state.http_client).await;

        let Some(latest) = latest else { continue };
        let current = format!("v{}", CURRENT_VERSION);

        if is_newer_version(&latest, CURRENT_VERSION) {
            println!("[LabNAS] Nueva version disponible: {} (actual: {})", latest, current);

            // Notify admins via Telegram
            let config = state.config.lock().await;
            let token = config.notifications.bot_token.clone();
            let chats = config.notifications.telegram_chats.clone();
            drop(config);

            if let Some(token) = token {
                let msg = format!(
                    "*Actualizacion disponible*\n\nActual: `{}`\nNueva: `{}`\n\nActualiza desde Configuracion en la web.",
                    current, latest
                );
                for chat in &chats {
                    if chat.role == crate::models::notifications::UserRole::Admin {
                        let _ = crate::handlers::notifications::send_tg_public(
                            &state.http_client, &token, chat.chat_id, &msg
                        ).await;
                    }
                }
            }
        }
    }
}

// --- Branding ---

pub async fn get_branding(State(state): State<AppState>) -> Json<crate::config::LabBranding> {
    let config = state.config.lock().await;
    Json(config.branding.clone())
}

pub async fn set_branding(
    State(state): State<AppState>,
    Json(req): Json<crate::config::LabBranding>,
) -> Result<Json<crate::config::LabBranding>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    config.branding = req;
    crate::config::save_config(&config).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let branding = config.branding.clone();
    Ok(Json(branding))
}

// --- mDNS ---

#[derive(serde::Serialize)]
pub struct MdnsStatus {
    pub enabled: bool,
    pub hostname: String,
    pub url: String,
}

pub async fn get_mdns_status(State(state): State<AppState>) -> Json<MdnsStatus> {
    let config = state.config.lock().await;
    let hostname = if config.mdns_hostname.is_empty() {
        "labnas".to_string()
    } else {
        config.mdns_hostname.clone()
    };
    Json(MdnsStatus {
        enabled: config.mdns_enabled,
        hostname: hostname.clone(),
        url: format!("http://{}.local:3001", hostname),
    })
}

#[derive(serde::Deserialize)]
pub struct SetMdnsRequest {
    pub enabled: bool,
    #[serde(default)]
    pub hostname: Option<String>,
}

pub async fn set_mdns(
    State(state): State<AppState>,
    Json(req): Json<SetMdnsRequest>,
) -> Result<Json<MdnsStatus>, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    config.mdns_enabled = req.enabled;
    if let Some(hostname) = req.hostname {
        let clean = hostname.trim().to_lowercase()
            .chars().filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();
        if !clean.is_empty() {
            config.mdns_hostname = clean;
        }
    }
    let hostname = config.mdns_hostname.clone();
    let enabled = config.mdns_enabled;

    crate::config::save_config(&config).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    drop(config);

    // Restart mDNS service
    let mut mdns = state.mdns_service.lock().await;
    // Stop existing
    if let Some(svc) = mdns.take() {
        let _ = svc.shutdown();
    }
    // Start new if enabled
    if enabled {
        match start_mdns_service(&hostname) {
            Ok(svc) => {
                println!("[mDNS] Activo: http://{}.local:3001", hostname);
                *mdns = Some(svc);
            }
            Err(e) => {
                eprintln!("[mDNS] Error: {}", e);
            }
        }
    } else {
        println!("[mDNS] Desactivado");
    }

    Ok(Json(MdnsStatus {
        enabled,
        hostname: hostname.clone(),
        url: format!("http://{}.local:3001", hostname),
    }))
}

pub fn start_mdns_service(hostname: &str) -> Result<mdns_sd::ServiceDaemon, String> {
    let mdns = mdns_sd::ServiceDaemon::new()
        .map_err(|e| format!("Error creando mDNS daemon: {}", e))?;

    let service_type = "_http._tcp.local.";
    let instance_name = hostname;

    let local_ip = local_ip_address::local_ip()
        .map_err(|e| format!("Error obteniendo IP: {}", e))?;

    let service_info = mdns_sd::ServiceInfo::new(
        service_type,
        instance_name,
        &format!("{}.local.", hostname),
        local_ip,
        3001,
        None,
    )
    .map_err(|e| format!("Error creando servicio: {}", e))?;

    mdns.register(service_info)
        .map_err(|e| format!("Error registrando: {}", e))?;

    Ok(mdns)
}
