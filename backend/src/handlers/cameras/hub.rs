//! Un "hub" por camara: un solo ffmpeg que alimenta la vista en vivo (MJPEG) de
//! todos los que miran y, cuando se graba, tambien el archivo MP4. Las webcams USB
//! no se pueden abrir dos veces, asi que nada mas toca el dispositivo.
//!
//! El proceso vive mientras alguien mire (o haya pedido una foto hace poco) o se
//! este grabando; al empezar o terminar una grabacion se reinicia con las salidas
//! nuevas (la vista se corta un instante).

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, Notify};

use crate::state::AppState;

/// Sin espectadores ni grabacion, el proceso se cierra tras este tiempo
const IDLE_GRACE: Duration = Duration::from_secs(15);
/// Una foto pedida mantiene el hub vivo este tiempo (la grilla pide fotos cada segundo)
const LEASE: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_secs(3);
const MAX_FRAME: usize = 8 * 1024 * 1024;

/// De donde lee ffmpeg
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    V4l2 { device: String, size: Option<String> },
    Url(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Audio {
    None,
    /// Dispositivo ALSA del servidor (p.ej. plughw:CARD=Camera,DEV=0)
    Alsa(String),
    /// Audio que trae el propio stream de la camara IP
    Stream,
}

#[derive(Clone, Debug)]
pub struct CamSpec {
    pub id: String,
    pub name: String,
    pub input: Input,
    pub audio: Audio,
}

#[derive(Clone, Debug)]
pub struct ActiveRecording {
    pub recording_id: String,
    pub camera: String,
    pub path: PathBuf,
    pub username: String,
    pub started: Instant,
    pub started_at: String,
    pub max: Duration,
}

pub struct Hub {
    spec: StdMutex<CamSpec>,
    frames: broadcast::Sender<Bytes>,
    latest: StdMutex<Option<(Instant, Bytes)>>,
    viewers: AtomicUsize,
    lease: StdMutex<Option<Instant>>,
    recording: StdMutex<Option<ActiveRecording>>,
    error: StdMutex<Option<String>>,
    changed: Notify,
}

static HUBS: LazyLock<StdMutex<HashMap<String, Arc<Hub>>>> = LazyLock::new(Default::default);

impl Hub {
    fn wanted(&self) -> bool {
        self.viewers.load(Ordering::SeqCst) > 0
            || self.recording.lock().map(|r| r.is_some()).unwrap_or(false)
            || self.lease.lock().ok().and_then(|l| *l).is_some_and(|t| t.elapsed() < LEASE)
    }

    fn recording_id(&self) -> Option<String> {
        self.recording.lock().ok().and_then(|r| r.as_ref().map(|x| x.recording_id.clone()))
    }

    pub fn recording(&self) -> Option<ActiveRecording> {
        self.recording.lock().ok().and_then(|r| r.clone())
    }

    pub fn last_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|e| e.clone())
    }

    pub fn viewers(&self) -> usize {
        self.viewers.load(Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.frames.subscribe()
    }

    /// Ultimo cuadro si es reciente
    pub fn latest(&self, max_age: Duration) -> Option<Bytes> {
        self.latest.lock().ok().and_then(|l| l.as_ref().filter(|(t, _)| t.elapsed() < max_age).map(|(_, b)| b.clone()))
    }

    pub fn touch_lease(&self) {
        if let Ok(mut l) = self.lease.lock() {
            *l = Some(Instant::now());
        }
        self.changed.notify_one();
    }
}

/// Mantiene vivo el hub mientras exista (una vista en vivo abierta)
pub struct ViewerGuard(Arc<Hub>);

impl Drop for ViewerGuard {
    fn drop(&mut self) {
        self.0.viewers.fetch_sub(1, Ordering::SeqCst);
        self.0.changed.notify_one();
    }
}

pub fn add_viewer(hub: &Arc<Hub>) -> ViewerGuard {
    hub.viewers.fetch_add(1, Ordering::SeqCst);
    hub.changed.notify_one();
    ViewerGuard(hub.clone())
}

pub fn get(id: &str) -> Option<Arc<Hub>> {
    HUBS.lock().ok()?.get(id).cloned()
}

/// Hub de la camara (lo crea y arranca su supervisor si no existe)
pub fn ensure(state: &AppState, spec: CamSpec) -> Arc<Hub> {
    let mut hubs = HUBS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(h) = hubs.get(&spec.id) {
        // La configuracion pudo cambiar: aplica en el proximo arranque de ffmpeg
        if let Ok(mut s) = h.spec.lock() {
            *s = spec;
        }
        return h.clone();
    }
    let hub = Arc::new(Hub {
        spec: StdMutex::new(spec.clone()),
        frames: broadcast::channel(4).0,
        latest: StdMutex::new(None),
        viewers: AtomicUsize::new(0),
        lease: StdMutex::new(Some(Instant::now())),
        recording: StdMutex::new(None),
        error: StdMutex::new(None),
        changed: Notify::new(),
    });
    hubs.insert(spec.id.clone(), hub.clone());
    tokio::spawn(supervise(state.clone(), hub.clone(), spec.id));
    hub
}

/// Empieza a grabar (falla si ya se esta grabando)
pub fn start_recording(hub: &Arc<Hub>, rec: ActiveRecording) -> Result<(), String> {
    let mut r = hub.recording.lock().map_err(|_| "estado corrupto".to_string())?;
    if let Some(cur) = r.as_ref() {
        return Err(format!("Ya se esta grabando (por {})", cur.username));
    }
    *r = Some(rec);
    drop(r);
    hub.changed.notify_one();
    Ok(())
}

/// Detiene la grabacion; el supervisor cierra el archivo y actualiza la base
pub fn stop_recording(hub: &Arc<Hub>) -> Option<ActiveRecording> {
    let taken = hub.recording.lock().ok().and_then(|mut r| r.take());
    hub.changed.notify_one();
    taken
}

/// Grabaciones en curso (camara, grabacion)
pub fn active_recordings() -> Vec<(String, ActiveRecording)> {
    let hubs = HUBS.lock().map(|h| h.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>()).unwrap_or_default();
    hubs.into_iter().filter_map(|(id, h)| h.recording().map(|r| (id, r))).collect()
}

/// Apaga todo (modulo desactivado): termina las grabaciones y cierra ffmpeg
pub fn stop_all() {
    let hubs: Vec<Arc<Hub>> = HUBS.lock().map(|h| h.values().cloned().collect()).unwrap_or_default();
    for h in hubs {
        if let Ok(mut r) = h.recording.lock() {
            r.take();
        }
        h.viewers.store(0, Ordering::SeqCst);
        if let Ok(mut l) = h.lease.lock() {
            *l = None;
        }
        h.changed.notify_one();
    }
}

// ═══════════════════════════════════════
// ffmpeg
// ═══════════════════════════════════════

#[cfg(test)]
pub static TEST_INPUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(not(test))]
fn alsa_args(dev: &str) -> Vec<String> {
    vec!["-f".into(), "alsa".into(), "-i".into(), dev.to_string()]
}

/// En pruebas con fuente sintetica el microfono tambien es sintetico
#[cfg(test)]
fn alsa_args(dev: &str) -> Vec<String> {
    if TEST_INPUT.load(Ordering::SeqCst) {
        return vec!["-f".into(), "lavfi".into(), "-i".into(), "sine=frequency=440:sample_rate=44100".into()];
    }
    vec!["-f".into(), "alsa".into(), "-i".into(), dev.to_string()]
}

fn input_args(spec: &CamSpec) -> Vec<String> {
    let s = |v: &str| v.to_string();
    #[cfg(test)]
    if TEST_INPUT.load(Ordering::SeqCst) {
        return vec![s("-f"), s("lavfi"), s("-i"), s("testsrc=size=320x240:rate=10")];
    }
    match &spec.input {
        Input::V4l2 { device, size } => {
            let mut a = vec![s("-f"), s("v4l2")];
            if let Some(sz) = size {
                a.extend([s("-video_size"), sz.clone()]);
            }
            a.extend([s("-i"), device.clone()]);
            a
        }
        Input::Url(url) => {
            let mut a = Vec::new();
            if url.starts_with("rtsp") {
                a.extend([s("-rtsp_transport"), s("tcp")]);
            }
            a.extend([s("-i"), url.clone()]);
            a
        }
    }
}

/// Argumentos de ffmpeg: vista MJPEG por stdout y, si se graba, MP4 fragmentado
/// (reproducible aunque el proceso se corte)
pub fn ffmpeg_args(spec: &CamSpec, rec: Option<&ActiveRecording>) -> Vec<String> {
    let s = |v: &str| v.to_string();
    let mut a = vec![s("-hide_banner"), s("-loglevel"), s("error")];
    a.extend(input_args(spec));
    let alsa = match (&spec.audio, rec) {
        (Audio::Alsa(dev), Some(_)) => {
            a.extend(alsa_args(dev));
            true
        }
        _ => false,
    };
    // Vista: 10 cuadros por segundo, maximo 1280 de ancho
    a.extend([
        s("-map"), s("0:v"), s("-an"),
        s("-vf"), s("fps=10,scale='min(1280,iw)':-2"),
        s("-c:v"), s("mjpeg"), s("-q:v"), s("7"),
        s("-f"), s("mpjpeg"), s("pipe:1"),
    ]);
    if let Some(r) = rec {
        a.extend([s("-map"), s("0:v")]);
        let audio = match spec.audio {
            Audio::Alsa(_) if alsa => {
                a.extend([s("-map"), s("1:a")]);
                true
            }
            Audio::Stream => {
                a.extend([s("-map"), s("0:a?")]);
                true
            }
            _ => false,
        };
        a.extend([s("-c:v"), s("libx264"), s("-preset"), s("veryfast"), s("-crf"), s("23"), s("-pix_fmt"), s("yuv420p")]);
        if audio {
            a.extend([s("-c:a"), s("aac"), s("-b:a"), s("128k")]);
        } else {
            a.push(s("-an"));
        }
        // Tope de seguridad por si el supervisor no alcanza a detenerla
        a.extend([s("-t"), (r.max.as_secs() + 30).to_string()]);
        a.extend([s("-movflags"), s("+frag_keyframe+empty_moov+default_base_moof"), s("-f"), s("mp4")]);
        a.push(r.path.to_string_lossy().to_string());
    }
    a
}

/// Lee los cuadros de la salida mpjpeg de ffmpeg (cabeceras con Content-length)
pub struct MjpegReader<R> {
    r: BufReader<R>,
}

impl<R: tokio::io::AsyncRead + Unpin> MjpegReader<R> {
    pub fn new(r: R) -> Self {
        Self { r: BufReader::new(r) }
    }

    pub async fn next_frame(&mut self) -> Option<Bytes> {
        let mut line = String::new();
        // Buscar el separador "--..."
        loop {
            line.clear();
            if self.r.read_line(&mut line).await.ok()? == 0 {
                return None;
            }
            if line.starts_with("--") {
                break;
            }
        }
        let mut len: Option<usize> = None;
        loop {
            line.clear();
            if self.r.read_line(&mut line).await.ok()? == 0 {
                return None;
            }
            let l = line.trim();
            if l.is_empty() {
                break;
            }
            if let Some((k, v)) = l.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    len = v.trim().parse().ok();
                }
            }
        }
        let len = len.filter(|n| *n > 0 && *n <= MAX_FRAME)?;
        let mut buf = vec![0u8; len];
        self.r.read_exact(&mut buf).await.ok()?;
        Some(Bytes::from(buf))
    }
}

// ═══════════════════════════════════════
// Supervisor
// ═══════════════════════════════════════

/// Ultimas lineas de error de ffmpeg, legibles
fn ffmpeg_error(stderr: &str) -> String {
    let last: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = last.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join(" | ");
    let low = stderr.to_lowercase();
    if low.contains("permission denied") {
        "Sin permiso para abrir la camara (el usuario del servicio debe estar en el grupo video)".into()
    } else if low.contains("device or resource busy") {
        "La camara esta en uso por otro programa".into()
    } else if low.contains("no such file") {
        "No se encontro la camara (¿esta conectada?)".into()
    } else if tail.is_empty() {
        "ffmpeg termino sin dar detalles".into()
    } else {
        tail
    }
}

async fn supervise(state: AppState, hub: Arc<Hub>, id: String) {
    loop {
        if !hub.wanted() {
            // Esperar un poco: alguien puede volver a mirar
            let woke = tokio::time::timeout(IDLE_GRACE, hub.changed.notified()).await.is_ok();
            if !woke && !hub.wanted() {
                break;
            }
            continue;
        }
        let spec = hub.spec.lock().map(|s| s.clone()).unwrap_or_else(|e| e.into_inner().clone());
        let rec = hub.recording();
        if let Some(r) = &rec {
            if let Some(dir) = r.path.parent() {
                let _ = tokio::fs::create_dir_all(dir).await;
            }
        }
        let mut cmd = Command::new("ffmpeg");
        cmd.args(ffmpeg_args(&spec, rec.as_ref()))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                if let Ok(mut er) = hub.error.lock() {
                    *er = Some(format!("No se pudo ejecutar ffmpeg: {}", e));
                }
                if let Some(r) = rec {
                    // No hay nada que grabar: se descarta la grabacion pedida
                    stop_recording(&hub);
                    super::finish_recording(&state, &r, false).await;
                }
                let _ = tokio::time::timeout(RETRY * 5, hub.changed.notified()).await;
                continue;
            }
        };
        let stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let mut stdin = child.stdin.take();
        let err_task = tokio::spawn(async move {
            let mut s = String::new();
            if let Some(e) = stderr.as_mut() {
                let _ = e.take(64 * 1024).read_to_string(&mut s).await;
            }
            s
        });

        let rec_id = rec.as_ref().map(|r| r.recording_id.clone());
        let deadline = rec.as_ref().map(|r| r.started + r.max);
        let mut got_frame = false;
        let mut exited_alone = false;
        if let Some(out) = stdout {
            let mut reader = MjpegReader::new(out);
            loop {
                let sleep_until = deadline.map(tokio::time::Instant::from_std);
                tokio::select! {
                    frame = reader.next_frame() => match frame {
                        Some(f) => {
                            if !got_frame {
                                got_frame = true;
                                if let Ok(mut er) = hub.error.lock() { *er = None; }
                            }
                            if let Ok(mut l) = hub.latest.lock() { *l = Some((Instant::now(), f.clone())); }
                            let _ = hub.frames.send(f);
                        }
                        None => { exited_alone = true; break; }
                    },
                    _ = hub.changed.notified() => {
                        if !hub.wanted() || hub.recording_id() != rec_id {
                            break;
                        }
                    },
                    _ = async { match sleep_until { Some(t) => tokio::time::sleep_until(t).await, None => std::future::pending().await } } => {
                        // Tiempo maximo de grabacion
                        stop_recording(&hub);
                        super::notify_auto_stop(&state, &spec.name);
                        break;
                    }
                }
            }
        }

        // Cierre ordenado: "q" hace que ffmpeg termine los archivos
        if let Some(mut i) = stdin.take() {
            let _ = i.write_all(b"q").await;
            let _ = i.flush().await;
        }
        if tokio::time::timeout(Duration::from_secs(6), child.wait()).await.is_err() {
            let _ = child.kill().await;
        }
        let stderr_text = err_task.await.unwrap_or_default();

        if let Some(r) = rec {
            // La grabacion de esta corrida termino (detenida, tope o falla)
            let still_same = hub.recording_id().as_deref() == Some(r.recording_id.as_str());
            if still_same && exited_alone {
                // ffmpeg murio solo mientras se grababa: no reintentar la misma grabacion
                stop_recording(&hub);
            }
            let ok = tokio::fs::metadata(&r.path).await.map(|m| m.len() > 0).unwrap_or(false);
            super::finish_recording(&state, &r, ok).await;
        }

        if exited_alone {
            if let Ok(mut er) = hub.error.lock() {
                *er = Some(ffmpeg_error(&stderr_text));
            }
            if !got_frame {
                if let Ok(mut l) = hub.latest.lock() {
                    *l = None;
                }
            }
            let _ = tokio::time::timeout(RETRY, hub.changed.notified()).await;
        }
    }
    if let Ok(mut hubs) = HUBS.lock() {
        // Solo si nadie lo reemplazo mientras tanto
        if hubs.get(&id).is_some_and(|h| Arc::ptr_eq(h, &hub)) {
            hubs.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(input: Input, audio: Audio) -> CamSpec {
        CamSpec { id: "c".into(), name: "C".into(), input, audio }
    }

    #[tokio::test]
    async fn lee_cuadros_mpjpeg() {
        let data = b"--ffmpeg\r\nContent-type: image/jpeg\r\nContent-length: 3\r\n\r\nABC\r\n--ffmpeg\r\nContent-type: image/jpeg\r\nContent-length: 2\r\n\r\nXY\r\n".to_vec();
        let mut r = MjpegReader::new(std::io::Cursor::new(data));
        assert_eq!(r.next_frame().await.as_deref(), Some(&b"ABC"[..]));
        assert_eq!(r.next_frame().await.as_deref(), Some(&b"XY"[..]));
        assert_eq!(r.next_frame().await, None);
    }

    #[test]
    fn argumentos() {
        let usb = spec(Input::V4l2 { device: "/dev/video0".into(), size: Some("1280x720".into()) }, Audio::Alsa("plughw:CARD=C,DEV=0".into()));
        let a = ffmpeg_args(&usb, None).join(" ");
        assert!(a.contains("-f v4l2 -video_size 1280x720 -i /dev/video0"));
        assert!(!a.contains("alsa"), "sin grabar no se abre el microfono");
        assert!(a.ends_with("-f mpjpeg pipe:1"));

        let rec = ActiveRecording {
            recording_id: "r".into(),
            camera: "C".into(),
            path: PathBuf::from("/x/a.mp4"),
            username: "u".into(),
            started: Instant::now(),
            started_at: String::new(),
            max: Duration::from_secs(60),
        };
        let a = ffmpeg_args(&usb, Some(&rec)).join(" ");
        assert!(a.contains("-f alsa -i plughw:CARD=C,DEV=0"));
        assert!(a.contains("-map 0:v -map 1:a -c:v libx264"));
        assert!(a.contains("-c:a aac") && a.contains("-t 90") && a.ends_with("-f mp4 /x/a.mp4"));

        let ip = spec(Input::Url("rtsp://cam/stream".into()), Audio::Stream);
        let a = ffmpeg_args(&ip, Some(&rec)).join(" ");
        assert!(a.contains("-rtsp_transport tcp -i rtsp://cam/stream"));
        assert!(a.contains("-map 0:a?"));
        let mudo = spec(Input::Url("http://cam/mjpg".into()), Audio::None);
        assert!(ffmpeg_args(&mudo, Some(&rec)).join(" ").contains("-pix_fmt yuv420p -an -t"));
    }

    #[test]
    fn errores_legibles() {
        assert!(ffmpeg_error("[video4linux2] /dev/video0: Permission denied").contains("grupo video"));
        assert!(ffmpeg_error("x: Device or resource busy").contains("en uso"));
        assert_eq!(ffmpeg_error(""), "ffmpeg termino sin dar detalles");
    }
}
