//! Salud de discos (SMART). Desactivado por defecto.
//!
//! LabNAS no corre como root: lee SMART con `sudo -n /usr/local/lib/labnas/smart-read`,
//! un wrapper de solo lectura que valida el disco y solo ejecuta `smartctl -j -a`
//! (lo instala setup-smart.sh junto con una regla de sudo limitada a ese wrapper).

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::events::{Audience, Level};
use crate::state::{AppState, SessionInfo};

const SETTING: &str = "smart_enabled";
const WRAPPER: &str = "/usr/local/lib/labnas/smart-read";
const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Comando alternativo para tests (en vez de sudo + wrapper)
#[cfg(test)]
pub static TEST_COMMAND: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// Ultimo estado conocido por disco (para avisar solo cuando empeora)
static LAST_STATUS: LazyLock<Mutex<HashMap<String, Health>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Unknown,
    Ok,
    Warning,
    Failing,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DiskHealth {
    pub device: String,
    pub model: String,
    pub serial: String,
    pub size: String,
    pub transport: String,
    pub status: Option<Health>,
    pub temperature: Option<i64>,
    pub power_on_hours: Option<i64>,
    /// ATA: sectores reasignados / pendientes / irrecuperables
    pub reallocated: Option<i64>,
    pub pending: Option<i64>,
    pub uncorrectable: Option<i64>,
    /// NVMe: errores de medio y desgaste (%)
    pub media_errors: Option<i64>,
    pub percentage_used: Option<i64>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct SmartResponse {
    pub enabled: bool,
    /// El wrapper + sudo responden (setup-smart.sh aplicado)
    pub ready: bool,
    pub disks: Vec<DiskHealth>,
}

/// Discos fisicos (sin zram/loop) segun lsblk
fn list_disks() -> Vec<DiskHealth> {
    let Ok(out) = std::process::Command::new("lsblk")
        .args(["-J", "-d", "-o", "NAME,TYPE,SIZE,MODEL,SERIAL,TRAN"])
        .output()
    else {
        return vec![];
    };
    let json: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    let s = |v: &Value| v.as_str().unwrap_or("").trim().to_string();
    json["blockdevices"]
        .as_array()
        .map(|devs| {
            devs.iter()
                .filter(|d| d["type"] == "disk")
                .filter(|d| {
                    let n = d["name"].as_str().unwrap_or("");
                    n.starts_with("sd") || n.starts_with("vd") || (n.starts_with("nvme") && n.contains('n'))
                })
                .map(|d| DiskHealth {
                    device: format!("/dev/{}", s(&d["name"])),
                    model: s(&d["model"]),
                    serial: s(&d["serial"]),
                    size: s(&d["size"]),
                    transport: s(&d["tran"]),
                    ..Default::default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn run_smartctl(device: &str) -> Result<Value, String> {
    #[cfg(test)]
    if let Some(cmd) = TEST_COMMAND.read().ok().and_then(|c| c.clone()) {
        let out = std::process::Command::new(&cmd).arg(device).output().map_err(|e| e.to_string())?;
        return serde_json::from_slice(&out.stdout).map_err(|e| e.to_string());
    }
    let out = std::process::Command::new("sudo")
        .args(["-n", WRAPPER, device])
        .output()
        .map_err(|e| format!("No se pudo ejecutar sudo: {}", e))?;
    // smartctl usa bits del codigo de salida para avisos: el JSON sigue siendo valido
    if out.stdout.is_empty() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.contains("password") || err.contains("contraseña") || err.contains("sudo") {
            "Falta configurar el permiso (sudo bash setup-smart.sh)".to_string()
        } else if err.is_empty() {
            "smartctl no devolvio datos".to_string()
        } else {
            err
        });
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("Respuesta de smartctl invalida: {}", e))
}

/// Completa `disk` con los datos de `smartctl -j -a`
fn apply_smart(disk: &mut DiskHealth, j: &Value) {
    let int = |v: &Value| v.as_i64();
    disk.temperature = int(&j["temperature"]["current"]);
    disk.power_on_hours = int(&j["power_on_time"]["hours"]);
    if disk.model.is_empty() {
        disk.model = j["model_name"].as_str().unwrap_or("").to_string();
    }

    // ATA: atributos 5, 197, 198 (valor crudo)
    if let Some(table) = j["ata_smart_attributes"]["table"].as_array() {
        let raw = |id: i64| table.iter().find(|a| a["id"] == id).and_then(|a| a["raw"]["value"].as_i64());
        disk.reallocated = raw(5);
        disk.pending = raw(197);
        disk.uncorrectable = raw(198);
    }
    // NVMe
    let nvme = &j["nvme_smart_health_information_log"];
    let critical = nvme["critical_warning"].as_i64().unwrap_or(0);
    if nvme.is_object() {
        disk.media_errors = int(&nvme["media_errors"]);
        disk.percentage_used = int(&nvme["percentage_used"]);
        disk.temperature = disk.temperature.or(int(&nvme["temperature"]));
        disk.power_on_hours = disk.power_on_hours.or(int(&nvme["power_on_hours"]));
    }

    let passed = j["smart_status"]["passed"].as_bool();
    let bad_counts = [disk.reallocated, disk.pending, disk.uncorrectable, disk.media_errors]
        .iter()
        .any(|v| v.unwrap_or(0) > 0);
    let worn = disk.percentage_used.unwrap_or(0) >= 90;
    let hot = disk.temperature.unwrap_or(0) >= 60;

    disk.status = Some(if passed == Some(false) || critical != 0 {
        Health::Failing
    } else if bad_counts || worn || hot {
        Health::Warning
    } else if passed == Some(true) {
        Health::Ok
    } else {
        Health::Unknown
    });
}

async fn check_disks() -> Vec<DiskHealth> {
    tokio::task::spawn_blocking(|| {
        list_disks()
            .into_iter()
            .map(|mut d| {
                match run_smartctl(&d.device) {
                    Ok(j) => apply_smart(&mut d, &j),
                    Err(e) => {
                        d.status = Some(Health::Unknown);
                        d.error = Some(e);
                    }
                }
                d
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

async fn enabled(state: &AppState) -> bool {
    crate::db::db_op(&state.db, |conn| Ok(crate::db::get_setting_bool(conn, SETTING))).await.unwrap_or(false)
}

/// GET /api/system/smart (admin)
pub async fn get_smart(State(state): State<AppState>) -> Json<SmartResponse> {
    if !enabled(&state).await {
        return Json(SmartResponse { enabled: false, ready: false, disks: vec![] });
    }
    let disks = check_disks().await;
    let ready = disks.iter().any(|d| d.error.is_none());
    Json(SmartResponse { enabled: true, ready, disks })
}

#[derive(serde::Deserialize)]
pub struct SmartSettings {
    pub enabled: bool,
}

/// PUT /api/system/smart (admin)
pub async fn set_smart(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<SmartSettings>,
) -> Result<Json<SmartResponse>, (StatusCode, String)> {
    let v = if req.enabled { "true" } else { "false" };
    crate::db::db_op(&state.db, move |conn| crate::db::set_setting(conn, SETTING, v)).await?;
    state
        .log_activity("SMART", if req.enabled { "activado" } else { "desactivado" }, &session.username)
        .await;
    Ok(get_smart(State(state)).await)
}

/// Revision periodica: avisa a los admins cuando un disco empeora
pub async fn smart_monitor_loop(state: AppState) {
    loop {
        if enabled(&state).await {
            let disks = check_disks().await;
            let worse: Vec<DiskHealth> = {
                let Ok(mut last) = LAST_STATUS.lock() else { continue };
                disks
                    .into_iter()
                    .filter(|d| {
                        let now = d.status.unwrap_or(Health::Unknown);
                        let prev = last.insert(d.device.clone(), now).unwrap_or(Health::Ok);
                        now > prev && now >= Health::Warning
                    })
                    .collect()
            };
            for d in worse {
                let failing = d.status == Some(Health::Failing);
                let title = format!("{} disco {}", if failing { "Falla en" } else { "Advertencia en" }, d.device);
                let body = format!(
                    "{} {} (temp {} C, reasignados {}, pendientes {}, errores {}, desgaste {}%)",
                    d.model,
                    d.serial,
                    d.temperature.map(|t| t.to_string()).unwrap_or("?".into()),
                    d.reallocated.unwrap_or(0),
                    d.pending.unwrap_or(0),
                    d.media_errors.unwrap_or(0) + d.uncorrectable.unwrap_or(0),
                    d.percentage_used.unwrap_or(0),
                );
                state.log_activity("SMART", &format!("{}: {}", title, body), "sistema").await;
                crate::events::notify(&state, Audience::Admins, None, if failing { Level::Error } else { Level::Warning }, &title, &body);
                crate::handlers::notifications::notify_admins(&state, &format!("*{}*\n\n{}", title, body)).await;
            }
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    pub const ATA_OK: &str = r#"{"smartctl":{"exit_status":0},"model_name":"WDC WD40EFRX","smart_status":{"passed":true},
        "temperature":{"current":34},"power_on_time":{"hours":21000},
        "ata_smart_attributes":{"table":[
            {"id":5,"name":"Reallocated_Sector_Ct","raw":{"value":0}},
            {"id":197,"name":"Current_Pending_Sector","raw":{"value":0}},
            {"id":198,"name":"Offline_Uncorrectable","raw":{"value":0}}]}}"#;
    pub const ATA_REALLOC: &str = r#"{"smart_status":{"passed":true},"temperature":{"current":38},
        "ata_smart_attributes":{"table":[{"id":5,"raw":{"value":24}},{"id":197,"raw":{"value":0}}]}}"#;
    pub const ATA_FAILED: &str = r#"{"smart_status":{"passed":false},"temperature":{"current":41}}"#;
    pub const NVME_OK: &str = r#"{"model_name":"SKHynix","smart_status":{"passed":true},"power_on_time":{"hours":3100},
        "nvme_smart_health_information_log":{"critical_warning":0,"temperature":35,"percentage_used":3,"media_errors":0}}"#;
    pub const NVME_WORN: &str = r#"{"smart_status":{"passed":true},
        "nvme_smart_health_information_log":{"critical_warning":0,"temperature":40,"percentage_used":95,"media_errors":0}}"#;
    pub const NVME_CRITICAL: &str = r#"{"smart_status":{"passed":true},
        "nvme_smart_health_information_log":{"critical_warning":4,"temperature":40,"percentage_used":10}}"#;
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn health(json: &str) -> DiskHealth {
        let mut d = DiskHealth::default();
        apply_smart(&mut d, &serde_json::from_str(json).unwrap());
        d
    }

    #[test]
    fn estados_ata() {
        let ok = health(ATA_OK);
        assert_eq!(ok.status, Some(Health::Ok));
        assert_eq!((ok.temperature, ok.power_on_hours, ok.reallocated), (Some(34), Some(21000), Some(0)));
        assert_eq!(ok.model, "WDC WD40EFRX");
        assert_eq!(health(ATA_REALLOC).status, Some(Health::Warning));
        assert_eq!(health(ATA_FAILED).status, Some(Health::Failing));
    }

    #[test]
    fn estados_nvme() {
        let ok = health(NVME_OK);
        assert_eq!(ok.status, Some(Health::Ok));
        assert_eq!((ok.temperature, ok.percentage_used, ok.power_on_hours), (Some(35), Some(3), Some(3100)));
        assert_eq!(health(NVME_WORN).status, Some(Health::Warning));
        assert_eq!(health(NVME_CRITICAL).status, Some(Health::Failing));
    }

    #[test]
    fn sin_datos_es_desconocido() {
        assert_eq!(health("{}").status, Some(Health::Unknown));
    }

    #[test]
    fn orden_de_gravedad() {
        assert!(Health::Failing > Health::Warning && Health::Warning > Health::Ok && Health::Ok > Health::Unknown);
    }
}
