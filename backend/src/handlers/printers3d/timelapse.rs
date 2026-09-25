//! Timelapse: snapshots de la camara durante la impresion y video al terminar (ffmpeg).
//!
//! Configuracion (settings): impresoras elegidas, intervalo y carpeta. Por defecto no
//! graba nada. La carpeta debe estar dentro de las carpetas accesibles (se ve desde el
//! explorador); por defecto `<primera carpeta>/Timelapses`.

use super::*;
use axum::Extension;

use crate::state::SessionInfo;

const SETTING: &str = "timelapse_config";
const DEFAULT_INTERVAL: u64 = 30;
/// Cuadros por segundo del video
const FPS: u32 = 24;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct TimelapseConfig {
    /// Impresoras que graban timelapse
    #[serde(default)]
    pub printers: Vec<String>,
    /// Segundos entre fotos
    #[serde(default)]
    pub interval_secs: u64,
    /// Carpeta destino (vacio = <primera carpeta accesible>/Timelapses)
    #[serde(default)]
    pub dir: String,
}

#[derive(serde::Serialize)]
pub struct TimelapseStatus {
    #[serde(flatten)]
    pub config: TimelapseConfig,
    /// Carpeta efectiva
    pub effective_dir: String,
    pub ffmpeg: bool,
}

pub(super) fn load_config(conn: &rusqlite::Connection) -> TimelapseConfig {
    let mut c: TimelapseConfig = crate::db::get_setting(conn, SETTING)
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    if c.interval_secs == 0 {
        c.interval_secs = DEFAULT_INTERVAL;
    }
    c
}

/// Carpeta efectiva: la configurada o `<primera raiz>/Timelapses`
pub(super) fn effective_dir(conn: &rusqlite::Connection, c: &TimelapseConfig) -> Option<std::path::PathBuf> {
    if !c.dir.trim().is_empty() {
        return Some(std::path::PathBuf::from(c.dir.trim()));
    }
    crate::storage::canonical_roots(conn).first().map(|r| r.join("Timelapses"))
}

pub(super) fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Nombre de carpeta seguro a partir del nombre de la impresora
fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { "impresora".to_string() } else { s }
}

/// Carpeta de una sesion de timelapse (una por impresion)
pub(super) fn session_dir(base: &std::path::Path, printer_name: &str) -> std::path::PathBuf {
    base.join(slug(printer_name)).join(chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string())
}

pub(super) fn frame_path(dir: &std::path::Path, n: u32) -> std::path::PathBuf {
    dir.join(format!("frame_{:05}.jpg", n))
}

/// Arma timelapse.mp4 con los fotogramas y los borra si salio bien
pub(super) fn assemble(dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let out = dir.join("timelapse.mp4");
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-framerate", &FPS.to_string(), "-i"])
        .arg(dir.join("frame_%05d.jpg"))
        // ancho/alto pares (libx264) y formato compatible con cualquier reproductor
        .args(["-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&out)
        .output()
        .map_err(|e| format!("No se pudo ejecutar ffmpeg: {}", e))?;
    if !status.status.success() {
        return Err(String::from_utf8_lossy(&status.stderr).lines().last().unwrap_or("ffmpeg fallo").to_string());
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("frame_") && name.ends_with(".jpg") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Ok(out)
}

// ─── Endpoints (lectura: cualquiera; cambios: operador/admin) ───

pub async fn get_timelapse(State(state): State<AppState>) -> Result<Json<TimelapseStatus>, (StatusCode, String)> {
    let (config, dir) = db_op(&state.db, |conn| {
        let c = load_config(conn);
        let d = effective_dir(conn, &c);
        Ok((c, d))
    })
    .await?;
    Ok(Json(TimelapseStatus {
        config,
        effective_dir: dir.map(|d| d.to_string_lossy().to_string()).unwrap_or_default(),
        ffmpeg: ffmpeg_available(),
    }))
}

pub async fn set_timelapse(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(mut req): Json<TimelapseConfig>,
) -> Result<Json<TimelapseStatus>, (StatusCode, String)> {
    if req.interval_secs == 0 {
        req.interval_secs = DEFAULT_INTERVAL;
    }
    if !(5..=3600).contains(&req.interval_secs) {
        return Err((StatusCode::BAD_REQUEST, "Intervalo entre 5 y 3600 segundos".to_string()));
    }
    // la carpeta debe quedar dentro de las carpetas accesibles
    if !req.dir.trim().is_empty() {
        let roots = crate::storage::load_roots(&state.db).await?;
        let resolved = crate::storage::resolve(&roots, req.dir.trim())
            .map_err(|_| (StatusCode::BAD_REQUEST, "La carpeta debe estar dentro de las carpetas accesibles".to_string()))?;
        req.dir = resolved.to_string_lossy().to_string();
    }
    let json = serde_json::to_string(&req).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    db_op(&state.db, move |conn| crate::db::set_setting(conn, SETTING, &json)).await?;
    state
        .log_activity("Timelapse", &format!("{} impresoras, cada {} s", req.printers.len(), req.interval_secs), &session.username)
        .await;
    get_timelapse(State(state)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres() {
        assert_eq!(slug("Prusa MK4 #2"), "Prusa_MK4__2");
        assert_eq!(slug("///"), "impresora");
        assert_eq!(frame_path(std::path::Path::new("/t"), 7), std::path::PathBuf::from("/t/frame_00007.jpg"));
    }

    #[test]
    fn arma_video_con_ffmpeg() {
        if !ffmpeg_available() {
            eprintln!("ffmpeg no disponible; test omitido");
            return;
        }
        let dir = std::env::temp_dir().join(format!("labnas-tl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 5 fotogramas JPEG de prueba (tamaño impar para probar el escalado a pares)
        let ok = std::process::Command::new("ffmpeg")
            .args(["-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=65x49:rate=1", "-frames:v", "5"])
            .arg(dir.join("frame_%05d.jpg").as_os_str())
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let video = assemble(&dir).unwrap();
        assert!(std::fs::metadata(&video).unwrap().len() > 0);
        assert!(!frame_path(&dir, 1).exists(), "borra los fotogramas");
        let _ = std::fs::remove_dir_all(dir);
    }
}
