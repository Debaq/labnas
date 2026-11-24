//! Impresion fallida («espagueti») con el `ml_api` de Obico, que corre aparte (en el
//! mismo servidor, con `setup-obico.sh`). Obico es AGPL: no va dentro de LabNAS, solo
//! se le habla por HTTP.
//!
//! El `ml_api` solo acepta la imagen por URL (`GET /p/?img=`), asi que LabNAS le entrega
//! el cuadro por un enlace de un solo uso que caduca en segundos (`serve_frame`).
//!
//! Un cuadro suelto no basta para avisar: `Smoother` replica la decision de Obico
//! (media exponencial y medias moviles de la suma de confianzas, sin avisar en los
//! primeros cuadros de cada impresion, y un umbral mas exigente para pausar que para
//! avisar). Parametros de Obico (`FD_1ST_GEN_PARAMS`), calibrados para un cuadro cada 10 s.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use super::vision::Detection;
use super::{bad, ApiError};
use crate::db::db_op;
use crate::models::notifications::UserRole;
use crate::state::{AppState, SessionInfo};

const URL_SETTING: &str = "obico_url";
const LABNAS_URL_SETTING: &str = "obico_labnas_url";
const TOKEN_SETTING: &str = "obico_token";
/// `setup-obico.sh` lo deja escuchando solo en el propio servidor
const DEFAULT_URL: &str = "http://127.0.0.1:3333";
const DEFAULT_LABNAS_URL: &str = "http://127.0.0.1:3001";
/// El enlace del cuadro sirve una vez y por poco tiempo
const FRAME_TTL: Duration = Duration::from_secs(60);

// ─── Configuracion ───

#[derive(Debug, Clone)]
struct Config {
    url: String,
    labnas_url: String,
    token: Option<String>,
}

async fn config(state: &AppState) -> Config {
    db_op(&state.db, |conn| {
        Ok(Config {
            url: crate::db::get_setting(conn, URL_SETTING).filter(|v| !v.is_empty()).unwrap_or_else(|| DEFAULT_URL.into()),
            labnas_url: crate::db::get_setting(conn, LABNAS_URL_SETTING)
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| DEFAULT_LABNAS_URL.into()),
            token: crate::db::get_secret_setting(conn, TOKEN_SETTING).filter(|v| !v.is_empty()),
        })
    })
    .await
    .unwrap_or(Config { url: DEFAULT_URL.into(), labnas_url: DEFAULT_LABNAS_URL.into(), token: None })
}

#[derive(Serialize)]
pub struct ObicoStatus {
    pub url: String,
    pub labnas_url: String,
    pub has_token: bool,
    /// `GET /hc/` respondio "ok"
    pub ready: bool,
    pub error: Option<String>,
}

async fn status(state: &AppState) -> ObicoStatus {
    let c = config(state).await;
    let hc = state
        .http_client
        .get(format!("{}/hc/", c.url.trim_end_matches('/')))
        .timeout(Duration::from_secs(3))
        .send()
        .await;
    let (ready, error) = match hc {
        Ok(r) if r.status().is_success() => match r.text().await.unwrap_or_default().trim() {
            "ok" => (true, None),
            other => (false, Some(format!("Obico responde pero el modelo no esta cargado ({})", other))),
        },
        Ok(r) => (false, Some(format!("Obico respondio {}", r.status()))),
        Err(_) => (false, Some(format!("Obico no responde en {} (ver docs/DETECCION.md)", c.url))),
    };
    ObicoStatus { url: c.url, labnas_url: c.labnas_url, has_token: c.token.is_some(), ready, error }
}

fn require_admin(s: &SessionInfo) -> Result<(), ApiError> {
    if s.role == UserRole::Admin {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Solo el administrador".into()))
    }
}

/// GET /api/cameras/obico
pub async fn get_obico(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<ObicoStatus>, ApiError> {
    require_admin(&session)?;
    Ok(Json(status(&state).await))
}

#[derive(Deserialize)]
pub struct ObicoInput {
    pub url: String,
    pub labnas_url: String,
    /// None: no cambiar; "": quitar
    pub token: Option<String>,
}

fn valid_http(url: &str) -> bool {
    let u = url.trim();
    (u.starts_with("http://") || u.starts_with("https://")) && u.len() <= 300 && !u.contains(char::is_whitespace)
}

/// PUT /api/cameras/obico
pub async fn set_obico(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(input): Json<ObicoInput>,
) -> Result<Json<ObicoStatus>, ApiError> {
    require_admin(&session)?;
    if !valid_http(&input.url) || !valid_http(&input.labnas_url) {
        return Err(bad("Las direcciones deben empezar con http:// o https://"));
    }
    let (url, labnas_url, token) = (input.url.trim().to_string(), input.labnas_url.trim().to_string(), input.token);
    db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, URL_SETTING, &url)?;
        crate::db::set_setting(conn, LABNAS_URL_SETTING, &labnas_url)?;
        match token.as_deref().map(str::trim) {
            Some("") => {
                conn.execute("DELETE FROM settings WHERE key = ?1", [TOKEN_SETTING]).map_err(|e| e.to_string())?;
            }
            Some(t) => crate::db::set_secret_setting(conn, TOKEN_SETTING, t)?,
            None => {}
        }
        Ok(())
    })
    .await?;
    state.log_activity("Camara", "Cambio la conexion con Obico (impresion fallida)", &session.username).await;
    Ok(Json(status(&state).await))
}

// ─── Enlace de un solo uso para el cuadro ───

static FRAMES: LazyLock<StdMutex<HashMap<String, (Bytes, Instant)>>> = LazyLock::new(Default::default);

fn put_frame(jpeg: Bytes) -> String {
    let token = uuid::Uuid::new_v4().simple().to_string();
    if let Ok(mut m) = FRAMES.lock() {
        m.retain(|_, (_, at)| at.elapsed() < FRAME_TTL);
        m.insert(token.clone(), (jpeg, Instant::now()));
    }
    token
}

fn take_frame(token: &str) -> Option<Bytes> {
    let mut m = FRAMES.lock().ok()?;
    let (jpeg, at) = m.remove(token)?;
    (at.elapsed() < FRAME_TTL).then_some(jpeg)
}

/// GET /api/cameras/frame/{token} — publica: la pide el `ml_api` de Obico. El token es
/// aleatorio, sirve una sola vez y caduca en un minuto.
pub async fn serve_frame(Path(token): Path<String>) -> impl IntoResponse {
    match take_frame(&token) {
        Some(jpeg) => ([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "no-store")], jpeg).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

// ─── Deteccion ───

/// Detecciones de Obico: `[etiqueta, confianza, [centro x, centro y, ancho, alto]]` en pixeles
fn parse_detections(body: &serde_json::Value) -> Vec<Detection> {
    body["detections"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|d| {
                    let score = d.get(1)?.as_f64()? as f32;
                    let b = d.get(2)?.as_array()?;
                    let n = |i: usize| b.get(i).and_then(|v| v.as_f64()).map(|v| v as f32);
                    let (xc, yc, w, h) = (n(0)?, n(1)?, n(2)?, n(3)?);
                    Some(Detection {
                        label: "falla".into(),
                        score,
                        x: (xc - w / 2.0).max(0.0),
                        y: (yc - h / 2.0).max(0.0),
                        w,
                        h,
                        class: 0,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Manda el cuadro al `ml_api` y devuelve lo que vio en ese cuadro (sin suavizar)
pub async fn detect(state: &AppState, jpeg: Bytes) -> Result<Vec<Detection>, String> {
    let c = config(state).await;
    let token = put_frame(jpeg);
    let img = format!("{}/api/cameras/frame/{}", c.labnas_url.trim_end_matches('/'), token);
    let mut req = state
        .http_client
        .get(format!("{}/p/", c.url.trim_end_matches('/')))
        .query(&[("img", img.as_str())])
        .timeout(Duration::from_secs(30));
    if let Some(t) = &c.token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await;
    // si Obico no lo pidio, que no quede dando vueltas
    take_frame(&token);
    let resp = resp.map_err(|_| format!("Obico no responde en {} (ver docs/DETECCION.md)", c.url))?;
    match resp.status() {
        s if s.is_success() => {}
        StatusCode::UNAUTHORIZED => return Err("Obico rechazo el token (revisa el token en la conexion con Obico)".into()),
        s => {
            let msg: serde_json::Value = resp.json().await.unwrap_or_default();
            return Err(match msg["message"].as_str() {
                Some(m) => format!("Obico respondio {}: {}", s, m),
                // 400 sin mensaje: casi siempre no pudo descargar el cuadro desde LabNAS
                None if s == StatusCode::BAD_REQUEST => {
                    format!("Obico no pudo bajar la imagen desde {} (revisa «Direccion de LabNAS para Obico»)", c.labnas_url)
                }
                None => format!("Obico respondio {}", s),
            });
        }
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("Respuesta de Obico invalida: {}", e))?;
    Ok(parse_detections(&body))
}

// ─── Suavizado en el tiempo (como Obico) ───

const EWM_ALPHA: f32 = 2.0 / (12.0 + 1.0);
const ROLLING_WIN_SHORT: u32 = 310;
const ROLLING_WIN_LONG: u32 = 7200;
/// Cuadros de cada impresion en que no se avisa (aprende como se ve normalmente)
pub const INIT_SAFE_FRAMES: u32 = 30;
/// En Obico THRESHOLD_HIGH / THRESHOLD_LOW = 0.78 / 0.38
const HIGH_OVER_LOW: f32 = 0.78 / 0.38;
const ROLLING_MEAN_SHORT_MULTIPLE: f32 = 3.8;
/// Pausar exige mas que avisar
const PAUSE_FACTOR: f32 = 1.75;

#[derive(Debug, Default, Clone)]
pub struct Smoother {
    ewm: f32,
    short: f32,
    /// Media de toda la vida de la regla: lo «normal» de esa camara
    long: f32,
    frames: u32,
    lifetime: u32,
}

fn next_rolling_mean(p: f32, current: f32, count: u32, win: u32) -> f32 {
    current + (p - current) / (if win <= count { win } else { count + 1 }) as f32
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Verdict {
    Ok,
    Alert,
    Pause,
}

impl Smoother {
    /// Nueva impresion: olvida lo de la anterior (no lo normal de la camara)
    pub fn new_print(&mut self) {
        self.ewm = 0.0;
        self.short = 0.0;
        self.frames = 0;
    }

    /// Agrega un cuadro (`p` = suma de las confianzas de Obico). `low`: umbral de aviso
    pub fn push(&mut self, p: f32, low: f32) -> Verdict {
        self.frames += 1;
        self.lifetime += 1;
        self.ewm = p * EWM_ALPHA + self.ewm * (1.0 - EWM_ALPHA);
        self.short = next_rolling_mean(p, self.short, self.frames, ROLLING_WIN_SHORT);
        self.long = next_rolling_mean(p, self.long, self.lifetime, ROLLING_WIN_LONG);
        if self.failing(low, PAUSE_FACTOR) {
            Verdict::Pause
        } else if self.failing(low, 1.0) {
            Verdict::Alert
        } else {
            Verdict::Ok
        }
    }

    fn failing(&self, low: f32, factor: f32) -> bool {
        if self.frames < INIT_SAFE_FRAMES {
            return false;
        }
        let adj = (self.ewm - self.long) / factor;
        if adj < low {
            return false;
        }
        adj > low * HIGH_OVER_LOW || adj > (self.short - self.long) * ROLLING_MEAN_SHORT_MULTIPLE
    }

    /// Nivel actual de 0 a 1 (1 = umbral alto de aviso), para mostrarlo
    pub fn level(&self, low: f32) -> f32 {
        ((self.ewm - self.long) / (low * HIGH_OVER_LOW)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecciones_de_obico() {
        let body = serde_json::json!({ "detections": [["failure", 0.6, [100.0, 80.0, 40.0, 20.0]], ["failure", "x"]] });
        let d = parse_detections(&body);
        assert_eq!(d.len(), 1, "la mal formada se ignora");
        assert_eq!((d[0].x, d[0].y, d[0].w, d[0].h), (80.0, 70.0, 40.0, 20.0));
        assert!((d[0].score - 0.6).abs() < 1e-6);
        assert!(parse_detections(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn no_avisa_al_empezar_ni_sin_espagueti() {
        let mut s = Smoother::default();
        for _ in 0..INIT_SAFE_FRAMES - 1 {
            assert_eq!(s.push(3.0, 0.38), Verdict::Ok, "los primeros cuadros nunca avisan");
        }
        let mut s = Smoother::default();
        for _ in 0..500 {
            assert_eq!(s.push(0.05, 0.38), Verdict::Ok, "ruido bajo constante");
        }
    }

    #[test]
    fn espagueti_sostenido_avisa_y_luego_pausa() {
        let mut s = Smoother::default();
        for _ in 0..200 {
            s.push(0.02, 0.38);
        }
        s.new_print();
        for _ in 0..INIT_SAFE_FRAMES {
            assert_eq!(s.push(0.02, 0.38), Verdict::Ok);
        }
        let verdicts: Vec<Verdict> = (0..40).map(|_| s.push(2.0, 0.38)).collect();
        let first_alert = verdicts.iter().position(|v| *v != Verdict::Ok).expect("debe avisar");
        let first_pause = verdicts.iter().position(|v| *v == Verdict::Pause).expect("debe pausar");
        assert!(first_alert < first_pause, "avisa antes de pausar: {:?}", verdicts);
        assert!(s.level(0.38) > 0.9);
        // Un umbral mas alto (menos sensible) tarda mas
        let mut t = Smoother::default();
        for _ in 0..INIT_SAFE_FRAMES {
            t.push(0.02, 0.6);
        }
        let later = (0..40).position(|_| t.push(2.0, 0.6) != Verdict::Ok).unwrap();
        assert!(later > first_alert);
    }

    #[test]
    fn enlace_de_un_solo_uso() {
        let t = put_frame(Bytes::from_static(b"jpg"));
        assert_eq!(take_frame(&t).as_deref(), Some(&b"jpg"[..]));
        assert!(take_frame(&t).is_none(), "solo una vez");
        assert!(take_frame("inventado").is_none());
    }
}
