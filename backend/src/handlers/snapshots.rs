//! Snapshots btrfs/ZFS programados: versiones instantaneas de carpetas completas,
//! casi gratis en espacio, que protegen contra borrados y ransomware.
//!
//! LabNAS no corre como root: usa `sudo -n /usr/local/lib/labnas/snapshot`, un
//! wrapper que solo actua sobre las rutas que el admin habilito con
//! setup-snapshots.sh y solo toca snapshots `labnas-*`.
//!
//! Los snapshots se ven (solo lectura) en `<ruta>/.snapshots/` (btrfs) o
//! `<ruta>/.zfs/snapshot/` (ZFS): restaurar es copiar desde ahi con el explorador.

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::db::db_op;
use crate::state::{AppState, SessionInfo};
use crate::error::{ApiError, internal};

const WRAPPER: &str = "/usr/local/lib/labnas/snapshot";

/// Comando alternativo para tests (en vez de sudo + wrapper)
#[cfg(test)]
pub static TEST_COMMAND: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);


fn run(args: &[&str]) -> Result<String, String> {
    #[cfg(test)]
    let cmd: Option<String> = TEST_COMMAND.read().ok().and_then(|c| c.clone());
    #[cfg(not(test))]
    let cmd: Option<String> = None;

    let out = match cmd {
        Some(c) => std::process::Command::new(c).args(args).output(),
        None => std::process::Command::new("sudo").arg("-n").arg(WRAPPER).args(args).output(),
    }
    .map_err(|e| format!("No se pudo ejecutar el wrapper: {}", e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.contains("password") || err.contains("contraseña") || err.contains("sudo:") {
            "Falta configurar el permiso (sudo bash setup-snapshots.sh <usuario> <ruta>)".to_string()
        } else {
            crate::textutil::ellipsize(&err, 300)
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn lines(s: &str) -> Vec<String> {
    s.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

fn valid_name(n: &str) -> bool {
    let b = n.as_bytes();
    n.len() == 22 && n.starts_with("labnas-") && b[15] == b'-' && b[7..15].iter().chain(&b[16..]).all(u8::is_ascii_digit)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    /// 0 = desactivado
    pub interval_hours: u32,
    /// Snapshots que se conservan por ruta
    pub keep: u32,
}

fn load_config(conn: &rusqlite::Connection) -> SnapshotConfig {
    SnapshotConfig {
        interval_hours: crate::db::get_setting_u32(conn, "snapshot_interval_hours", 0),
        keep: crate::db::get_setting_u32(conn, "snapshot_keep", 24),
    }
}

#[derive(Serialize)]
pub struct PathSnapshots {
    pub path: String,
    pub snapshots: Vec<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct SnapshotsView {
    /// El wrapper responde (setup-snapshots.sh aplicado)
    pub ready: bool,
    pub error: Option<String>,
    pub config: SnapshotConfig,
    pub paths: Vec<PathSnapshots>,
}

fn collect() -> (bool, Option<String>, Vec<PathSnapshots>) {
    match run(&["paths"]) {
        Ok(out) => {
            let paths = lines(&out)
                .into_iter()
                .map(|p| match run(&["list", &p]) {
                    Ok(l) => {
                        let mut snaps: Vec<String> = lines(&l).into_iter().filter(|n| valid_name(n)).collect();
                        snaps.sort();
                        snaps.reverse();
                        PathSnapshots { path: p, snapshots: snaps, error: None }
                    }
                    Err(e) => PathSnapshots { path: p, snapshots: vec![], error: Some(e) },
                })
                .collect();
            (true, None, paths)
        }
        Err(e) => (false, Some(e), vec![]),
    }
}

/// GET /api/snapshots
pub async fn get_snapshots(State(state): State<AppState>) -> Result<Json<SnapshotsView>, ApiError> {
    let config = db_op(&state.db, |conn| Ok(load_config(conn))).await?;
    let (ready, error, paths) = tokio::task::spawn_blocking(collect).await.map_err(internal)?;
    Ok(Json(SnapshotsView { ready, error, config, paths }))
}

/// PUT /api/snapshots/config
pub async fn set_config(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(cfg): Json<SnapshotConfig>,
) -> Result<Json<SnapshotConfig>, ApiError> {
    if cfg.interval_hours > 24 * 7 || cfg.keep == 0 || cfg.keep > 1000 {
        return Err((StatusCode::BAD_REQUEST, "Frecuencia: 0 a 168 horas; conservar: 1 a 1000".to_string()));
    }
    let c = cfg.clone();
    db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, "snapshot_interval_hours", &c.interval_hours.to_string())?;
        crate::db::set_setting(conn, "snapshot_keep", &c.keep.to_string())
    })
    .await?;
    let detail = if cfg.interval_hours == 0 {
        "Snapshots desactivados".to_string()
    } else {
        format!("Cada {} h, conservar {}", cfg.interval_hours, cfg.keep)
    };
    state.log_activity("snapshots_config", &detail, &session.username).await;
    Ok(Json(cfg))
}

/// Crea un snapshot en cada ruta habilitada y aplica la retencion. Devuelve (creados, errores).
fn snapshot_all(keep: u32) -> (Vec<String>, Vec<String>) {
    let mut created = Vec::new();
    let mut errors = Vec::new();
    let paths = match run(&["paths"]) {
        Ok(out) => lines(&out),
        Err(e) => return (created, vec![e]),
    };
    for p in paths {
        match run(&["create", &p]) {
            Ok(name) => created.push(format!("{} ({})", p, name.trim())),
            Err(e) => {
                errors.push(format!("{}: {}", p, e));
                continue;
            }
        }
        // Retencion: borrar los mas viejos (los nombres ordenan por fecha)
        if let Ok(l) = run(&["list", &p]) {
            let mut snaps: Vec<String> = lines(&l).into_iter().filter(|n| valid_name(n)).collect();
            snaps.sort();
            let excess = snaps.len().saturating_sub(keep as usize);
            for old in &snaps[..excess] {
                if let Err(e) = run(&["delete", &p, old]) {
                    errors.push(format!("{}: no se pudo borrar {}: {}", p, old, e));
                }
            }
        }
    }
    (created, errors)
}

/// POST /api/snapshots/now
pub async fn snapshot_now(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let keep = db_op(&state.db, |conn| Ok(load_config(conn).keep)).await?;
    let (created, errors) = tokio::task::spawn_blocking(move || snapshot_all(keep))
        .await
        .map_err(internal)?;
    state.log_activity("snapshot_manual", &format!("{} creados, {} errores", created.len(), errors.len()), &session.username).await;
    if created.is_empty() && !errors.is_empty() {
        return Err((StatusCode::BAD_GATEWAY, errors.join(" | ")));
    }
    Ok(Json(serde_json::json!({ "created": created, "errors": errors })))
}

/// Snapshots programados (revisa cada 5 minutos si toca)
pub async fn snapshot_loop(state: AppState) {
    let mut last = std::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await;
        let cfg = db_op(&state.db, |conn| Ok(load_config(conn))).await.ok();
        let Some(cfg) = cfg.filter(|c| c.interval_hours > 0) else { continue };
        if last.elapsed() < Duration::from_secs(u64::from(cfg.interval_hours) * 3600) {
            continue;
        }
        last = std::time::Instant::now();
        let keep = cfg.keep;
        let Ok((created, errors)) = tokio::task::spawn_blocking(move || snapshot_all(keep)).await else { continue };
        if !errors.is_empty() {
            let msg = errors.join(" | ");
            state.log_activity("snapshot_error", &msg, "sistema").await;
            crate::events::notify(&state, crate::events::Audience::Admins, None, crate::events::Level::Error, "Error en snapshots", &msg);
        } else if !created.is_empty() {
            state.log_activity("snapshot", &created.join(", "), "sistema").await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::valid_name;

    #[test]
    fn nombres() {
        assert!(valid_name("labnas-20261118-103000"));
        assert!(!valid_name("labnas-20261118-1030"));
        assert!(!valid_name("otro-20261118-103000"));
        assert!(!valid_name("labnas-2026111a-103000"));
    }
}
