//! Deteccion con IA en camaras: reglas por camara (personas, objetos, movimiento) con
//! horario, y acciones al detectar (foto marcada, aviso por Telegram con foto, clip).
//!
//! Todo corre en el NAS: las imagenes no salen del laboratorio. El modelo (YOLOX-nano,
//! Apache-2.0) no viene en el binario: se descarga al activar una regla que lo usa, se
//! verifica su huella y se guarda en ~/.labnas/models. Las detecciones no se borran
//! (trazabilidad).
//!
//! Es vigilancia: solo la ven los admins y las personas que un admin autoriza (setting
//! `cameras_watchers`). Eso incluye reglas, historial y las fotos y clips de la IA.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Datelike, Timelike};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use super::vision::{self, Detection, Yolox};
use super::{bad, find_camera, hub, insert_record, safe_component, target_dir, ApiError, CameraRow};
use crate::db::db_op;
use crate::events::Audience;
use crate::state::{AppState, SessionInfo};
use crate::error::internal;

const MODEL_URL: &str = "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_nano.onnx";
const MODEL_SHA256: &str = "c789161ed43c8269fcd4e67c67eeeb4e80c622da2eb296a20bc6007bd18a0b7d";
const MODEL_FILE: &str = "yolox_nano.onnx";
const MODEL_MAX_BYTES: usize = 20 * 1024 * 1024;

pub const KINDS: &[(&str, &str)] = &[
    ("personas", "Personas"),
    ("objetos", "Objetos"),
    ("movimiento", "Movimiento"),
    ("falla_impresion", "Impresión fallida (Obico)"),
];
const FAILED_PRINT: &str = "falla_impresion";
/// Lunes = 0 ... domingo = 6
const DAY_NAMES: [&str; 7] = ["lun", "mar", "mie", "jue", "vie", "sab", "dom"];

/// El supervisor recarga las reglas al cambiar
static RULES_CHANGED: LazyLock<Notify> = LazyLock::new(Notify::new);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Schedule {
    /// 0 = lunes ... 6 = domingo
    pub days: Vec<u8>,
    /// "HH:MM"; si `to` <= `from` la ventana cruza la medianoche
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Actions {
    #[serde(default)]
    pub photo: bool,
    #[serde(default)]
    pub telegram: bool,
    /// Segundos de video al detectar (0 = no grabar)
    #[serde(default)]
    pub clip_s: u32,
    /// Impresion fallida: impresora que mira la camara (solo se analiza mientras imprime)
    #[serde(default)]
    pub printer_id: Option<String>,
    /// Impresion fallida: pausarla cuando la falla es clara (exige mas que el aviso)
    #[serde(default)]
    pub pause_printer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleInput {
    pub camera_id: String,
    pub kind: String,
    /// Objetos: clases COCO (nombre en ingles o español)
    #[serde(default)]
    pub classes: Vec<String>,
    /// Confianza minima (0..1); en movimiento, fraccion de la imagen que debe cambiar
    pub min_score: f32,
    pub interval_s: u32,
    pub schedule: Option<Schedule>,
    pub actions: Actions,
    pub cooldown_s: u32,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub id: String,
    #[serde(flatten)]
    pub input: RuleInput,
}

fn minutes(hhmm: &str) -> Option<u32> {
    let (h, m) = hhmm.split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60 && hhmm.len() == 5).then_some(h * 60 + m)
}

/// ¿Esta `now` dentro del horario? Sin horario: siempre
pub fn in_schedule(s: Option<&Schedule>, now: chrono::NaiveDateTime) -> bool {
    let Some(s) = s else { return true };
    let (Some(from), Some(to)) = (minutes(&s.from), minutes(&s.to)) else { return false };
    let t = now.hour() * 60 + now.minute();
    let today = now.weekday().num_days_from_monday() as u8;
    let yesterday = (today + 6) % 7;
    if from < to {
        s.days.contains(&today) && t >= from && t < to
    } else {
        // Cruza la medianoche: la ventana pertenece al dia en que empieza
        (s.days.contains(&today) && t >= from) || (s.days.contains(&yesterday) && t < to)
    }
}

fn validate(r: &RuleInput) -> Result<(), ApiError> {
    if !KINDS.iter().any(|(k, _)| *k == r.kind) {
        return Err(bad("Tipo de deteccion invalido"));
    }
    if r.kind == "objetos" {
        if r.classes.is_empty() || r.classes.len() > 80 {
            return Err(bad("Elige que objetos detectar"));
        }
        if let Some(c) = r.classes.iter().find(|c| vision::class_index(c).is_none()) {
            return Err(bad(&format!("Objeto desconocido: {}", c)));
        }
    }
    if r.kind == FAILED_PRINT {
        if r.actions.pause_printer && r.actions.printer_id.as_deref().unwrap_or("").is_empty() {
            return Err(bad("Para pausar, elige la impresora"));
        }
        if r.interval_s < 5 {
            return Err(bad("Para impresion fallida, analiza cada 5 segundos o mas (Obico usa 10)"));
        }
    }
    let (lo, hi) = if r.kind == "movimiento" { (0.005, 0.8) } else { (0.1, 0.95) };
    if !(lo..=hi).contains(&r.min_score) {
        return Err(bad("Umbral fuera de rango"));
    }
    if !(1..=60).contains(&r.interval_s) {
        return Err(bad("El intervalo debe ser de 1 a 60 segundos"));
    }
    if !(10..=3600).contains(&r.cooldown_s) {
        return Err(bad("La pausa entre avisos debe ser de 10 s a 1 hora"));
    }
    if r.actions.clip_s > 120 {
        return Err(bad("El clip puede durar hasta 120 segundos"));
    }
    if let Some(s) = &r.schedule {
        if minutes(&s.from).is_none() || minutes(&s.to).is_none() || s.from == s.to {
            return Err(bad("Horario invalido (HH:MM)"));
        }
        if s.days.is_empty() || s.days.iter().any(|d| *d > 6) {
            return Err(bad("Elige los dias del horario"));
        }
    }
    Ok(())
}

// ─── Base de datos ───

const COLS: &str = "id, camera_id, kind, classes, min_score, interval_s, schedule, actions, cooldown_s, enabled";

fn row_to_rule(r: &rusqlite::Row) -> rusqlite::Result<Rule> {
    let classes: String = r.get(3)?;
    let schedule: Option<String> = r.get(6)?;
    let actions: String = r.get(7)?;
    Ok(Rule {
        id: r.get(0)?,
        input: RuleInput {
            camera_id: r.get(1)?,
            kind: r.get(2)?,
            classes: serde_json::from_str(&classes).unwrap_or_default(),
            min_score: r.get::<_, f64>(4)? as f32,
            interval_s: r.get(5)?,
            schedule: schedule.and_then(|s| serde_json::from_str(&s).ok()),
            actions: serde_json::from_str(&actions).unwrap_or_default(),
            cooldown_s: r.get(8)?,
            enabled: r.get::<_, i64>(9)? != 0,
        },
    })
}

async fn load_rules(state: &AppState) -> Result<Vec<Rule>, ApiError> {
    db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare(&format!("SELECT {} FROM camera_detectors ORDER BY created_at", COLS))
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_rule).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .await
}

// ─── Modelo ───

#[derive(Default, Clone, Serialize)]
pub struct ModelStatus {
    pub ready: bool,
    pub downloading: bool,
    pub error: Option<String>,
}

static MODEL: LazyLock<StdMutex<Option<Arc<Yolox>>>> = LazyLock::new(Default::default);
static MODEL_STATUS: LazyLock<StdMutex<ModelStatus>> = LazyLock::new(Default::default);
static MODEL_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);

fn model_path() -> PathBuf {
    crate::db::data_dir().join("models").join(MODEL_FILE)
}

fn model_status() -> ModelStatus {
    let mut s = MODEL_STATUS.lock().map(|s| s.clone()).unwrap_or_default();
    s.ready = MODEL.lock().is_ok_and(|m| m.is_some()) || model_path().is_file();
    s
}

fn set_status(f: impl FnOnce(&mut ModelStatus)) {
    if let Ok(mut s) = MODEL_STATUS.lock() {
        f(&mut s);
    }
}

/// Modelo listo para usar: lo descarga (una vez, verificando la huella) y lo carga
async fn ensure_model(state: &AppState) -> Result<Arc<Yolox>, String> {
    if let Some(m) = MODEL.lock().ok().and_then(|m| m.clone()) {
        return Ok(m);
    }
    let _guard = MODEL_LOCK.lock().await;
    if let Some(m) = MODEL.lock().ok().and_then(|m| m.clone()) {
        return Ok(m);
    }
    let path = model_path();
    if !path.is_file() {
        set_status(|s| {
            s.downloading = true;
            s.error = None;
        });
        let res = download_model(state, &path).await;
        set_status(|s| {
            s.downloading = false;
            s.error = res.as_ref().err().cloned();
        });
        res?;
    }
    let p2 = path.clone();
    let model = tokio::task::spawn_blocking(move || Yolox::load(&p2))
        .await
        .map_err(|e| e.to_string())?
        .inspect_err(|e| set_status(|s| s.error = Some(e.clone())))?;
    let model = Arc::new(model);
    if let Ok(mut m) = MODEL.lock() {
        *m = Some(model.clone());
    }
    set_status(|s| s.error = None);
    Ok(model)
}

async fn download_model(state: &AppState, path: &std::path::Path) -> Result<(), String> {
    let resp = state
        .http_client
        .get(MODEL_URL)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| format!("No se pudo descargar el modelo: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("No se pudo descargar el modelo (HTTP {})", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("Descarga del modelo interrumpida: {}", e))?;
    if bytes.len() > MODEL_MAX_BYTES {
        return Err("El modelo descargado es demasiado grande".into());
    }
    if crate::ledger::sha256_hex(&bytes) != MODEL_SHA256 {
        return Err("El modelo descargado no coincide con su huella: se descarto".into());
    }
    let dir = path.parent().ok_or("ruta del modelo invalida")?;
    tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, &bytes).await.map_err(|e| e.to_string())?;
    tokio::fs::rename(&tmp, path).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Solo pruebas: reemplaza al modelo con detecciones fijas
#[cfg(test)]
pub static TEST_DETECTIONS: StdMutex<Option<Vec<Detection>>> = StdMutex::new(None);

/// Corre la regla sobre un cuadro. `prev`: cuadro gris anterior (movimiento)
async fn analyze(state: &AppState, rule: &RuleInput, jpeg: axum::body::Bytes, prev: Option<Vec<u8>>) -> Result<(Vec<Detection>, Option<Vec<u8>>), String> {
    let kind = rule.kind.clone();
    if kind == FAILED_PRINT {
        return super::obico::detect(state, jpeg).await.map(|d| (d, None));
    }
    if kind != "movimiento" {
        #[cfg(test)]
        if let Some(d) = TEST_DETECTIONS.lock().unwrap().clone() {
            return Ok((d, None));
        }
        let model = ensure_model(state).await?;
        let classes: Vec<usize> = if kind == "personas" {
            vec![0]
        } else {
            rule.classes.iter().filter_map(|c| vision::class_index(c)).collect()
        };
        let min = rule.min_score;
        return tokio::task::spawn_blocking(move || {
            let img = vision::decode_jpeg(&jpeg)?;
            model.detect(&img, min, &|c| classes.contains(&c)).map(|d| (d, None))
        })
        .await
        .map_err(|e| e.to_string())?;
    }
    let min = rule.min_score;
    tokio::task::spawn_blocking(move || {
        let img = vision::decode_jpeg(&jpeg)?;
        let (_, _, gray) = vision::gray_small(&img);
        let ratio = prev.as_deref().map(|p| vision::motion_ratio(p, &gray)).unwrap_or(0.0);
        let hit = (ratio >= min).then(|| Detection {
            label: "movimiento".into(),
            score: ratio,
            x: 0.0,
            y: 0.0,
            w: img.w as f32,
            h: img.h as f32,
            class: usize::MAX,
        });
        Ok((hit.into_iter().collect(), Some(gray)))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ─── Supervisor ───

#[derive(Default, Clone, Serialize)]
pub struct Runtime {
    pub last_fire_at: Option<String>,
    pub last_labels: Option<String>,
    pub last_error: Option<String>,
    #[serde(skip)]
    last_run: Option<Instant>,
    #[serde(skip)]
    last_fire: Option<Instant>,
    #[serde(skip)]
    prev_gray: Option<Vec<u8>>,
    /// Impresion fallida: nivel actual (0 a 1) y estado del suavizado
    pub fail_level: Option<f32>,
    #[serde(skip)]
    smoother: super::obico::Smoother,
    #[serde(skip)]
    was_printing: bool,
    #[serde(skip)]
    paused: bool,
}

static RUNTIME: LazyLock<StdMutex<HashMap<String, Runtime>>> = LazyLock::new(Default::default);

fn runtime(id: &str) -> Runtime {
    RUNTIME.lock().ok().and_then(|m| m.get(id).cloned()).unwrap_or_default()
}

fn update_runtime(id: &str, f: impl FnOnce(&mut Runtime)) {
    if let Ok(mut m) = RUNTIME.lock() {
        f(m.entry(id.to_string()).or_default());
    }
}

fn kind_label(r: &RuleInput) -> String {
    match r.kind.as_str() {
        "personas" => "personas".into(),
        "movimiento" => "movimiento".into(),
        FAILED_PRINT => "impresion fallida".into(),
        _ => r
            .classes
            .iter()
            .filter_map(|c| vision::class_index(c).map(|i| vision::COCO[i].1))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Bucle del modulo: cada segundo revisa las reglas activas y dentro de horario
pub async fn detection_loop(state: AppState) {
    let mut rules: Vec<Rule> = Vec::new();
    let mut cameras: HashMap<String, CameraRow> = HashMap::new();
    let mut reload_at = Instant::now();
    loop {
        if Instant::now() >= reload_at {
            rules = load_rules(&state).await.unwrap_or_default();
            cameras = super::all_cameras(&state)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect();
            reload_at = Instant::now() + Duration::from_secs(30);
        }

        let now = chrono::Local::now().naive_local();
        let active: Vec<&Rule> = rules
            .iter()
            .filter(|r| r.input.enabled && in_schedule(r.input.schedule.as_ref(), now) && cameras.contains_key(&r.input.camera_id))
            .collect();

        for rule in active {
            let cam = &cameras[&rule.input.camera_id];
            let rt = runtime(&rule.id);
            let failed_print = rule.input.kind == FAILED_PRINT;
            if failed_print {
                // Solo mientras imprime; si no se sabe (modulo de impresoras apagado), se analiza
                let printing = rule
                    .input
                    .actions
                    .printer_id
                    .as_deref()
                    .and_then(crate::handlers::printers3d::printing_now)
                    .unwrap_or(true);
                let started = printing && !rt.was_printing;
                update_runtime(&rule.id, |r| {
                    r.was_printing = printing;
                    if started {
                        r.smoother.new_print();
                        r.paused = false;
                    }
                    if !printing {
                        r.fail_level = None;
                    }
                });
                if !printing {
                    continue;
                }
            }
            // Mantener la camara encendida mientras la regla esta activa (y, en impresion
            // fallida, mientras imprime)
            let h = hub::ensure(&state, cam.spec());
            h.touch_lease();
            if rt.last_run.is_some_and(|t| t.elapsed() < Duration::from_secs(rule.input.interval_s as u64)) {
                continue;
            }
            update_runtime(&rule.id, |r| r.last_run = Some(Instant::now()));
            let Some(frame) = h.latest(Duration::from_secs(5)) else { continue };
            match analyze(&state, &rule.input, frame.clone(), rt.prev_gray.clone()).await {
                Ok((dets, gray)) => {
                    update_runtime(&rule.id, |r| {
                        r.last_error = None;
                        if gray.is_some() {
                            r.prev_gray = gray;
                        }
                    });
                    let cooled = rt.last_fire.is_none_or(|t| t.elapsed() >= Duration::from_secs(rule.input.cooldown_s as u64));
                    // Impresion fallida: decide el suavizado, no un cuadro suelto
                    let (hit, pause) = if failed_print {
                        let p: f32 = dets.iter().map(|d| d.score).sum();
                        let low = rule.input.min_score;
                        let mut verdict = super::obico::Verdict::Ok;
                        update_runtime(&rule.id, |r| {
                            verdict = r.smoother.push(p, low);
                            r.fail_level = Some(r.smoother.level(low));
                        });
                        (verdict != super::obico::Verdict::Ok, verdict == super::obico::Verdict::Pause)
                    } else {
                        (!dets.is_empty(), false)
                    };
                    // La pausa no espera la pausa entre avisos (una vez por impresion)
                    let pause = pause && rule.input.actions.pause_printer && !rt.paused;
                    if (hit && cooled) || pause {
                        update_runtime(&rule.id, |r| {
                            r.last_fire = Some(Instant::now());
                            r.paused |= pause;
                        });
                        fire(&state, rule, cam, frame, dets, pause).await;
                    }
                }
                Err(e) => update_runtime(&rule.id, |r| r.last_error = Some(e)),
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            _ = RULES_CHANGED.notified() => { reload_at = Instant::now(); }
        }
    }
}

fn summary(dets: &[Detection]) -> String {
    let mut counts: Vec<(String, usize, f32)> = Vec::new();
    for d in dets {
        match counts.iter_mut().find(|c| c.0 == d.label) {
            Some(c) => {
                c.1 += 1;
                c.2 = c.2.max(d.score);
            }
            None => counts.push((d.label.clone(), 1, d.score)),
        }
    }
    counts
        .iter()
        .map(|(l, n, s)| if *n > 1 { format!("{} {} ({:.0}%)", n, l, s * 100.0) } else { format!("{} ({:.0}%)", l, s * 100.0) })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Acciones al detectar: foto marcada, Telegram, clip; queda registrado y auditado
async fn fire(state: &AppState, rule: &Rule, cam: &CameraRow, frame: axum::body::Bytes, dets: Vec<Detection>, pause: bool) {
    let who = format!("IA ({})", rule.input.kind);
    let mut labels = summary(&dets);
    if rule.input.kind == FAILED_PRINT {
        if labels.is_empty() {
            labels = "falla sostenida".into();
        }
        if pause {
            labels.push_str(&format!(" · {}", pause_printer(state, rule.input.actions.printer_id.as_deref().unwrap_or("")).await));
        }
    }
    let best = dets.iter().map(|d| d.score).fold(0.0f32, f32::max);
    let at = chrono::Local::now();

    // Foto con las detecciones marcadas (si falla el marcado, la original)
    let boxes: Vec<Detection> = dets.iter().filter(|d| d.class != usize::MAX).cloned().collect();
    let marked = tokio::task::spawn_blocking({
        let frame = frame.clone();
        move || -> Option<Vec<u8>> {
            let mut img = vision::decode_jpeg(&frame).ok()?;
            vision::draw_boxes(&mut img, &boxes);
            vision::encode_jpeg(&img, 85).ok()
        }
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| frame.to_vec());

    let mut photo_id = None;
    let mut errors = Vec::new();
    if rule.input.actions.photo {
        match target_dir(state, cam).await {
            Ok((dir, _)) => {
                let path = dir.join(format!("{}_{}.jpg", at.format("%Y-%m-%d_%H-%M-%S"), safe_component(&format!("deteccion {}", rule.input.kind))));
                let saved = async {
                    tokio::fs::create_dir_all(&dir).await?;
                    tokio::fs::write(&path, &marked).await
                }
                .await;
                match saved {
                    Ok(()) => {
                        let rid = uuid::Uuid::new_v4().simple().to_string();
                        if insert_record(state, &rid, cam, "foto", &who, &path, Some(marked.len() as u64)).await.is_ok() {
                            photo_id = Some(rid);
                        }
                    }
                    Err(e) => errors.push(format!("foto: {}", e)),
                }
            }
            Err((_, e)) => errors.push(format!("foto: {}", e)),
        }
    }
    let mut clip_id = None;
    if rule.input.actions.clip_s > 0 {
        let h = hub::ensure(state, cam.spec());
        if h.recording().is_none() {
            match super::begin_recording(state, cam, &who, Some(Duration::from_secs(rule.input.actions.clip_s as u64))).await {
                Ok((rid, _)) => clip_id = Some(rid),
                Err((_, e)) => errors.push(format!("clip: {}", e)),
            }
        }
    }
    let watchers = watchers(state).await;
    // Resultado del aviso: (chats a los que llego, por que no llego)
    let mut tg: (Option<i64>, Option<String>) = (None, None);
    if rule.input.actions.telegram {
        // A los admins y a todas las personas autorizadas a ver la deteccion
        let extra: Vec<String> = watchers.clone();
        let caption = format!("🔎 {} en {}: {}\n{}", kind_title(&rule.input.kind), cam.name, labels, at.format("%d-%m-%Y %H:%M:%S"));
        tg = match crate::handlers::notifications::notify_admins_photo(state, marked, &caption, &extra).await {
            Ok(n) => (Some(n as i64), None),
            Err(e) => {
                errors.push(format!("telegram: {}", e));
                (Some(0), Some(e))
            }
        };
    }

    let (id, rid, cid, cname, kind, l2, at2) = (
        uuid::Uuid::new_v4().simple().to_string(),
        rule.id.clone(),
        cam.id.clone(),
        cam.name.clone(),
        rule.input.kind.clone(),
        labels.clone(),
        at.to_rfc3339(),
    );
    let (p2, c2) = (photo_id.clone(), clip_id.clone());
    let _ = db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO camera_detections (id, detector_id, camera_id, camera_name, kind, labels, score, at, photo_id, clip_id, tg_sent, tg_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![id, rid, cid, cname, kind, l2, best as f64, at2, p2, c2, tg.0, tg.1],
        )
        .map_err(|e| e.to_string())
    })
    .await;
    state.log_activity("Camara", &format!("{} detecto en {}: {}", who, cam.name, labels), "IA").await;
    let at_s = at.to_rfc3339();
    update_runtime(&rule.id, |r| {
        r.last_fire_at = Some(at_s);
        r.last_labels = Some(labels.clone());
        r.last_error = (!errors.is_empty()).then(|| errors.join("; "));
    });
    // Solo a quienes pueden ver la deteccion
    let data = serde_json::json!({ "camera_id": cam.id, "camera": cam.name, "kind": rule.input.kind, "labels": labels });
    let title = format!("{} en {}", kind_title(&rule.input.kind), cam.name);
    let audiences = std::iter::once(Audience::Admins).chain(watchers.iter().map(|w| Audience::User(w.clone())));
    for a in audiences {
        state.events.publish("cameras.detection", data.clone(), a.clone(), Some("cameras"));
        crate::events::notify(state, a, Some("cameras"), crate::events::Level::Warning, &title, &labels);
    }
}

/// Pausa la impresora por una falla; devuelve el resultado para el aviso
async fn pause_printer(state: &AppState, printer_id: &str) -> String {
    let req = crate::models::printers3d::ControlPrintRequest { command: "pause".into() };
    let r = crate::handlers::printers3d::control_print(State(state.clone()), Path(printer_id.to_string()), Json(req)).await;
    let name = crate::handlers::printers3d::list_printers(State(state.clone()))
        .await
        .ok()
        .and_then(|Json(l)| l.into_iter().find(|p| p.id == printer_id).map(|p| p.name))
        .unwrap_or_else(|| printer_id.to_string());
    match r {
        Ok(_) => {
            state.log_activity("Impresoras 3D", &format!("{} pausada por posible impresion fallida", name), "IA").await;
            format!("{} pausada", name)
        }
        Err((_, e)) => format!("no se pudo pausar {}: {}", name, e),
    }
}

fn kind_title(kind: &str) -> &'static str {
    match kind {
        "personas" => "Persona detectada",
        "movimiento" => "Movimiento detectado",
        FAILED_PRINT => "Posible impresion fallida",
        _ => "Objeto detectado",
    }
}

// ─── Quien puede ver ───

const WATCHERS_SETTING: &str = "cameras_watchers";

/// Personas (ademas de los admins) que pueden ver la deteccion
pub(crate) async fn watchers(state: &AppState) -> Vec<String> {
    db_op(&state.db, |conn| {
        Ok(crate::db::get_setting(conn, WATCHERS_SETTING).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
    })
    .await
    .unwrap_or_default()
}

pub(crate) async fn can_view(state: &AppState, s: &SessionInfo) -> bool {
    s.role.is_admin() || watchers(state).await.contains(&s.username)
}

async fn require_view(state: &AppState, s: &SessionInfo) -> Result<(), ApiError> {
    if can_view(state, s).await {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Solo el administrador y las personas autorizadas ven la deteccion".into()))
    }
}

/// Fotos y clips hechos por la IA (usuario "IA (...)")
pub(crate) fn is_ai_record(username: &str) -> bool {
    username.starts_with("IA (")
}

#[derive(Deserialize)]
pub struct WatchersInput {
    pub users: Vec<String>,
}

/// PUT /api/cameras/detectors/watchers (admin)
pub async fn set_watchers(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(input): Json<WatchersInput>,
) -> Result<Json<Vec<String>>, ApiError> {
    let mut users = input.users;
    users.sort();
    users.dedup();
    let u2 = users.clone();
    db_op(&state.db, move |conn| {
        for u in &u2 {
            let exists: bool = conn
                .query_row("SELECT COUNT(*) FROM web_users WHERE username = ?1", [u], |r| r.get::<_, i64>(0))
                .map_err(|e| e.to_string())?
                > 0;
            if !exists {
                return Err(format!("Usuario desconocido: {}", u));
            }
        }
        crate::db::set_setting(conn, WATCHERS_SETTING, &serde_json::to_string(&u2).map_err(|e| e.to_string())?)
    })
    .await
    .map_err(|(_, e)| (StatusCode::BAD_REQUEST, e))?;
    let list = if users.is_empty() { "solo administradores".to_string() } else { users.join(", ") };
    state.log_activity("Camara", &format!("Quienes ven la deteccion: {}", list), &session.username).await;
    Ok(Json(users))
}

// ─── API ───

#[derive(Serialize)]
pub struct RuleView {
    #[serde(flatten)]
    pub rule: Rule,
    pub camera: String,
    /// Habilitada y dentro de horario ahora
    pub active_now: bool,
    #[serde(flatten)]
    pub runtime: Runtime,
}

#[derive(Serialize)]
pub struct DetectorsResponse {
    pub rules: Vec<RuleView>,
    pub model: ModelStatus,
    pub kinds: Vec<(&'static str, &'static str)>,
    /// Objetos elegibles: (id en ingles, nombre en español)
    pub classes: Vec<(&'static str, &'static str)>,
    pub days: [&'static str; 7],
    /// Personas autorizadas (solo lo ve el admin)
    pub watchers: Vec<String>,
}

/// GET /api/cameras/detectors
pub async fn list_detectors(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<DetectorsResponse>, ApiError> {
    require_view(&state, &session).await?;
    let is_admin = session.role.is_admin();
    let rules = load_rules(&state).await?;
    let cams: HashMap<String, String> = super::all_cameras(&state).await?.into_iter().map(|c| (c.id, c.name)).collect();
    let now = chrono::Local::now().naive_local();
    let rules = rules
        .into_iter()
        .map(|r| RuleView {
            camera: cams.get(&r.input.camera_id).cloned().unwrap_or_else(|| "(camara eliminada)".into()),
            active_now: r.input.enabled && in_schedule(r.input.schedule.as_ref(), now),
            runtime: runtime(&r.id),
            rule: r,
        })
        .collect();
    Ok(Json(DetectorsResponse {
        rules,
        model: model_status(),
        kinds: KINDS.to_vec(),
        classes: vision::COCO.to_vec(),
        days: DAY_NAMES,
        watchers: if is_admin { watchers(&state).await } else { Vec::new() },
    }))
}

async fn save_rule(state: &AppState, session: &SessionInfo, id: Option<String>, input: RuleInput) -> Result<Json<Rule>, ApiError> {
    validate(&input)?;
    let cam = find_camera(state, &input.camera_id).await?;
    let is_new = id.is_none();
    let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
    let (i2, inp, user) = (id.clone(), input.clone(), session.username.clone());
    let changed = db_op(&state.db, move |conn| {
        let classes = serde_json::to_string(&inp.classes).map_err(|e| e.to_string())?;
        let schedule = inp.schedule.as_ref().map(serde_json::to_string).transpose().map_err(|e| e.to_string())?;
        let actions = serde_json::to_string(&inp.actions).map_err(|e| e.to_string())?;
        let (score, enabled) = (inp.min_score as f64, inp.enabled as i64);
        let n = if is_new {
            conn.execute(
                "INSERT INTO camera_detectors (id, camera_id, kind, classes, min_score, interval_s, schedule, actions, cooldown_s, enabled, created_by, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![i2, inp.camera_id, inp.kind, classes, score, inp.interval_s, schedule, actions, inp.cooldown_s, enabled, user, chrono::Local::now().to_rfc3339()],
            )
        } else {
            conn.execute(
                "UPDATE camera_detectors SET camera_id = ?2, kind = ?3, classes = ?4, min_score = ?5, interval_s = ?6, schedule = ?7,
                 actions = ?8, cooldown_s = ?9, enabled = ?10 WHERE id = ?1",
                params![i2, inp.camera_id, inp.kind, classes, score, inp.interval_s, schedule, actions, inp.cooldown_s, enabled],
            )
        };
        n.map_err(|e| e.to_string())
    })
    .await?;
    if changed == 0 {
        return Err((StatusCode::NOT_FOUND, "Regla no encontrada".into()));
    }
    let what = if is_new { "creo" } else { "cambio" };
    state
        .log_activity(
            "Camara",
            &format!("{} una deteccion de {} en {} ({})", what, kind_label(&input), cam.name, if input.enabled { "activa" } else { "pausada" }),
            &session.username,
        )
        .await;
    RULES_CHANGED.notify_one();
    // Si usa el modelo y falta, empezar a bajarlo ya
    if input.enabled && input.kind != "movimiento" && input.kind != FAILED_PRINT && !model_status().ready {
        let s = state.clone();
        tokio::spawn(async move {
            let _ = ensure_model(&s).await;
        });
    }
    Ok(Json(Rule { id, input }))
}

/// POST /api/cameras/detectors
pub async fn create_detector(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(input): Json<RuleInput>,
) -> Result<Json<Rule>, ApiError> {
    save_rule(&state, &session, None, input).await
}

/// PUT /api/cameras/detectors/{id}
pub async fn update_detector(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(input): Json<RuleInput>,
) -> Result<Json<Rule>, ApiError> {
    save_rule(&state, &session, Some(id), input).await
}

/// DELETE /api/cameras/detectors/{id} (el historial de detecciones se conserva)
pub async fn delete_detector(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let i2 = id.clone();
    let n = db_op(&state.db, move |conn| conn.execute("DELETE FROM camera_detectors WHERE id = ?1", [i2]).map_err(|e| e.to_string())).await?;
    if n == 0 {
        return Err((StatusCode::NOT_FOUND, "Regla no encontrada".into()));
    }
    if let Ok(mut m) = RUNTIME.lock() {
        m.remove(&id);
    }
    state.log_activity("Camara", &format!("Elimino una regla de deteccion ({})", id), &session.username).await;
    RULES_CHANGED.notify_one();
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct TestResult {
    pub detections: Vec<Detection>,
    /// Foto actual con las detecciones marcadas (JPEG en base64)
    pub image: String,
}

/// POST /api/cameras/detectors/test: corre una regla (aun sin guardar) sobre el cuadro
/// actual, sin horario ni acciones
pub async fn test_detector(State(state): State<AppState>, Json(input): Json<RuleInput>) -> Result<Json<TestResult>, ApiError> {
    validate(&input)?;
    let cam = find_camera(&state, &input.camera_id).await?;
    let frame = super::current_frame(&state, &cam).await?;
    let bad_gw = |e: String| (StatusCode::BAD_GATEWAY, e);
    let (dets, _) = if input.kind == "movimiento" {
        // Dos cuadros con un segundo de diferencia
        let (_, g) = analyze(&state, &input, frame.clone(), None).await.map_err(bad_gw)?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let f2 = super::current_frame(&state, &cam).await?;
        analyze(&state, &input, f2, g).await.map_err(bad_gw)?
    } else {
        analyze(&state, &input, frame.clone(), None).await.map_err(bad_gw)?
    };
    let boxes: Vec<Detection> = dets.iter().filter(|d| d.class != usize::MAX).cloned().collect();
    let image = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let mut img = vision::decode_jpeg(&frame)?;
        vision::draw_boxes(&mut img, &boxes);
        vision::encode_jpeg(&img, 80)
    })
    .await
    .map_err(internal)?
    .map_err(bad_gw)?;
    use base64::Engine;
    Ok(Json(TestResult { detections: dets, image: base64::engine::general_purpose::STANDARD.encode(image) }))
}

/// POST /api/cameras/ai/model: descarga y carga el modelo ahora
pub async fn prepare_model(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<ModelStatus>, ApiError> {
    ensure_model(&state).await.map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    state.log_activity("Camara", "Descargo el modelo de deteccion (YOLOX-nano)", &session.username).await;
    Ok(Json(model_status()))
}

#[derive(Serialize)]
pub struct DetectionRow {
    pub id: String,
    /// Regla que disparo
    pub detector_id: String,
    pub camera_id: String,
    pub camera_name: String,
    pub kind: String,
    pub labels: String,
    pub score: f64,
    pub at: String,
    pub photo_id: Option<String>,
    pub clip_id: Option<String>,
    /// Chats de Telegram a los que llego el aviso (null = la regla no avisa o es anterior a v3.4.2)
    pub tg_sent: Option<i64>,
    /// Por que no llego el aviso
    pub tg_error: Option<String>,
}

#[derive(Deserialize)]
pub struct DetectionsQuery {
    camera: Option<String>,
    /// Regla (detector)
    rule: Option<String>,
    /// Dia local `YYYY-MM-DD`
    day: Option<String>,
    /// Paginacion: solo las anteriores a este instante (el `at` de la ultima recibida)
    before: Option<String>,
    limit: Option<u32>,
}

/// Rango de texto `[dia, dia siguiente)` para comparar con `at` (RFC 3339 en hora local)
fn day_range(day: &str) -> Option<(String, String)> {
    let d = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    let next = d.succ_opt()?;
    Some((d.format("%Y-%m-%d").to_string(), next.format("%Y-%m-%d").to_string()))
}

/// GET /api/cameras/detections[?camera=id&rule=id&day=YYYY-MM-DD&before=at&limit=n]
pub async fn list_detections(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<DetectionsQuery>,
) -> Result<Json<Vec<DetectionRow>>, ApiError> {
    require_view(&state, &session).await?;
    let (from, to) = match q.day.as_deref().filter(|d| !d.is_empty()) {
        Some(d) => {
            let (a, b) = day_range(d).ok_or((StatusCode::BAD_REQUEST, "Dia invalido (AAAA-MM-DD)".to_string()))?;
            (Some(a), Some(b))
        }
        None => (None, None),
    };
    let limit = q.limit.unwrap_or(200).clamp(1, 500);
    let rows = db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, detector_id, camera_id, camera_name, kind, labels, score, at, photo_id, clip_id, tg_sent, tg_error FROM camera_detections
                 WHERE (?1 IS NULL OR camera_id = ?1) AND (?2 IS NULL OR detector_id = ?2)
                   AND (?3 IS NULL OR at >= ?3) AND (?4 IS NULL OR at < ?4) AND (?5 IS NULL OR at < ?5)
                 ORDER BY at DESC LIMIT ?6",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![q.camera, q.rule, from, to, q.before, limit], |r| {
                Ok(DetectionRow {
                    id: r.get(0)?,
                    detector_id: r.get(1)?,
                    camera_id: r.get(2)?,
                    camera_name: r.get(3)?,
                    kind: r.get(4)?,
                    labels: r.get(5)?,
                    score: r.get(6)?,
                    at: r.get(7)?,
                    photo_id: r.get(8)?,
                    clip_id: r.get(9)?,
                    tg_sent: r.get(10)?,
                    tg_error: r.get(11)?,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .await?;
    Ok(Json(rows))
}

/// Al apagar el modulo
pub fn shutdown() {
    if let Ok(mut m) = RUNTIME.lock() {
        m.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32, hh: u32, mm: u32) -> chrono::NaiveDateTime {
        // 2026-11-16 es lunes
        chrono::NaiveDate::from_ymd_opt(2026, 11, 16 + day).unwrap().and_hms_opt(hh, mm, 0).unwrap()
    }

    #[test]
    fn horario_normal_y_cruzando_medianoche() {
        let oficina = Schedule { days: vec![0, 1, 2, 3, 4], from: "08:30".into(), to: "18:00".into() };
        assert!(in_schedule(Some(&oficina), at(0, 8, 30)));
        assert!(!in_schedule(Some(&oficina), at(0, 18, 0)));
        assert!(!in_schedule(Some(&oficina), at(5, 10, 0)), "sabado");
        assert!(in_schedule(None, at(6, 3, 0)), "sin horario: siempre");

        // Noches de lunes a viernes: 20:00 a 07:00 del dia siguiente
        let noche = Schedule { days: vec![0, 1, 2, 3, 4], from: "20:00".into(), to: "07:00".into() };
        assert!(in_schedule(Some(&noche), at(0, 23, 0)), "lunes en la noche");
        assert!(in_schedule(Some(&noche), at(1, 3, 0)), "madrugada del martes: ventana del lunes");
        assert!(in_schedule(Some(&noche), at(5, 6, 59)), "madrugada del sabado: ventana del viernes");
        assert!(!in_schedule(Some(&noche), at(5, 22, 0)), "sabado en la noche no");
        assert!(!in_schedule(Some(&noche), at(0, 6, 0)), "madrugada del lunes: el domingo no esta");
        assert!(!in_schedule(Some(&noche), at(2, 12, 0)));
    }

    #[test]
    fn validacion() {
        let base = || RuleInput {
            camera_id: "c".into(),
            kind: "personas".into(),
            classes: vec![],
            min_score: 0.5,
            interval_s: 2,
            schedule: None,
            actions: Actions { photo: true, telegram: true, clip_s: 10, ..Default::default() },
            cooldown_s: 60,
            enabled: true,
        };
        assert!(validate(&base()).is_ok());
        assert!(validate(&RuleInput { kind: "otra".into(), ..base() }).is_err());
        assert!(validate(&RuleInput { kind: "objetos".into(), ..base() }).is_err(), "objetos sin clases");
        assert!(validate(&RuleInput { kind: "objetos".into(), classes: vec!["notebook".into(), "backpack".into()], ..base() }).is_ok());
        assert!(validate(&RuleInput { kind: "objetos".into(), classes: vec!["dragon".into()], ..base() }).is_err());
        assert!(validate(&RuleInput { min_score: 0.01, ..base() }).is_err());
        assert!(validate(&RuleInput { kind: "movimiento".into(), min_score: 0.02, ..base() }).is_ok());
        assert!(validate(&RuleInput { interval_s: 0, ..base() }).is_err());
        assert!(validate(&RuleInput { cooldown_s: 5, ..base() }).is_err());
        assert!(validate(&RuleInput { actions: Actions { clip_s: 600, ..Default::default() }, ..base() }).is_err());
        let s = |from: &str, to: &str, days: Vec<u8>| Some(Schedule { days, from: from.into(), to: to.into() });
        assert!(validate(&RuleInput { schedule: s("20:00", "07:00", vec![0, 4]), ..base() }).is_ok());
        assert!(validate(&RuleInput { schedule: s("25:00", "07:00", vec![0]), ..base() }).is_err());
        assert!(validate(&RuleInput { schedule: s("08:00", "08:00", vec![0]), ..base() }).is_err());
        assert!(validate(&RuleInput { schedule: s("08:00", "09:00", vec![]), ..base() }).is_err());
        assert!(validate(&RuleInput { schedule: s("08:00", "09:00", vec![7]), ..base() }).is_err());
    }

    #[test]
    fn resumen_de_detecciones() {
        let d = |l: &str, s: f32| Detection { label: l.into(), score: s, x: 0.0, y: 0.0, w: 1.0, h: 1.0, class: 0 };
        assert_eq!(summary(&[d("persona", 0.81), d("persona", 0.9), d("perro", 0.55)]), "2 persona (90%), perro (55%)");
    }
}
