use axum::{extract::State, http::StatusCode, Json};
use sysinfo::{Disks, System};
use tokio::process::Command;

use crate::models::system::{AutostartStatus, DiskInfo, HealthResponse, SystemInfoResponse};
use crate::state::AppState;

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
        version: "0.2.0".to_string(),
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

    Json(AutostartStatus { installed, enabled })
}

pub async fn install_autostart() -> Result<(StatusCode, String), (StatusCode, String)> {
    let exe_path = std::env::current_exe()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let exe_path = std::fs::canonicalize(&exe_path).unwrap_or(exe_path);
    let work_dir = exe_path
        .parent()
        .unwrap_or(std::path::Path::new("/"))
        .to_string_lossy();

    let service = format!(
        "\
[Unit]
Description=LabNAS - NAS de Laboratorio
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={}
WorkingDirectory={}
Restart=on-failure
RestartSec=5
AmbientCapabilities=CAP_NET_RAW

[Install]
WantedBy=multi-user.target
",
        exe_path.display(),
        work_dir
    );

    tokio::fs::write(SERVICE_PATH, &service).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error escribiendo servicio: {}. Ejecuta con sudo.", e),
        )
    })?;

    Command::new("systemctl")
        .args(["daemon-reload"])
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let output = Command::new("systemctl")
        .args(["enable", "labnas"])
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err((StatusCode::INTERNAL_SERVER_ERROR, err));
    }

    Ok((StatusCode::OK, "Servicio instalado y habilitado".to_string()))
}

pub async fn remove_autostart() -> Result<StatusCode, (StatusCode, String)> {
    let _ = Command::new("systemctl")
        .args(["disable", "labnas"])
        .output()
        .await;

    let _ = tokio::fs::remove_file(SERVICE_PATH).await;

    let _ = Command::new("systemctl")
        .args(["daemon-reload"])
        .output()
        .await;

    Ok(StatusCode::NO_CONTENT)
}
