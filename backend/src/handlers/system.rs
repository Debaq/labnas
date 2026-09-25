use axum::{extract::{Path, State}, http::StatusCode, Json};
use rusqlite::params;
use std::time::Duration;
use sysinfo::{Disks, System};
use tokio::process::Command;

use crate::models::system::{AutostartStatus, DiskInfo, HealthResponse, SystemInfoResponse};
use crate::state::AppState;

use crate::updater::CURRENT_VERSION;

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
        shutdown.cancel();
    });
    "Apagando LabNAS..."
}

/// Apagado ordenado seguido de re-ejecucion del binario (ya actualizado).
/// No depende de systemd ni de root: main() hace exec() al terminar.
fn request_restart(state: &AppState) {
    state.restart_requested.store(true, std::sync::atomic::Ordering::SeqCst);
    let shutdown = state.shutdown.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        shutdown.cancel();
    });
}

pub async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let uptime = state.start_time.elapsed();
    let secs = uptime.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let uptime_str = format!("{}h {}m {}s", hours, mins, secs % 60);

    let ip = local_ip_address::local_ip().ok().map(|ip| ip.to_string());
    let upload_limit_mb = crate::db::db_op(&state.db, |conn| {
        Ok(crate::db::get_setting_u32(conn, "upload_limit_mb", 50))
    }).await.unwrap_or(50);

    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: uptime_str,
        ip,
        upload_limit_mb,
    })
}

/// Filesystems virtuales/irrelevantes que se filtran
const SKIP_FS: &[&str] = &[
    "tmpfs", "devtmpfs", "squashfs", "overlay", "efivarfs",
    "proc", "sysfs", "devpts", "securityfs", "cgroup", "cgroup2",
    "pstore", "bpf", "tracefs", "debugfs", "hugetlbfs", "mqueue",
    "configfs", "fusectl", "ramfs", "fuse.portal",
];

const SKIP_MOUNTS: &[&str] = &[
    "/boot", "/boot/efi", "/snap", "/run", "/dev",
];

pub async fn storage_info() -> Result<Json<Vec<DiskInfo>>, (StatusCode, String)> {
    let disks_info = tokio::task::spawn_blocking(|| {
        let disks = Disks::new_with_refreshed_list();
        let mut result = Vec::new();
        let mut seen_devices = std::collections::HashSet::new();

        for disk in disks.list() {
            let fs = String::from_utf8_lossy(disk.file_system().as_encoded_bytes()).to_string();
            let mount = disk.mount_point().to_string_lossy().to_string();
            let name = disk.name().to_string_lossy().to_string();

            if SKIP_FS.iter().any(|s| fs == *s) { continue; }
            if SKIP_MOUNTS.iter().any(|s| mount.starts_with(s)) { continue; }

            let total = disk.total_space();
            if total == 0 { continue; }

            // Deduplicar: mismo dispositivo (btrfs subvolúmenes, etc.)
            // Usar nombre del dispositivo como clave; preferir montaje en /
            if seen_devices.contains(&name) { continue; }
            seen_devices.insert(name.clone());

            let available = disk.available_space();
            result.push(DiskInfo {
                name,
                mount_point: mount,
                total_space: total,
                available_space: available,
                used_space: total.saturating_sub(available),
                file_system: fs,
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

/// Nombre del usuario dueño de un archivo (via /etc/passwd)
fn owner_name(path: &std::path::Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata(path).ok()?.uid();
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|l| {
        let mut f = l.split(':');
        let name = f.next()?;
        let _pw = f.next()?;
        (f.next()?.parse::<u32>().ok()? == uid).then(|| name.to_string())
    })
}

/// Unidad systemd: corre como el dueño de la instalacion (no root), con capacidades
/// solo para ping ICMP (escaner de red) y puerto 80.
fn build_autostart_commands() -> (String, String) {
    let exe_path = crate::updater::exe_path().unwrap_or_default();
    let work_dir = exe_path
        .parent()
        .unwrap_or(std::path::Path::new("/"))
        .to_string_lossy()
        .to_string();
    let user = owner_name(&exe_path)
        .filter(|u| u != "root")
        .or_else(crate::config::detect_session_user)
        .unwrap_or_else(|| "CAMBIAR_USUARIO".to_string());

    let install_cmd = format!(
        "cat > /tmp/labnas.service << 'EOF'\n\
         [Unit]\n\
         Description=LabNAS - NAS de Laboratorio\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         User={user}\n\
         ExecStart={exe}\n\
         WorkingDirectory={dir}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         AmbientCapabilities=CAP_NET_RAW CAP_NET_BIND_SERVICE\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n\
         EOF\n\
         sudo chown -R {user}: {dir} && sudo cp /tmp/labnas.service /etc/systemd/system/ && sudo systemctl daemon-reload && sudo systemctl enable labnas && echo 'LabNAS configurado para iniciar con el sistema (usuario {user})'",
        user = user,
        exe = exe_path.display(),
        dir = work_dir,
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
    // Usar cache si fue consultado hace menos de 30 minutos
    let cache = state.update_cache.lock().await;
    let use_cache = cache.checked_at
        .map(|t| t.elapsed() < Duration::from_secs(30 * 60))
        .unwrap_or(false);

    if use_cache {
        let update_available = cache.latest_tag.as_ref()
            .map(|v| is_newer_version(v, CURRENT_VERSION))
            .unwrap_or(false);
        return Json(UpdateStatus {
            current_version: CURRENT_VERSION.to_string(),
            latest_version: cache.latest_tag.clone(),
            update_available,
            download_url: cache.download_url.clone(),
        });
    }
    drop(cache);

    let (latest, url) = fetch_latest_release(&state.http_client).await;
    let update_available = latest.as_ref().map(|v| is_newer_version(v, CURRENT_VERSION)).unwrap_or(false);

    // Guardar en cache
    if latest.is_some() {
        let mut cache = state.update_cache.lock().await;
        cache.latest_tag = latest.clone();
        cache.download_url = url.clone();
        cache.checked_at = Some(std::time::Instant::now());
    }

    Json(UpdateStatus {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest,
        update_available,
        download_url: url,
    })
}

/// POST /api/system/update/force-check - Forzar verificacion ignorando cache
/// POST /api/system/upload-limit
pub async fn set_upload_limit(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    let limit = req["limit_mb"].as_u64().unwrap_or(50) as u32;
    if limit == 0 || limit > 500 {
        return Err((StatusCode::BAD_REQUEST, "Limite debe ser entre 1 y 500 MB".to_string()));
    }
    let limit_str = limit.to_string();
    crate::db::db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, "upload_limit_mb", &limit_str)
    }).await?;
    Ok(StatusCode::OK)
}

pub async fn force_check_update(
    State(state): State<AppState>,
) -> Json<UpdateStatus> {
    // Limpiar cache
    {
        let mut cache = state.update_cache.lock().await;
        cache.checked_at = None;
    }
    // Reusar check_update que ahora no tendra cache
    check_update(State(state)).await
}

pub async fn do_update(
    State(state): State<AppState>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let release = crate::updater::fetch_release(&state.http_client, None).await
        .ok_or((StatusCode::BAD_GATEWAY, "No se pudo consultar GitHub".to_string()))?;
    let assets = crate::updater::assets_for(&release)
        .ok_or((StatusCode::NOT_FOUND, "El ultimo release no tiene binario para esta arquitectura".to_string()))?;

    crate::updater::install(&state.http_client, &assets).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    state.log_activity("Actualizacion", &format!("v{} -> {}", CURRENT_VERSION, assets.tag), "sistema").await;
    request_restart(&state);

    Ok((StatusCode::OK, format!("Actualizado a {}. Reiniciando...", assets.tag)))
}

/// POST /api/system/reinstall - Reinstala la versión actual
pub async fn reinstall(
    State(state): State<AppState>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let tag = format!("v{}", CURRENT_VERSION);
    let release = crate::updater::fetch_release(&state.http_client, Some(&tag)).await
        .ok_or((StatusCode::NOT_FOUND, format!("No se encontro release para {}", tag)))?;
    let assets = crate::updater::assets_for(&release)
        .ok_or((StatusCode::NOT_FOUND, format!("El release {} no tiene binario para esta arquitectura", tag)))?;

    crate::updater::install(&state.http_client, &assets).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    state.log_activity("Reinstalacion", &tag, "sistema").await;
    request_restart(&state);

    Ok((StatusCode::OK, format!("Reinstalado {}. Reiniciando...", tag)))
}

#[derive(serde::Serialize)]
pub struct RollbackStatus {
    pub available: bool,
    pub version: Option<String>,
}

/// GET /api/system/rollback - ¿Hay una version anterior guardada?
pub async fn rollback_status() -> Json<RollbackStatus> {
    let version = crate::updater::rollback_version();
    Json(RollbackStatus { available: version.is_some(), version })
}

/// POST /api/system/rollback - Vuelve a la version anterior (repetirlo vuelve a la nueva)
pub async fn do_rollback(
    State(state): State<AppState>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let restored = tokio::task::spawn_blocking(crate::updater::swap_rollback).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    state.log_activity("Rollback", &format!("v{} -> v{}", CURRENT_VERSION, restored), "sistema").await;
    request_restart(&state);

    Ok((StatusCode::OK, format!("Restaurada v{}. Reiniciando...", restored)))
}

async fn fetch_latest_release(client: &reqwest::Client) -> (Option<String>, Option<String>) {
    match crate::updater::fetch_release(client, None).await {
        Some(json) => (
            json["tag_name"].as_str().map(|s| s.to_string()),
            crate::updater::assets_for(&json).map(|a| a.tarball_url),
        ),
        None => (None, None),
    }
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

            // Read token and admin chats from DB (sync scope, no await)
            let tg_data: Option<(String, Vec<i64>)> = {
                let conn = match crate::db::get_conn(&state.db) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let token: Option<String> = conn.query_row(
                    "SELECT bot_token FROM notification_config WHERE id = 1",
                    [],
                    |row| row.get(0),
                ).ok().flatten();

                token.map(|t| {
                    let mut stmt = conn.prepare(
                        "SELECT chat_id FROM telegram_chats WHERE role = 'admin'"
                    ).unwrap();
                    let chat_ids: Vec<i64> = stmt.query_map([], |row| row.get(0))
                        .unwrap()
                        .filter_map(|r| r.ok())
                        .collect();
                    (t, chat_ids)
                })
            };

            // Now send notifications (async, conn already dropped)
            if let Some((token, chat_ids)) = tg_data {
                let msg = format!(
                    "*Actualizacion disponible*\n\nActual: `{}`\nNueva: `{}`\n\nActualiza desde Configuracion en la web.",
                    current, latest
                );
                for chat_id in &chat_ids {
                    let _ = crate::handlers::notifications::send_tg_public(
                        &state.http_client, &token, *chat_id, &msg
                    ).await;
                }
            }
        }
    }
}

// --- Branding ---

pub async fn get_branding(State(state): State<AppState>) -> Json<crate::config::LabBranding> {
    let branding = crate::db::db_op(&state.db, |conn| {
        let result = conn.query_row(
            "SELECT lab_name, institution, logo_url, mission, vision, website, contact_email, location, accent_color FROM branding WHERE id = 1",
            [],
            |row| {
                Ok(crate::config::LabBranding {
                    lab_name: row.get(0)?,
                    institution: row.get(1)?,
                    logo_url: row.get(2)?,
                    mission: row.get(3)?,
                    vision: row.get(4)?,
                    website: row.get(5)?,
                    contact_email: row.get(6)?,
                    location: row.get(7)?,
                    accent_color: row.get(8)?,
                })
            },
        );
        match result {
            Ok(b) => Ok(b),
            Err(_) => Ok(crate::config::LabBranding::default()),
        }
    }).await.unwrap_or_else(|_| crate::config::LabBranding::default());

    Json(branding)
}

pub async fn set_branding(
    State(state): State<AppState>,
    Json(req): Json<crate::config::LabBranding>,
) -> Result<Json<crate::config::LabBranding>, (StatusCode, String)> {
    let branding = req.clone();
    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO branding (id, lab_name, institution, logo_url, mission, vision, website, contact_email, location, accent_color)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                branding.lab_name, branding.institution, branding.logo_url,
                branding.mission, branding.vision, branding.website,
                branding.contact_email, branding.location, branding.accent_color,
            ],
        ).map_err(|e| format!("Error guardando branding: {}", e))?;
        Ok(())
    }).await?;
    Ok(Json(req))
}

// --- mDNS ---

#[derive(serde::Serialize)]
pub struct MdnsStatus {
    pub enabled: bool,
    pub hostname: String,
    pub url: String,
}

pub async fn get_mdns_status(State(state): State<AppState>) -> Json<MdnsStatus> {
    let (enabled, hostname) = crate::db::db_op(&state.db, |conn| {
        let enabled = crate::db::get_setting_bool(conn, "mdns_enabled");
        let hostname = crate::db::get_setting(conn, "mdns_hostname")
            .unwrap_or_else(|| "labnas".to_string());
        let hostname = if hostname.is_empty() { "labnas".to_string() } else { hostname };
        Ok((enabled, hostname))
    }).await.unwrap_or((false, "labnas".to_string()));

    Json(MdnsStatus {
        enabled,
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
    let enabled = req.enabled;
    let clean_hostname = req.hostname.map(|h| {
        h.trim().to_lowercase()
            .chars().filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>()
    });

    let hostname = crate::db::db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, "mdns_enabled", if enabled { "true" } else { "false" })?;
        if let Some(ref h) = clean_hostname {
            if !h.is_empty() {
                crate::db::set_setting(conn, "mdns_hostname", h)?;
                return Ok(h.clone());
            }
        }
        Ok(crate::db::get_setting(conn, "mdns_hostname").unwrap_or_else(|| "labnas".to_string()))
    }).await?;

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

// --- Servicios del lab ---

pub async fn get_services(State(state): State<AppState>) -> Json<Vec<crate::config::LabService>> {
    let services = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare("SELECT port, name, description, icon FROM services ORDER BY port")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::config::LabService {
                port: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                icon: row.get(3)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await.unwrap_or_default();

    Json(services)
}

pub async fn add_service(
    State(state): State<AppState>,
    Json(req): Json<crate::config::LabService>,
) -> Result<Json<Vec<crate::config::LabService>>, (StatusCode, String)> {
    let new_svc = req.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        // Check if port already exists
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM services WHERE port = ?1",
            params![new_svc.port],
            |row| row.get(0),
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if exists {
            return Err((StatusCode::CONFLICT, "Ya existe un servicio en ese puerto".to_string()));
        }
        conn.execute(
            "INSERT INTO services (port, name, description, icon) VALUES (?1, ?2, ?3, ?4)",
            params![new_svc.port, new_svc.name, new_svc.description, new_svc.icon],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(())
    }).await?;

    // Return updated list
    let services = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn.prepare("SELECT port, name, description, icon FROM services ORDER BY port")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::config::LabService {
                port: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                icon: row.get(3)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }).await?;

    Ok(Json(services))
}

pub async fn update_service(
    State(state): State<AppState>,
    Path(port): Path<u16>,
    Json(req): Json<crate::config::LabService>,
) -> Result<Json<crate::config::LabService>, (StatusCode, String)> {
    let updated = req.clone();
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute(
            "UPDATE services SET name = ?1, port = ?2, description = ?3, icon = ?4 WHERE port = ?5",
            params![updated.name, updated.port, updated.description, updated.icon, port],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Servicio no encontrado".to_string()));
        }
        Ok(())
    }).await?;
    Ok(Json(req))
}

pub async fn delete_service(
    State(state): State<AppState>,
    Path(port): Path<u16>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op_status(&state.db, move |conn| {
        let changed = conn.execute(
            "DELETE FROM services WHERE port = ?1",
            params![port],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Servicio no encontrado".to_string()));
        }
        Ok(())
    }).await?;
    Ok(StatusCode::NO_CONTENT)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparacion_de_versiones() {
        assert!(is_newer_version("v2.10.0", "2.9.9"));
        assert!(is_newer_version("v3.0.0", "2.99.99"));
        assert!(!is_newer_version("v2.8.2", "2.8.2"));
        assert!(!is_newer_version("v2.8.1", "2.8.2"));
        assert!(!is_newer_version("basura", "2.8.2"));
    }
}
