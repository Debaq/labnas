//! Controles multimedia del sistema y avisos de cancion nueva.
//!
//! - Linux: MPRIS por D-Bus (widget de medios de KDE/GNOME, pantalla de bloqueo, teclas
//!   ⏯ ⏭ ⏮ del teclado).
//! - Windows: SMTC (el panel de medios junto al volumen y las teclas multimedia); necesita
//!   una ventana propia, oculta.
//!
//! La musica suena en LabNAS: esto solo muestra que suena y manda las ordenes.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection};

/// Lo que pide el sistema (teclas, panel de medios)
pub enum Action {
    /// Play / Pause explicitos (el estado lo decide quien recibe)
    SetPaused(bool),
    Toggle,
    Next,
    Previous,
    SeekTo(u32),
    SeekBy(i64),
    Volume(u8),
    /// Clic en el reproductor del sistema: abrir el widget
    Raise,
}

/// Paso de las teclas de adelantar / retroceder sin cantidad
const SEEK_STEP: i64 = 10;

pub struct Media {
    controls: MediaControls,
    #[cfg(windows)]
    _window: tao::window::Window,
    /// Ultimo estado mostrado: (id de la cancion, en pausa, ancla del tiempo)
    shown: Option<(String, bool, u64)>,
    volume: Option<u64>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn to_action(e: MediaControlEvent) -> Option<Action> {
    let secs = |d: Duration| i64::try_from(d.as_secs()).unwrap_or(i64::MAX);
    Some(match e {
        MediaControlEvent::Play => Action::SetPaused(false),
        MediaControlEvent::Pause | MediaControlEvent::Stop => Action::SetPaused(true),
        MediaControlEvent::Toggle => Action::Toggle,
        MediaControlEvent::Next => Action::Next,
        MediaControlEvent::Previous => Action::Previous,
        MediaControlEvent::Seek(SeekDirection::Forward) => Action::SeekBy(SEEK_STEP),
        MediaControlEvent::Seek(SeekDirection::Backward) => Action::SeekBy(-SEEK_STEP),
        MediaControlEvent::SeekBy(SeekDirection::Forward, d) => Action::SeekBy(secs(d)),
        MediaControlEvent::SeekBy(SeekDirection::Backward, d) => Action::SeekBy(-secs(d)),
        MediaControlEvent::SetPosition(MediaPosition(d)) => Action::SeekTo(u32::try_from(d.as_secs()).unwrap_or(u32::MAX)),
        MediaControlEvent::SetVolume(v) => Action::Volume((v * 100.0).round().clamp(0.0, 100.0) as u8),
        MediaControlEvent::Raise => Action::Raise,
        _ => return None,
    })
}

impl Media {
    pub fn new<T: 'static>(target: &tao::event_loop::EventLoopWindowTarget<T>, on: impl Fn(Action) + Send + 'static) -> Option<Media> {
        #[cfg(not(windows))]
        let _ = target;
        #[cfg(windows)]
        let window = tao::window::WindowBuilder::new()
            .with_title("LabNAS")
            .with_visible(false)
            .build(target)
            .map_err(|e| log!("controles multimedia: sin ventana: {e}"))
            .ok()?;
        #[cfg(windows)]
        let hwnd = {
            use tao::platform::windows::WindowExtWindows;
            Some(window.hwnd() as *mut std::ffi::c_void)
        };
        #[cfg(not(windows))]
        let hwnd = None;
        // un nombre por proceso: la copia de pruebas convive con la instalada
        let dbus_name = format!("labnas_tray.instance{}", std::process::id());
        let config = PlatformConfig { display_name: "LabNAS", dbus_name: &dbus_name, hwnd };
        let mut controls = MediaControls::new(config).map_err(|e| log!("controles multimedia: {e:?}")).ok()?;
        controls
            .attach(move |e| {
                if let Some(a) = to_action(e) {
                    on(a);
                }
            })
            .map_err(|e| log!("controles multimedia: {e:?}"))
            .ok()?;
        Some(Media {
            controls,
            #[cfg(windows)]
            _window: window,
            shown: None,
            volume: None,
        })
    }

    /// Refleja el MusicState de LabNAS (None = sin conexion o sin vincular)
    pub fn update(&mut self, music: Option<&Value>) {
        let cur = music.and_then(|m| m.get("current")).filter(|c| c.is_object());
        let (Some(m), Some(c)) = (music, cur) else {
            if self.shown.take().is_some() {
                let _ = self.controls.set_metadata(MediaMetadata::default());
                let _ = self.controls.set_playback(MediaPlayback::Stopped);
            }
            return;
        };
        let text = |k: &str| c.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
        let id = text("id").unwrap_or_default().to_string();
        let paused = m["paused"].as_bool().unwrap_or(false);
        let elapsed = m["elapsed"].as_u64().unwrap_or(0);
        // en pausa el tiempo queda quieto; sonando, se compara el momento en que empezo
        let anchor = if paused { elapsed } else { now_secs().saturating_sub(elapsed) };

        let new_track = self.shown.as_ref().is_none_or(|s| s.0 != id);
        if new_track {
            let duration = c["duration"].as_u64().filter(|d| *d > 0).map(Duration::from_secs);
            let _ = self.controls.set_metadata(MediaMetadata {
                title: text("title"),
                artist: text("artist"),
                album: Some("LabNAS"),
                cover_url: text("thumbnail"),
                duration,
            });
        }
        let moved = self.shown.as_ref().is_none_or(|s| s.1 != paused || s.2.abs_diff(anchor) > 2);
        if new_track || moved {
            let progress = Some(MediaPosition(Duration::from_secs(elapsed)));
            let _ = self
                .controls
                .set_playback(if paused { MediaPlayback::Paused { progress } } else { MediaPlayback::Playing { progress } });
            self.shown = Some((id, paused, anchor));
        }

        let volume = m["volume"].as_u64();
        if volume != self.volume {
            self.volume = volume;
            #[cfg(target_os = "linux")]
            if let Some(v) = volume {
                let _ = self.controls.set_volume(v as f64 / 100.0);
            }
        }
    }
}

/// Identidad de los avisos en Windows (registrada en HKCU, sin instalador)
#[cfg(windows)]
const AUMID: &str = "LabNAS.Bandeja";

/// Windows solo muestra avisos de una aplicacion con identidad registrada
#[cfg(windows)]
pub fn register_notifications() {
    let key = format!(r"HKCU\Software\Classes\AppUserModelId\{AUMID}");
    let _ = crate::autostart::reg(&["add", &key, "/v", "DisplayName", "/t", "REG_SZ", "/d", "LabNAS", "/f"]);
    if let Some(icon) = icon_file() {
        let _ = crate::autostart::reg(&["add", &key, "/v", "IconUri", "/t", "REG_SZ", "/d", &icon.to_string_lossy(), "/f"]);
    }
}

/// El rayo de LabNAS como archivo (los avisos piden una ruta, no bytes)
fn icon_file() -> Option<std::path::PathBuf> {
    let dir = dirs::data_local_dir()?.join("labnas-tray");
    let icon = dir.join("icono.png");
    let png = include_bytes!("../assets/tray.png");
    if std::fs::metadata(&icon).map(|m| m.len()).ok() != Some(png.len() as u64) {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&icon, png).ok()?;
    }
    Some(icon)
}

/// Que dibujo lleva el aviso
#[derive(Clone, Copy)]
pub enum Kind {
    Music,
    LabNas,
}

/// Aviso del sistema (en otro hilo: no frena la consulta)
pub fn notify(kind: Kind, title: String, body: String) {
    std::thread::spawn(move || {
        let mut n = notify_rust::Notification::new();
        n.summary(&title).body(&body).appname("LabNAS").timeout(notify_rust::Timeout::Milliseconds(6000));
        #[cfg(windows)]
        n.app_id(AUMID);
        #[cfg(target_os = "linux")]
        match (kind, icon_file()) {
            (Kind::LabNas, Some(icon)) => n.icon(&icon.to_string_lossy()),
            _ => n.icon("audio-x-generic"),
        };
        #[cfg(not(target_os = "linux"))]
        let _ = kind;
        if let Err(e) = n.show() {
            log!("aviso: {e}");
        }
    });
}
