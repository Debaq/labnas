//! Modulo de camaras: webcams USB del servidor y camaras IP de la red (incluidas
//! las de las impresoras 3D). Vista en vivo para operadores, fotos y grabaciones con
//! audio que se guardan en una carpeta del NAS.
//!
//! Transparencia: cada vista, foto y grabacion queda en la auditoria y en la tabla
//! `camera_recordings`; mientras se graba, todos los usuarios ven un aviso.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};

use crate::db::db_op;
use crate::events::Audience;
use crate::state::{AppState, SessionInfo};
use crate::error::{ApiError, internal};

pub mod hub;
pub mod vision;
pub mod detect;
pub mod obico;
use hub::{Audio, CamSpec, Input};


const FOLDER_SETTING: &str = "cameras_folder";
const MAX_MINUTES_SETTING: &str = "cameras_max_minutes";
const DEFAULT_MAX_MINUTES: u32 = 60;
/// Espacio libre minimo para empezar a grabar
const MIN_FREE_BYTES: u64 = 1024 * 1024 * 1024;
/// Prefijo de las camaras que vienen del modulo de impresoras 3D
const P3D_PREFIX: &str = "p3d-";

fn bad(msg: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

// ═══════════════════════════════════════
// Configuracion
// ═══════════════════════════════════════

#[derive(Clone, Debug)]
struct CameraRow {
    id: String,
    name: String,
    kind: String,
    device: String,
    url: Option<String>,
    resolution: String,
    audio: String,
}

impl CameraRow {
    fn spec(&self) -> CamSpec {
        let input = if self.kind == "usb" {
            Input::V4l2 { device: self.device.clone(), size: Some(self.resolution.clone()).filter(|r| !r.is_empty()) }
        } else {
            Input::Url(self.url.clone().unwrap_or_default())
        };
        let audio = match self.audio.as_str() {
            "" => Audio::None,
            "stream" => Audio::Stream,
            dev => Audio::Alsa(dev.to_string()),
        };
        CamSpec { id: self.id.clone(), name: self.name.clone(), input, audio }
    }
}

fn row_to_camera(r: &rusqlite::Row) -> rusqlite::Result<CameraRow> {
    Ok(CameraRow {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        device: r.get(3)?,
        url: r.get(4)?,
        resolution: r.get(5)?,
        audio: r.get(6)?,
    })
}

const CAMERA_COLS: &str = "id, name, kind, device, labnas_decrypt(url), resolution, audio";

async fn own_cameras(state: &AppState) -> Result<Vec<CameraRow>, ApiError> {
    db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare(&format!("SELECT {} FROM cameras ORDER BY display_order, name", CAMERA_COLS))
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_camera).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .await
}

/// Camaras propias mas las de las impresoras 3D (si ese modulo esta encendido)
async fn all_cameras(state: &AppState) -> Result<Vec<CameraRow>, ApiError> {
    let mut list = own_cameras(state).await?;
    if state.enabled_modules.lock().await.contains("printers3d") {
        for (id, name, url) in crate::handlers::printers3d::printer_stream_cameras(state).await {
            list.push(CameraRow {
                id: format!("{}{}", P3D_PREFIX, id),
                name: format!("{} (impresora 3D)", name),
                kind: "printer3d".into(),
                device: String::new(),
                url: Some(url),
                resolution: String::new(),
                audio: String::new(),
            });
        }
    }
    Ok(list)
}

async fn find_camera(state: &AppState, id: &str) -> Result<CameraRow, ApiError> {
    all_cameras(state)
        .await?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Camara no encontrada".to_string()))
}

struct Settings {
    folder: String,
    max_minutes: u32,
}

async fn load_settings(state: &AppState) -> Result<Settings, ApiError> {
    db_op(&state.db, |conn| {
        let folder = crate::db::get_setting(conn, FOLDER_SETTING).unwrap_or_default();
        let max_minutes = crate::db::get_setting_u32(conn, MAX_MINUTES_SETTING, DEFAULT_MAX_MINUTES);
        Ok(Settings { folder, max_minutes })
    })
    .await
}

/// Oculta la clave de una URL (rtsp://usuario:clave@host -> rtsp://usuario:***@host)
fn mask_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut u) if u.password().is_some() => {
            let _ = u.set_password(Some("***"));
            u.to_string()
        }
        Ok(u) => u.to_string(),
        Err(_) => String::new(),
    }
}

#[derive(Serialize)]
pub struct RecordingNow {
    camera_id: String,
    camera: String,
    by: String,
    since: String,
}

#[derive(Serialize)]
pub struct CameraView {
    id: String,
    name: String,
    kind: String,
    device: String,
    url: String,
    resolution: String,
    audio: String,
    /// Las camaras de impresoras 3D se configuran en su modulo
    editable: bool,
    live: bool,
    viewers: usize,
    error: Option<String>,
    recording: Option<RecordingNow>,
}

#[derive(Serialize)]
pub struct CamerasResponse {
    cameras: Vec<CameraView>,
    folder: String,
    max_minutes: u32,
    ffmpeg: bool,
}

async fn ffmpeg_available() -> bool {
    tokio::process::Command::new("ffmpeg").arg("-version").output().await.map(|o| o.status.success()).unwrap_or(false)
}

fn recording_now(camera_id: &str, camera: &str, r: &hub::ActiveRecording) -> RecordingNow {
    RecordingNow { camera_id: camera_id.into(), camera: camera.into(), by: r.username.clone(), since: r.started_at.clone() }
}

/// GET /api/cameras
pub async fn list_cameras(State(state): State<AppState>) -> Result<Json<CamerasResponse>, ApiError> {
    let settings = load_settings(&state).await?;
    let cameras = all_cameras(&state)
        .await?
        .into_iter()
        .map(|c| {
            let h = hub::get(&c.id);
            CameraView {
                recording: h.as_ref().and_then(|h| h.recording()).map(|r| recording_now(&c.id, &c.name, &r)),
                live: h.as_ref().is_some_and(|h| h.latest(Duration::from_secs(5)).is_some()),
                viewers: h.as_ref().map(|h| h.viewers()).unwrap_or(0),
                error: h.as_ref().and_then(|h| h.last_error()),
                editable: c.kind != "printer3d",
                url: c.url.as_deref().map(mask_url).unwrap_or_default(),
                id: c.id,
                name: c.name,
                kind: c.kind,
                device: c.device,
                resolution: c.resolution,
                audio: c.audio,
            }
        })
        .collect();
    Ok(Json(CamerasResponse { cameras, folder: settings.folder, max_minutes: settings.max_minutes, ffmpeg: ffmpeg_available().await }))
}

#[derive(Deserialize)]
pub struct CameraBody {
    name: String,
    kind: String,
    #[serde(default)]
    device: String,
    /// IP: URL nueva; vacia al editar = conservar la guardada
    #[serde(default)]
    url: String,
    #[serde(default)]
    resolution: String,
    #[serde(default)]
    audio: String,
}

fn validate(body: &CameraBody, editing: bool) -> Result<(), ApiError> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(bad("El nombre es obligatorio (maximo 60 caracteres)"));
    }
    match body.kind.as_str() {
        "usb" => {
            let d = &body.device;
            if !(d.starts_with("/dev/video") || d.starts_with("/dev/v4l/")) || d.contains("..") {
                return Err(bad("Dispositivo de video invalido"));
            }
            let alsa = (body.audio.starts_with("plughw:") || body.audio.starts_with("hw:"))
                && body.audio.chars().all(|c| c.is_ascii_alphanumeric() || ":=,._-".contains(c));
            let audio_ok = body.audio.is_empty() || body.audio == "default" || alsa;
            if !audio_ok {
                return Err(bad("Dispositivo de audio invalido"));
            }
        }
        "ip" => {
            if !(editing && body.url.is_empty()) {
                let u = url::Url::parse(body.url.trim()).map_err(|_| bad("URL invalida"))?;
                if !matches!(u.scheme(), "rtsp" | "rtsps" | "http" | "https") || u.host_str().is_none() {
                    return Err(bad("La URL debe ser rtsp:// o http(s):// con un host"));
                }
            }
            if !(body.audio.is_empty() || body.audio == "stream") {
                return Err(bad("Audio invalido para camara IP"));
            }
        }
        _ => return Err(bad("Tipo de camara invalido")),
    }
    let r = &body.resolution;
    let res_ok = r.is_empty()
        || r.split_once('x').is_some_and(|(w, h)| {
            [w, h].iter().all(|n| (2..=4).contains(&n.len()) && n.chars().all(|c| c.is_ascii_digit()))
        });
    if !res_ok {
        return Err(bad("Resolucion invalida (ej: 1280x720)"));
    }
    Ok(())
}

/// POST /api/cameras (admin)
pub async fn create_camera(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(body): Json<CameraBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    validate(&body, false)?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let url = if body.kind == "ip" { Some(crate::secrets::encrypt(body.url.trim()).map_err(internal)?) } else { None };
    let (id2, name) = (id.clone(), body.name.trim().to_string());
    let resolution = if body.kind == "usb" { body.resolution.clone() } else { String::new() };
    let device = if body.kind == "usb" { body.device.clone() } else { String::new() };
    let (kind, audio, n2) = (body.kind.clone(), body.audio.clone(), name.clone());
    db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO cameras (id, name, kind, device, url, resolution, audio, display_order, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, (SELECT COALESCE(MAX(display_order), 0) + 1 FROM cameras), ?8)",
            params![id2, n2, kind, device, url, resolution, audio, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;
    state.log_activity("Camara", &format!("Agrego la camara {}", name), &session.username).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// PUT /api/cameras/{id} (admin)
pub async fn update_camera(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(body): Json<CameraBody>,
) -> Result<StatusCode, ApiError> {
    validate(&body, true)?;
    let current = own_cameras(&state).await?.into_iter().find(|c| c.id == id).ok_or((StatusCode::NOT_FOUND, "Camara no encontrada".to_string()))?;
    if current.kind != body.kind {
        return Err(bad("No se puede cambiar el tipo de una camara"));
    }
    let url = match (body.kind.as_str(), body.url.trim()) {
        ("ip", "") => current.url.clone(),
        ("ip", u) => Some(u.to_string()),
        _ => None,
    };
    let url_enc = match url {
        Some(u) => Some(crate::secrets::encrypt(&u).map_err(internal)?),
        None => None,
    };
    let name = body.name.trim().to_string();
    let (id2, n2, device, resolution, audio) = (id.clone(), name.clone(), body.device.clone(), body.resolution.clone(), body.audio.clone());
    db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE cameras SET name = ?2, device = ?3, url = ?4, resolution = ?5, audio = ?6 WHERE id = ?1",
            params![id2, n2, device, url_enc, resolution, audio],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;
    // Si estaba abierta, la proxima vez arranca con la configuracion nueva
    if let Some(h) = hub::get(&id) {
        if h.recording().is_none() {
            let cam = find_camera(&state, &id).await?;
            hub::ensure(&state, cam.spec());
        }
    }
    state.log_activity("Camara", &format!("Edito la camara {}", name), &session.username).await;
    Ok(StatusCode::OK)
}

/// DELETE /api/cameras/{id} (admin)
pub async fn delete_camera(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if hub::get(&id).is_some_and(|h| h.recording().is_some()) {
        return Err((StatusCode::CONFLICT, "La camara esta grabando: detén la grabacion primero".into()));
    }
    let id2 = id.clone();
    let name: Option<String> = db_op(&state.db, move |conn| {
        let name = conn.query_row("SELECT name FROM cameras WHERE id = ?1", params![id2], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM cameras WHERE id = ?1", params![id2]).map_err(|e| e.to_string())?;
        Ok(name)
    })
    .await?;
    let name = name.ok_or((StatusCode::NOT_FOUND, "Camara no encontrada".to_string()))?;
    state.log_activity("Camara", &format!("Elimino la camara {}", name), &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SettingsBody {
    folder: String,
    max_minutes: u32,
}

/// PUT /api/cameras/settings (admin): carpeta de grabaciones y duracion maxima
pub async fn update_settings(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(body): Json<SettingsBody>,
) -> Result<StatusCode, ApiError> {
    if !(1..=600).contains(&body.max_minutes) {
        return Err(bad("La duracion maxima debe estar entre 1 y 600 minutos"));
    }
    let folder = body.folder.trim().to_string();
    if !folder.is_empty() {
        let st = crate::storage::Storage::load(&state.db).await?;
        let dir = st.resolve_for(&session, &folder, crate::storage::Op::Write)?;
        if !dir.is_dir() {
            return Err((StatusCode::NOT_FOUND, "La carpeta no existe".into()));
        }
    }
    let (f2, m) = (folder.clone(), body.max_minutes);
    db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, FOLDER_SETTING, &f2).map_err(|e| e.to_string())?;
        crate::db::set_setting(conn, MAX_MINUTES_SETTING, &m.to_string()).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;
    state.log_activity("Camara", &format!("Grabaciones en {} (maximo {} min)", folder, m), &session.username).await;
    Ok(StatusCode::OK)
}

// ═══════════════════════════════════════
// Deteccion de dispositivos del servidor
// ═══════════════════════════════════════

#[derive(Serialize, Debug, PartialEq)]
pub struct DeviceOption {
    id: String,
    name: String,
}

#[derive(Serialize)]
pub struct DetectResponse {
    video: Vec<DeviceOption>,
    audio: Vec<DeviceOption>,
}

/// Camaras V4L2 que capturan (index 0 de cada una), con ruta estable de /dev/v4l/by-id si existe
fn detect_video() -> Vec<DeviceOption> {
    let mut by_id: HashMap<String, String> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir("/dev/v4l/by-id") {
        for e in entries.flatten() {
            let p = e.path();
            if let (Ok(target), Some(name)) = (std::fs::canonicalize(&p), p.file_name()) {
                let name = name.to_string_lossy().to_string();
                if name.ends_with("-index0") {
                    by_id.insert(target.to_string_lossy().to_string(), p.to_string_lossy().to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/video4linux") else { return out };
    let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    names.sort();
    for n in names {
        let base = format!("/sys/class/video4linux/{}", n);
        let index = std::fs::read_to_string(format!("{}/index", base)).unwrap_or_default();
        if index.trim() != "0" {
            continue;
        }
        let label = std::fs::read_to_string(format!("{}/name", base)).unwrap_or_default().trim().to_string();
        let dev = format!("/dev/{}", n);
        out.push(DeviceOption { id: by_id.get(&dev).cloned().unwrap_or(dev.clone()), name: format!("{} ({})", label, dev) });
    }
    out
}

/// `arecord -l` -> microfonos ALSA (plughw:CARD=<id>,DEV=<n>, estable entre reinicios)
fn parse_arecord(out: &str) -> Vec<DeviceOption> {
    out.lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("card ")?;
            let (_, rest) = rest.split_once(": ")?;
            let (card_id, rest) = rest.split_once(" [")?;
            let (card_name, rest) = rest.split_once("], device ")?;
            let (dev, rest) = rest.split_once(": ")?;
            let dev_name = rest.split_once(" [").map(|(_, d)| d.trim_end_matches(']')).unwrap_or(rest);
            Some(DeviceOption { id: format!("plughw:CARD={},DEV={}", card_id.trim(), dev.trim()), name: format!("{} — {}", card_name, dev_name) })
        })
        .collect()
}

/// GET /api/cameras/detect (admin)
pub async fn detect_devices() -> Json<DetectResponse> {
    let video = tokio::task::spawn_blocking(detect_video).await.unwrap_or_default();
    let audio = match tokio::process::Command::new("arecord").arg("-l").env("LANG", "C").output().await {
        Ok(o) => parse_arecord(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => Vec::new(),
    };
    Json(DetectResponse { video, audio })
}

// ═══════════════════════════════════════
// Vista en vivo y foto
// ═══════════════════════════════════════

/// Ultima vez que cada usuario abrio cada camara (para no llenar la auditoria)
static VIEW_LOG: LazyLock<StdMutex<HashMap<(String, String), Instant>>> = LazyLock::new(Default::default);
const VIEW_LOG_EVERY: Duration = Duration::from_secs(30 * 60);

async fn log_view(state: &AppState, user: &str, cam: &CameraRow) {
    let key = (user.to_string(), cam.id.clone());
    let fresh = VIEW_LOG
        .lock()
        .map(|mut m| {
            let due = m.get(&key).is_none_or(|t| t.elapsed() > VIEW_LOG_EVERY);
            if due {
                m.insert(key, Instant::now());
            }
            due
        })
        .unwrap_or(true);
    if fresh {
        state.log_access(&format!("Vio la camara {}", cam.name), user).await;
    }
}

/// GET /api/cameras/{id}/stream: MJPEG (multipart/x-mixed-replace) para un <img>
pub async fn stream(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let cam = find_camera(&state, &id).await?;
    let h = hub::ensure(&state, cam.spec());
    log_view(&state, &session.username, &cam).await;
    let guard = hub::add_viewer(&h);
    let rx = h.subscribe();
    let first = h.latest(Duration::from_secs(2));

    let part = |jpeg: &[u8]| {
        let mut b = format!("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", jpeg.len()).into_bytes();
        b.extend_from_slice(jpeg);
        b.extend_from_slice(b"\r\n");
        axum::body::Bytes::from(b)
    };
    let initial = first.map(|f| part(&f));
    let body_stream = futures_util::stream::unfold((rx, guard, initial), move |(mut rx, guard, mut initial)| async move {
        if let Some(b) = initial.take() {
            return Some((Ok::<_, std::io::Error>(b), (rx, guard, None)));
        }
        loop {
            match rx.recv().await {
                Ok(f) => return Some((Ok(part(&f)), (rx, guard, None))),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    });
    Ok((
        [
            (header::CONTENT_TYPE, "multipart/x-mixed-replace; boundary=frame"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(body_stream),
    )
        .into_response())
}

/// Cuadro actual (arranca la camara si hace falta y espera hasta 10 s)
async fn current_frame(state: &AppState, cam: &CameraRow) -> Result<axum::body::Bytes, ApiError> {
    let h = hub::ensure(state, cam.spec());
    h.touch_lease();
    if let Some(f) = h.latest(Duration::from_secs(2)) {
        return Ok(f);
    }
    let mut rx = h.subscribe();
    match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
        Ok(Ok(f)) => Ok(f),
        _ => Err((
            StatusCode::BAD_GATEWAY,
            h.last_error().unwrap_or_else(|| "La camara no entrego imagen".into()),
        )),
    }
}

/// GET /api/cameras/{id}/snapshot: JPEG actual (la grilla lo pide cada segundo)
pub async fn snapshot(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let cam = find_camera(&state, &id).await?;
    let f = current_frame(&state, &cam).await?;
    log_view(&state, &session.username, &cam).await;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "no-store")], f).into_response())
}

// ═══════════════════════════════════════
// Fotos y grabaciones
// ═══════════════════════════════════════

fn safe_component(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).take(60).collect();
    let t = t.trim().to_string();
    if t.is_empty() { "camara".into() } else { t }
}

/// Carpeta de destino ya validada (dentro de las raices) para la camara
async fn target_dir(state: &AppState, cam: &CameraRow) -> Result<(PathBuf, u32), ApiError> {
    let settings = load_settings(state).await?;
    if settings.folder.is_empty() {
        return Err(bad("Falta configurar la carpeta de grabaciones (Camaras > Ajustes)"));
    }
    let st = crate::storage::Storage::load(&state.db).await?;
    // Validacion con permisos de administrador: la carpeta la eligio el admin
    let admin = SessionInfo::new("labnas".into(), crate::models::notifications::UserRole::admin(), crate::roles::Grants::admin(), None);
    let base = st.resolve_for(&admin, &settings.folder, crate::storage::Op::Write)?;
    if !base.is_dir() {
        return Err((StatusCode::NOT_FOUND, "La carpeta de grabaciones no existe".into()));
    }
    Ok((base.join(safe_component(&cam.name)), settings.max_minutes))
}

fn free_bytes(path: &std::path::Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: ruta terminada en NUL y estructura valida para escribir
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0).then(|| s.f_bavail as u64 * s.f_frsize as u64)
}

fn file_stamp(user: &str) -> String {
    format!("{}_{}", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"), safe_component(user))
}

/// Fila del registro: con tamano ya esta lista (foto); sin tamano es una grabacion en curso
async fn insert_record(state: &AppState, id: &str, cam: &CameraRow, kind: &str, user: &str, path: &std::path::Path, size: Option<u64>) -> Result<String, ApiError> {
    let now = chrono::Local::now().to_rfc3339();
    let status = if size.is_some() { "ok" } else { "grabando" };
    let (id, cid, cname, kind, user, path, status, n2) =
        (id.to_string(), cam.id.clone(), cam.name.clone(), kind.to_string(), user.to_string(), path.to_string_lossy().to_string(), status.to_string(), now.clone());
    let ended = if status == "grabando" { None } else { Some(now.clone()) };
    db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO camera_recordings (id, camera_id, camera_name, kind, username, path, started_at, ended_at, size, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, cid, cname, kind, user, path, n2, ended, size.map(|s| s as i64), status],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;
    Ok(now)
}

/// POST /api/cameras/{id}/photo: guarda el cuadro actual en la carpeta
pub async fn take_photo(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cam = find_camera(&state, &id).await?;
    let (dir, _) = target_dir(&state, &cam).await?;
    let frame = current_frame(&state, &cam).await?;
    tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
    let path = dir.join(format!("{}.jpg", file_stamp(&session.username)));
    tokio::fs::write(&path, &frame).await.map_err(internal)?;
    let rid = uuid::Uuid::new_v4().simple().to_string();
    insert_record(&state, &rid, &cam, "foto", &session.username, &path, Some(frame.len() as u64)).await?;
    state.log_activity("Camara", &format!("Tomo una foto con {} ({})", cam.name, path.display()), &session.username).await;
    Ok(Json(serde_json::json!({ "id": rid, "path": path })))
}

fn publish_recording_state(state: &AppState) {
    state.events.publish("cameras.recording", recording_list_blocking(), Audience::All, Some("cameras"));
}

fn recording_list_blocking() -> Vec<RecordingNow> {
    hub::active_recordings()
        .into_iter()
        .map(|(id, r)| recording_now(&id, &r.camera, &r))
        .collect()
}

/// POST /api/cameras/{id}/record
pub async fn start_record(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cam = find_camera(&state, &id).await?;
    let (rid, path) = begin_recording(&state, &cam, &session.username, None).await?;
    Ok(Json(serde_json::json!({ "id": rid, "path": path })))
}

/// Empieza a grabar (boton "Grabar" o un clip de la deteccion con IA). `max`: duracion
/// propia; sin ella, la maxima configurada
async fn begin_recording(state: &AppState, cam: &CameraRow, user: &str, max: Option<Duration>) -> Result<(String, PathBuf), ApiError> {
    if !ffmpeg_available().await {
        return Err(bad("ffmpeg no esta instalado en el servidor"));
    }
    let (dir, max_minutes) = target_dir(state, cam).await?;
    let base = dir.parent().map(|p| p.to_path_buf()).unwrap_or(dir.clone());
    if free_bytes(&base).is_some_and(|f| f < MIN_FREE_BYTES) {
        return Err((StatusCode::INSUFFICIENT_STORAGE, "Queda menos de 1 GB libre en la carpeta de grabaciones".into()));
    }
    let h = hub::ensure(state, cam.spec());
    let rid = uuid::Uuid::new_v4().simple().to_string();
    let path = dir.join(format!("{}.mp4", file_stamp(user)));
    let rec = hub::ActiveRecording {
        recording_id: rid.clone(),
        camera: cam.name.clone(),
        path: path.clone(),
        username: user.to_string(),
        started: Instant::now(),
        started_at: chrono::Local::now().to_rfc3339(),
        max: max.unwrap_or(Duration::from_secs(max_minutes as u64 * 60)),
    };
    hub::start_recording(&h, rec).map_err(|e| (StatusCode::CONFLICT, e))?;
    insert_record(state, &rid, cam, "video", user, &path, None).await?;
    state.log_activity("Camara", &format!("Empezo a grabar con {} ({})", cam.name, path.display()), user).await;
    publish_recording_state(state);
    Ok((rid, path))
}

/// POST /api/cameras/{id}/stop
pub async fn stop_record(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let h = hub::get(&id).ok_or((StatusCode::NOT_FOUND, "La camara no esta grabando".to_string()))?;
    let rec = hub::stop_recording(&h).ok_or((StatusCode::NOT_FOUND, "La camara no esta grabando".to_string()))?;
    state
        .log_activity("Camara", &format!("Detuvo la grabacion de {} (iniciada por {})", id_name(&state, &id).await, rec.username), &session.username)
        .await;
    publish_recording_state(&state);
    Ok(StatusCode::OK)
}

async fn id_name(state: &AppState, id: &str) -> String {
    find_camera(state, id).await.map(|c| c.name).unwrap_or_else(|_| id.to_string())
}

/// Lo llama el supervisor cuando el archivo queda cerrado
pub(crate) async fn finish_recording(state: &AppState, r: &hub::ActiveRecording, ok: bool) {
    let size = tokio::fs::metadata(&r.path).await.map(|m| m.len()).unwrap_or(0);
    let (id, status) = (r.recording_id.clone(), if ok { "ok" } else { "error" });
    let _ = db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE camera_recordings SET ended_at = ?2, size = ?3, status = ?4 WHERE id = ?1 AND status = 'grabando'",
            params![id, chrono::Local::now().to_rfc3339(), size as i64, status],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await;
    if !ok {
        state.log_activity("Camara", &format!("La grabacion {} fallo", r.path.display()), &r.username).await;
    }
    publish_recording_state(state);
}

pub(crate) fn notify_auto_stop(state: &AppState, camera: &str) {
    crate::events::notify(
        state,
        Audience::All,
        Some("cameras"),
        crate::events::Level::Info,
        "Grabacion detenida",
        &format!("{}: se alcanzo la duracion maxima", camera),
    );
}

/// GET /api/cameras/recording-now: lo que se esta grabando (lo ven todos los usuarios)
pub async fn recording_now_list() -> Json<Vec<RecordingNow>> {
    Json(recording_list_blocking())
}

#[derive(Serialize)]
pub struct RecordingRow {
    id: String,
    camera_id: String,
    camera_name: String,
    kind: String,
    username: String,
    path: String,
    started_at: String,
    ended_at: Option<String>,
    size: Option<i64>,
    status: String,
    deleted_by: Option<String>,
}

#[derive(Deserialize)]
pub struct RecordingsQuery {
    camera: Option<String>,
}

/// GET /api/cameras/recordings[?camera=id]
pub async fn list_recordings(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<RecordingsQuery>,
) -> Result<Json<Vec<RecordingRow>>, ApiError> {
    let sees_ai = detect::can_view(&state, &session).await;
    let rows = db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, camera_id, camera_name, kind, username, path, started_at, ended_at, size, status, deleted_by
                 FROM camera_recordings WHERE (?1 IS NULL OR camera_id = ?1) ORDER BY started_at DESC LIMIT 300",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![q.camera], |r| {
                Ok(RecordingRow {
                    id: r.get(0)?,
                    camera_id: r.get(1)?,
                    camera_name: r.get(2)?,
                    kind: r.get(3)?,
                    username: r.get(4)?,
                    path: r.get(5)?,
                    started_at: r.get(6)?,
                    ended_at: r.get(7)?,
                    size: r.get(8)?,
                    status: r.get(9)?,
                    deleted_by: r.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect::<Vec<RecordingRow>>())
    })
    .await?;
    // Lo que hizo la IA solo lo ven quienes ven la deteccion
    let rows = rows.into_iter().filter(|r| sees_ai || !detect::is_ai_record(&r.username)).collect();
    Ok(Json(rows))
}

async fn recording_path(state: &AppState, id: &str) -> Result<(String, String, String, String), ApiError> {
    let id = id.to_string();
    db_op(&state.db, move |conn| {
        conn.query_row("SELECT path, kind, status, username FROM camera_recordings WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .optional()
            .map_err(|e| e.to_string())
    })
    .await?
    .ok_or((StatusCode::NOT_FOUND, "Grabacion no encontrada".to_string()))
}

#[derive(Deserialize, Default)]
pub struct DownloadQuery {
    /// `inline`: verla en el navegador; `thumb`: miniatura de una foto (no queda en la
    /// auditoria); sin nada: descargar
    #[serde(default)]
    view: Option<String>,
}

/// Ancho de las miniaturas de fotos
const THUMB_W: usize = 320;

/// GET /api/cameras/recordings/{id}/download[?view=inline|thumb]
pub async fn download_recording(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Query(q): Query<DownloadQuery>,
) -> Result<Response, ApiError> {
    let (path, kind, status, user) = recording_path(&state, &id).await?;
    // Las de la IA solo para quienes ven la deteccion; el resto, admin u operador
    let allowed = if detect::is_ai_record(&user) {
        detect::can_view(&state, &session).await
    } else {
        session.can("cameras", crate::roles::Level::Ver)
    };
    if !allowed {
        return Err((StatusCode::FORBIDDEN, "No tienes acceso a esta grabacion".into()));
    }
    if status == "borrado" {
        return Err((StatusCode::GONE, "La grabacion fue borrada".into()));
    }
    let view = q.view.as_deref().unwrap_or("");
    if view == "thumb" {
        if kind != "foto" {
            return Err((StatusCode::BAD_REQUEST, "Solo las fotos tienen miniatura".into()));
        }
        let bytes = tokio::fs::read(&path).await.map_err(|_| (StatusCode::NOT_FOUND, "El archivo ya no existe".to_string()))?;
        let thumb = tokio::task::spawn_blocking(move || {
            let img = vision::decode_jpeg(&bytes)?;
            vision::encode_jpeg(&vision::downscale(&img, THUMB_W), 70)
        })
        .await
        .map_err(internal)?
        .map_err(internal)?;
        return Ok((
            [(header::CONTENT_TYPE, "image/jpeg".to_string()), (header::CACHE_CONTROL, "private, max-age=86400".to_string())],
            thumb,
        )
            .into_response());
    }
    let file = tokio::fs::File::open(&path).await.map_err(|_| (StatusCode::NOT_FOUND, "El archivo ya no existe".to_string()))?;
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let inline = view == "inline";
    state.log_access(&format!("{} la grabacion {}", if inline { "Vio" } else { "Descargo" }, path), &session.username).await;
    let ctype = if kind == "foto" { "image/jpeg" } else { "video/mp4" };
    let disposition = if inline { "inline" } else { "attachment" };
    Ok((
        [
            (header::CONTENT_TYPE, ctype.to_string()),
            (header::CONTENT_LENGTH, len.to_string()),
            (header::CONTENT_DISPOSITION, format!("{}; filename*=UTF-8''{}", disposition, urlencoding::encode(&name))),
        ],
        Body::from_stream(tokio_util::io::ReaderStream::new(file)),
    )
        .into_response())
}

/// DELETE /api/cameras/recordings/{id} (admin): borra el archivo; el registro queda
pub async fn delete_recording(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let (path, _, status, _) = recording_path(&state, &id).await?;
    if status == "grabando" {
        return Err((StatusCode::CONFLICT, "La grabacion sigue en curso".into()));
    }
    let _ = tokio::fs::remove_file(&path).await;
    let (id2, user) = (id.clone(), session.username.clone());
    db_op(&state.db, move |conn| {
        conn.execute("UPDATE camera_recordings SET status = 'borrado', deleted_by = ?2 WHERE id = ?1", params![id2, user])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;
    state.log_activity("Camara", &format!("Borro la grabacion {}", path), &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

// ═══════════════════════════════════════
// Ciclo de vida del modulo
// ═══════════════════════════════════════

/// Al encender el modulo (o arrancar LabNAS): grabaciones que quedaron abiertas por un corte
pub async fn startup_cleanup(state: AppState) {
    let _ = db_op(&state.db, |conn| {
        conn.execute(
            "UPDATE camera_recordings SET status = 'interrumpida', ended_at = COALESCE(ended_at, started_at) WHERE status = 'grabando'",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await;
}

/// Al apagar el modulo: cierra grabaciones y camaras
pub fn shutdown(state: &AppState) {
    detect::shutdown();
    hub::stop_all();
    publish_recording_state(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microfonos_de_arecord() {
        let out = "**** List of CAPTURE Hardware Devices ****\ncard 0: PCH [HDA Intel PCH], device 0: ALC3246 Analog [ALC3246 Analog]\n  Subdevices: 1/1\ncard 2: Camera [USB Camera], device 0: USB Audio [USB Audio]\n";
        let d = parse_arecord(out);
        assert_eq!(d.len(), 2);
        assert_eq!(d[1], DeviceOption { id: "plughw:CARD=Camera,DEV=0".into(), name: "USB Camera — USB Audio".into() });
    }

    #[test]
    fn url_sin_clave() {
        assert_eq!(mask_url("rtsp://admin:secreta@10.0.0.5:554/s1"), "rtsp://admin:***@10.0.0.5:554/s1");
        assert_eq!(mask_url("http://10.0.0.5/mjpg"), "http://10.0.0.5/mjpg");
    }

    #[test]
    fn validacion() {
        let b = |kind: &str, device: &str, url: &str, audio: &str, res: &str| CameraBody {
            name: "Sala".into(),
            kind: kind.into(),
            device: device.into(),
            url: url.into(),
            resolution: res.into(),
            audio: audio.into(),
        };
        assert!(validate(&b("usb", "/dev/video0", "", "plughw:CARD=Camera,DEV=0", "1280x720"), false).is_ok());
        assert!(validate(&b("usb", "/dev/../etc/passwd", "", "", ""), false).is_err());
        assert!(validate(&b("usb", "/dev/video0", "", "hw:0;rm -rf", ""), false).is_err());
        assert!(validate(&b("usb", "/dev/video0", "", "", "grande"), false).is_err());
        assert!(validate(&b("ip", "", "rtsp://u:p@10.0.0.9/s", "stream", ""), false).is_ok());
        assert!(validate(&b("ip", "", "file:///etc/passwd", "", ""), false).is_err());
        assert!(validate(&b("ip", "", "", "", ""), true).is_ok(), "al editar, URL vacia = conservar");
        assert!(validate(&b("ip", "", "", "", ""), false).is_err());
    }

    #[test]
    fn nombres_seguros() {
        assert_eq!(safe_component("../Sala 1/"), "___Sala 1_");
        assert_eq!(safe_component(""), "camara");
    }
}
