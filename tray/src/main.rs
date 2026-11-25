//! labnas-tray: la musica de LabNAS desde la bandeja del sistema, para los computadores
//! de quienes usan el laboratorio (Windows y Linux).
//!
//! - Clic en el icono (Windows) o «Abrir control» en su menu: un widget flotante con lo
//!   que suena, anterior/pausa/siguiente, volumen, la cola, buscar y agregar canciones y
//!   las listas guardadas. Es una pagina embebida (`assets/widget.html`) en un webview
//!   (WebView2 en Windows, WebKitGTK en Linux) que habla con este programa por IPC: el
//!   token nunca llega a la pagina.
//! - Asistente: encuentra el LabNAS (direccion que trae la descarga, configuracion, mDNS,
//!   labnas.local) y si no, la pide; despues vincula la cuenta desde el navegador con un
//!   token de dispositivo que solo sirve para la musica (revocable en LabNAS, Configuracion
//!   > General > Dispositivos vinculados).
//!
//! Hilos: el principal maneja la bandeja y el widget (tao); uno aparte hace la red y le
//! manda el estado por el `EventLoopProxy`; lo lento (buscar, reproducir) va en hilos propios.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod autostart;
mod config;
mod http;
mod link;
#[macro_use]
mod logfile;
mod media;
mod notices;

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
use wry::{WebContext, WebView, WebViewBuilder};

use http::Url;

const MDNS_TYPE: &str = "_labnas._tcp.local.";
const MDNS_TIMEOUT: Duration = Duration::from_millis(2500);
/// Ultimo intento si no hay mDNS (Windows 10+ y la mayoria de los Linux resuelven .local)
const FALLBACK_URL: &str = "http://labnas.local:3001";
/// Cada cuanto se consulta lo que suena
const POLL: Duration = Duration::from_secs(3);
/// Si el servidor no responde, cada cuanto se lo vuelve a buscar (cambio de IP)
const REDISCOVER_EVERY: Duration = Duration::from_secs(60);
/// Plazo para pedidos que pasan por yt-dlp en el servidor
const SLOW: Duration = Duration::from_secs(90);
const VOLUME_STEP: i64 = 10;
/// Cada cuanto se relee la piel (por si se cambio en LabNAS)
const LOOK_EVERY: Duration = Duration::from_secs(60);
/// Puerto local para que una segunda copia le pida a la primera abrir el widget
const SINGLE_INSTANCE_PORT: u16 = 47321;
const WIDGET_W: f64 = 372.0;
/// Alto inicial (reproductor sin cola); la pagina pide el alto que necesita con `size`
const WIDGET_H: f64 = 300.0;
const WIDGET_H_RANGE: (f64, f64) = (160.0, 640.0);

/// Lo que muestran la bandeja y el widget
#[derive(Debug, Clone, Default)]
struct Status {
    /// buscando | configurar | vincular | vinculando | listo | sin-conexion | certificado
    phase: &'static str,
    /// Primera linea del menu y del tooltip
    headline: String,
    /// MusicState tal cual lo manda LabNAS
    music: Option<Value>,
    playing: bool,
    paused: bool,
    linked: bool,
    user: Option<String>,
    base: Option<String>,
    /// Direccion para proponer en el asistente
    suggest: Option<String>,
    /// Error para el asistente (direccion que no responde, vinculacion fallida...)
    error: Option<String>,
    /// Aviso en lugar del titulo (musica desactivada, cuenta pendiente...)
    notice: Option<String>,
    /// Certificado cambiado: (host, huella guardada, actual)
    cert_changed: Option<(String, String, String)>,
    /// Piel elegida en LabNAS (cuenta) y color de acento del laboratorio (branding)
    theme: Option<String>,
    accent: Option<String>,
    /// Aviso del sistema al cambiar la cancion (opcional)
    notify: bool,
    /// Avisos de LabNAS que se muestran: (personales, laboratorio, administracion)
    notices: (bool, bool, bool),
    /// La cuenta es de un admin (para ofrecer los avisos de administracion)
    admin: bool,
}

impl Status {
    fn to_json(&self) -> Value {
        json!({
            "phase": self.phase,
            "music": self.music,
            "user": self.user,
            "base": self.base,
            "suggest": self.suggest,
            "error": self.error,
            "notice": self.notice,
            "cert": self.cert_changed.as_ref().map(|(h, o, n)| json!({ "host": h, "old": o, "new": n })),
            "theme": self.theme,
            "accent": self.accent,
            "notify": self.notify,
            "notices": { "personal": self.notices.0, "lab": self.notices.1, "admin": self.notices.2 },
            "admin": self.admin,
        })
    }
}

enum UserEvent {
    Menu(MenuId),
    /// Clic izquierdo en el icono, con su posicion en pantalla (Windows)
    TrayClick(Option<tray_icon::Rect>),
    Status(Box<Status>),
    /// Evento para la pagina del widget (resultados de busqueda, listas, avisos)
    Widget(Value),
    /// Mensaje de la pagina del widget
    Ipc(String),
    ShowWidget,
}

enum Cmd {
    Toggle,
    Next,
    Previous,
    Volume(i64),
    SetVolume(u8),
    Shuffle,
    Repeat,
    Search(String),
    Play { id: String, now: bool },
    QueuePlay(usize),
    QueueRemove(usize),
    QueueMove(usize, usize),
    Playlists,
    LoadPlaylist(String),
    /// Mix: llena la cola con recomendaciones (como el boton del reproductor web)
    Mix,
    /// Saltar a un segundo de la cancion
    Seek(u32),
    /// Adelantar / retroceder (teclas del sistema)
    SeekBy(i64),
    /// Play / Pause del sistema (solo cambia si hace falta)
    SetPaused(bool),
    /// Guardar la cancion actual en una lista
    SaveTo { id: String, name: String },
    SetNotify(bool),
    /// Avisos de LabNAS: `personal`, `lab` o `admin`
    SetNotices(String, bool),
    Link,
    Linked(Result<link::Linked, String>),
    Unlink,
    Rediscover,
    ChangeUrl,
    SetUrl(String),
    TrustNewCert,
    /// Respuesta de un pedido lento hecho en otro hilo (MusicState o error)
    Applied(Result<http::Response, String>),
    Refresh,
}

// ─── Red ───

fn discover() -> Option<String> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    let mdns = ServiceDaemon::new().ok()?;
    let rx = mdns.browse(MDNS_TYPE).ok()?;
    let deadline = Instant::now() + MDNS_TIMEOUT;
    let mut found = None;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                if let Some(ip) = info.get_addresses_v4().into_iter().next() {
                    found = Some(format!("http://{}:{}", ip, info.get_port()));
                    break;
                }
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    let _ = mdns.shutdown();
    found
}

/// Sigue la redireccion a HTTPS (si el servidor la tiene) y comprueba que es un LabNAS
fn resolve(url: &Url) -> Result<Url, http::Error> {
    let health = |u: &Url| -> Result<http::Response, http::Error> { http::request(u, "GET", "/api/health", None, None) };
    let not_labnas = |u: &Url, status: u16| http::Error::Proto(format!("{} no parece un LabNAS (HTTP {status})", u.base()));
    let r = health(url)?;
    if (300..400).contains(&r.status)
        && let Some(loc) = r.location.as_deref().and_then(Url::parse)
    {
        let r2 = health(&loc)?;
        return if r2.status == 200 { Ok(loc) } else { Err(not_labnas(&loc, r2.status)) };
    }
    if r.status == 200 { Ok(url.clone()) } else { Err(not_labnas(url, r.status)) }
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

/// Lo que escribio la persona: agrega `http://` si falta
fn normalize_url(raw: &str) -> Option<Url> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.contains("://") { Url::parse(raw) } else { Url::parse(&format!("http://{raw}")) }
}

fn error_text(r: &http::Response, fallback: &str) -> String {
    let t = r.text();
    shorten(if t.is_empty() || t.starts_with('<') { fallback } else { &t }, 120)
}

struct Worker {
    cfg: config::Config,
    base: Option<Url>,
    status: Status,
    proxy: EventLoopProxy<UserEvent>,
    tx: Sender<Cmd>,
    last_discovery: Option<Instant>,
    searching: bool,
    offline: bool,
    linking: bool,
    /// La persona pidio cambiar la direccion
    force_setup: bool,
    /// Ultima vez que se leyo la piel y el acento
    last_look: Option<Instant>,
    /// Cancion de la ultima consulta (None = aun no se consulto; "" = nada sonando)
    last_track: Option<String>,
    /// Lo que necesita el hilo de los avisos de LabNAS
    notices: notices::Shared,
}

impl Worker {
    fn phase(&self) -> &'static str {
        if self.status.cert_changed.is_some() {
            "certificado"
        } else if self.searching {
            "buscando"
        } else if self.base.is_none() || self.force_setup {
            "configurar"
        } else if self.linking {
            "vinculando"
        } else if self.cfg.token.is_none() {
            "vincular"
        } else if self.offline {
            "sin-conexion"
        } else {
            "listo"
        }
    }

    fn publish(&mut self) {
        self.status.phase = self.phase();
        self.status.linked = self.cfg.token.is_some();
        self.status.user = self.cfg.user.clone();
        self.status.notify = self.cfg.notify;
        self.status.notices = (!self.cfg.quiet_personal, self.cfg.lab, !self.cfg.quiet_admin);
        if let Ok(mut l) = self.notices.lock() {
            let ready = self.status.cert_changed.is_none() && !self.force_setup;
            l.base = self.base.clone().filter(|_| ready);
            l.token = self.cfg.token.clone();
            (l.personal, l.lab, l.admin) = self.status.notices;
        }
        self.status.base = self.base.as_ref().map(Url::base);
        if self.status.phase != "listo" {
            self.status.headline = match self.status.phase {
                "buscando" => "Buscando LabNAS…".into(),
                "configurar" => "Falta la dirección de LabNAS".into(),
                "vincular" => "Sin vincular a tu cuenta".into(),
                "vinculando" => "Autoriza en el navegador…".into(),
                "sin-conexion" => "LabNAS no responde".into(),
                _ => "⚠ El certificado de LabNAS cambió".into(),
            };
        }
        let _ = self.proxy.send_event(UserEvent::Status(Box::new(self.status.clone())));
    }

    fn widget(&self, ev: Value) {
        let _ = self.proxy.send_event(UserEvent::Widget(ev));
    }

    fn toast(&self, text: impl Into<String>, bad: bool) {
        self.widget(json!({ "type": "toast", "text": text.into(), "bad": bad }));
    }

    /// Prueba una direccion; true si queda resuelta (o con el certificado cambiado)
    fn try_url(&mut self, url: &Url) -> Result<(), String> {
        match resolve(url) {
            Ok(base) => {
                self.status.cert_changed = None;
                self.offline = false;
                if self.cfg.url.as_deref() != Some(base.base().as_str()) {
                    self.cfg.url = Some(base.base());
                    config::save(&self.cfg);
                }
                self.base = Some(base);
                Ok(())
            }
            Err(http::Error::CertChanged(h, old, new)) => {
                self.base = Some(url.clone());
                self.status.cert_changed = Some((h, old, new));
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Busca el servidor: variable de entorno, configuracion, direccion de la descarga y,
    /// con `broad`, mDNS y labnas.local. Vinculado no se busca en la red: un LabNAS
    /// cualquiera que conteste no debe recibir el token de este computador.
    fn find_server(&mut self, broad: bool) {
        self.last_discovery = Some(Instant::now());
        let mut known: Vec<String> = Vec::new();
        if let Ok(env) = std::env::var("LABNAS_URL") {
            known.push(env);
        }
        known.extend(self.cfg.url.clone());
        known.extend(config::bundled_url());
        for c in known {
            let Some(url) = Url::parse(&c) else { continue };
            match self.try_url(&url) {
                Ok(()) => return,
                Err(e) => log!("{c}: {e}"),
            }
        }
        if !broad {
            return;
        }
        for c in discover().into_iter().chain(std::iter::once(FALLBACK_URL.to_string())) {
            let Some(url) = Url::parse(&c) else { continue };
            if self.try_url(&url).is_ok() {
                return;
            }
        }
    }

    fn token(&self) -> Option<String> {
        self.cfg.token.clone()
    }

    /// Pedido autenticado a la API; None si el servidor no contesto
    fn call(&mut self, method: &str, path: &str, body: Option<&str>) -> Option<http::Response> {
        let (Some(base), Some(token)) = (self.base.clone(), self.token()) else {
            return None;
        };
        match http::request(&base, method, path, Some(&token), body) {
            Ok(r) => {
                self.offline = false;
                Some(r)
            }
            Err(http::Error::CertChanged(h, old, new)) => {
                self.status.cert_changed = Some((h, old, new));
                None
            }
            Err(e) => {
                if !self.offline {
                    log!("{path}: {e}");
                }
                // se reintenta la misma direccion en la proxima consulta
                self.offline = true;
                None
            }
        }
    }

    /// Pedido lento en otro hilo; el MusicState vuelve como `Cmd::Applied`
    fn slow(&self, method: &'static str, path: String, body: Option<String>) {
        let (Some(base), Some(token)) = (self.base.clone(), self.token()) else { return };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = http::request_timeout(&base, method, &path, Some(&token), body.as_deref(), SLOW).map_err(|e| e.to_string());
            match &res {
                Ok(r) if r.status == 200 => log!("{method} {path}: 200"),
                Ok(r) => log!("{method} {path}: {} {}", r.status, shorten(&r.text(), 160)),
                Err(e) => log!("{method} {path}: {e}"),
            }
            let _ = tx.send(Cmd::Applied(res));
        });
    }

    /// Interpreta la respuesta a un pedido de musica y actualiza el estado
    fn apply(&mut self, r: http::Response) {
        match r.status {
            200 => {
                let Some(ms) = r.json() else { return };
                if !ms.is_object() || ms.get("queue").is_none() {
                    return; // no es un MusicState (p. ej. cargar una lista): se refresca aparte
                }
                self.status.notice = None;
                self.status.paused = ms["paused"].as_bool().unwrap_or(false);
                match ms["current"].as_object() {
                    Some(cur) => {
                        let title = cur.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        let artist = cur.get("artist").and_then(|v| v.as_str()).unwrap_or("");
                        let icon = if self.status.paused { "⏸" } else { "♪" };
                        let text = if artist.is_empty() { title.to_string() } else { format!("{title} — {artist}") };
                        self.status.headline = format!("{icon} {}", shorten(&text, 60));
                        self.status.playing = true;
                        let id = cur.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        // la primera consulta no avisa: solo los cambios
                        if self.last_track.as_deref().is_some_and(|t| t != id) && self.cfg.notify {
                            let by = cur.get("added_by").and_then(|v| v.as_str()).or(ms["started_by"].as_str());
                            let body = match by {
                                Some(by) if artist.is_empty() => format!("Puesta por {by}"),
                                Some(by) => format!("{artist} · puesta por {by}"),
                                None => artist.to_string(),
                            };
                            media::notify(media::Kind::Music, title.to_string(), body);
                        }
                        self.last_track = Some(id.to_string());
                    }
                    None => {
                        self.status.headline = "Nada sonando".into();
                        self.status.playing = false;
                        self.last_track = Some(String::new());
                    }
                }
                self.status.music = Some(ms);
            }
            401 => {
                // revocado desde LabNAS (o la cuenta se borro)
                self.cfg.token = None;
                self.cfg.user = None;
                config::save(&self.cfg);
                self.status.music = None;
                self.status.error = Some("La vinculación fue revocada en LabNAS: vuelve a vincular.".into());
            }
            404 => {
                self.status.notice = Some("La música está desactivada en LabNAS".into());
                self.status.headline = self.status.notice.clone().unwrap_or_default();
            }
            _ => self.toast(error_text(&r, "LabNAS no pudo hacerlo"), true),
        }
    }

    /// Piel de la cuenta y acento del laboratorio, para que el widget se vea como LabNAS
    fn refresh_look(&mut self) {
        if self.base.is_none() || self.last_look.is_some_and(|t| t.elapsed() < LOOK_EVERY) {
            return;
        }
        self.last_look = Some(Instant::now());
        if let Some(base) = self.base.clone()
            && let Ok(r) = http::request(&base, "GET", "/api/system/branding", None, None)
            && r.status == 200
        {
            self.status.accent = r.json().and_then(|b| b["accent_color"].as_str().map(str::to_string)).filter(|a| !a.is_empty());
        }
        if self.token().is_some()
            && let Some(r) = self.call("GET", "/api/auth/preferences", None)
            && r.status == 200
        {
            self.status.theme = r.json().and_then(|p| p["theme"].as_str().map(str::to_string));
        }
        if self.token().is_some()
            && let Some(r) = self.call("GET", "/api/auth/device-tokens/me", None)
            && r.status == 200
        {
            self.status.admin = r.json().is_some_and(|m| m["admin"] == true);
        }
    }

    fn refresh(&mut self) {
        if self.token().is_none() || self.status.cert_changed.is_some() || self.force_setup {
            return;
        }
        if self.base.is_none() {
            if self.last_discovery.is_none_or(|t| t.elapsed() > REDISCOVER_EVERY) {
                self.find_server(false);
            }
            if self.base.is_none() {
                return;
            }
        }
        if let Some(r) = self.call("GET", "/api/music/current", None) {
            self.apply(r);
        }
        self.refresh_look();
    }

    fn action(&mut self, method: &str, path: &str, body: Option<String>) {
        if let Some(r) = self.call(method, path, Some(body.as_deref().unwrap_or("{}"))) {
            self.apply(r);
        }
    }

    fn send_playlists(&mut self) {
        if let Some(r) = self.call("GET", "/api/music/playlists", None) {
            let items = if r.status == 200 { r.json().unwrap_or(json!([])) } else { json!([]) };
            self.widget(json!({ "type": "playlists", "items": items }));
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Toggle => self.action("POST", "/api/music/pause", None),
            Cmd::Next => self.action("POST", "/api/music/next", None),
            Cmd::Previous => {
                if let Some(r) = self.call("POST", "/api/music/previous", Some("{}")) {
                    if r.status == 200 { self.apply(r) } else { self.toast("No hay canción anterior", false) }
                }
            }
            Cmd::Volume(delta) => {
                let cur = self.status.music.as_ref().and_then(|m| m["volume"].as_i64()).unwrap_or(50);
                let v = (cur + delta).clamp(0, 100);
                self.action("POST", "/api/music/volume", Some(format!("{{\"volume\":{v}}}")));
            }
            Cmd::SetVolume(v) => self.action("POST", "/api/music/volume", Some(format!("{{\"volume\":{}}}", v.min(100)))),
            Cmd::Seek(pos) => self.action("POST", "/api/music/seek", Some(format!("{{\"position\":{pos}}}"))),
            Cmd::SeekBy(delta) => {
                // el tiempo de ahora, no el de la consulta de hace unos segundos
                self.refresh();
                let Some(ms) = self.status.music.as_ref().filter(|m| m["current"].is_object()) else { return };
                let pos = (ms["elapsed"].as_i64().unwrap_or(0) + delta).max(0);
                self.action("POST", "/api/music/seek", Some(format!("{{\"position\":{pos}}}")));
            }
            Cmd::SetPaused(want) => {
                if self.status.playing && self.status.paused != want {
                    self.action("POST", "/api/music/pause", None);
                }
            }
            Cmd::SaveTo { id, name } => {
                let Some(cur) = self.status.music.as_ref().map(|m| m["current"].clone()).filter(Value::is_object) else {
                    self.toast("No hay nada sonando", true);
                    return;
                };
                let body = json!({
                    "id": cur["id"], "title": cur["title"], "artist": cur["artist"],
                    "thumbnail": cur["thumbnail"], "duration": cur["duration"],
                })
                .to_string();
                match self.call("POST", &format!("/api/music/playlists/{id}/tracks"), Some(&body)) {
                    Some(r) if r.status == 200 => {
                        let title = cur["title"].as_str().unwrap_or("La canción");
                        self.toast(format!("«{}» guardada en {name}", shorten(title, 40)), false);
                        self.send_playlists();
                    }
                    Some(r) => self.toast(error_text(&r, "No se pudo guardar"), true),
                    None => {}
                }
                return;
            }
            Cmd::SetNotify(on) => {
                self.cfg.notify = on;
                config::save(&self.cfg);
                #[cfg(windows)]
                if on {
                    media::register_notifications();
                }
            }
            Cmd::SetNotices(key, on) => {
                match key.as_str() {
                    "personal" => self.cfg.quiet_personal = !on,
                    "lab" => self.cfg.lab = on,
                    _ => self.cfg.quiet_admin = !on,
                }
                config::save(&self.cfg);
                #[cfg(windows)]
                if on {
                    media::register_notifications();
                }
            }
            Cmd::Shuffle => self.action("POST", "/api/music/shuffle", None),
            Cmd::Repeat => self.action("POST", "/api/music/repeat", None),
            Cmd::QueuePlay(i) => self.action("POST", &format!("/api/music/queue/play/{i}"), None),
            Cmd::QueueRemove(i) => self.action("DELETE", "/api/music/queue", Some(format!("{{\"index\":{i}}}"))),
            Cmd::QueueMove(from, to) => self.action("POST", "/api/music/queue/move", Some(format!("{{\"from\":{from},\"to\":{to}}}"))),
            Cmd::Search(q) => {
                let (Some(base), Some(token)) = (self.base.clone(), self.token()) else { return };
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let path = format!("/api/music/search?q={}", http::pct_encode(&q));
                    let res = http::request_timeout(&base, "GET", &path, Some(&token), None, SLOW);
                    match &res {
                        Ok(r) => log!("buscar {q:?}: {}", r.status),
                        Err(e) => log!("buscar {q:?}: {e}"),
                    }
                    let ev = match res {
                        Ok(r) if r.status == 200 => json!({ "type": "search", "q": q, "results": r.json().unwrap_or(json!([])) }),
                        Ok(r) => json!({ "type": "search", "q": q, "error": error_text(&r, "La búsqueda falló") }),
                        Err(e) => json!({ "type": "search", "q": q, "error": format!("La búsqueda falló: {e}") }),
                    };
                    let _ = proxy.send_event(UserEvent::Widget(ev));
                });
                return;
            }
            Cmd::Play { id, now } => {
                let body = json!({ "id": id, "force": now }).to_string();
                self.slow("POST", "/api/music/play".into(), Some(body));
                return;
            }
            Cmd::Playlists => {
                self.send_playlists();
                return;
            }
            Cmd::Mix => {
                self.slow("POST", "/api/music/recommend".into(), Some("{}".into()));
                return;
            }
            Cmd::LoadPlaylist(id) => {
                let id = playlist_id(&id);
                self.slow("POST", format!("/api/music/playlists/{id}/load"), Some("{}".into()));
                return;
            }
            Cmd::Applied(res) => match res {
                Ok(r) if r.status == 200 => {
                    let was_state = r.json().is_some_and(|v| v.get("queue").is_some());
                    self.apply(r);
                    if !was_state {
                        self.toast("Lista agregada a la cola", false);
                        self.refresh();
                    }
                }
                Ok(r) => self.toast(error_text(&r, "LabNAS no pudo hacerlo"), true),
                Err(e) => self.toast(format!("Sin respuesta de LabNAS: {}", shorten(&e, 80)), true),
            },
            Cmd::Link => {
                let Some(base) = self.base.as_ref().map(Url::base) else { return };
                self.linking = true;
                self.status.error = None;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Cmd::Linked(link::link(&base)));
                });
            }
            Cmd::Linked(res) => {
                self.linking = false;
                match res {
                    Ok(l) => {
                        self.cfg.token = Some(l.token);
                        self.cfg.user = l.user;
                        self.status.error = None;
                        self.last_look = None;
                        config::save(&self.cfg);
                        let _ = self.proxy.send_event(UserEvent::ShowWidget);
                    }
                    Err(e) => {
                        log!("vincular: {e}");
                        self.status.error = Some(format!("No se vinculó: {e}"));
                    }
                }
            }
            Cmd::Unlink => {
                // avisar a LabNAS para que el token deje de valer; si no responde, igual se olvida
                let _ = self.call("DELETE", "/api/auth/device-tokens/me", None);
                self.cfg.token = None;
                self.cfg.user = None;
                self.status.music = None;
                config::save(&self.cfg);
            }
            Cmd::Rediscover => {
                self.status.cert_changed = None;
                self.status.error = None;
                self.force_setup = false;
                self.searching = true;
                self.publish();
                let before = (self.base.take(), self.cfg.url.take());
                self.find_server(true);
                self.searching = false;
                let changed = self.base.as_ref().map(Url::base) != before.0.as_ref().map(Url::base);
                if self.base.is_none() {
                    (self.base, self.cfg.url) = before;
                    config::save(&self.cfg);
                    self.status.error = Some("No lo encontré en la red. Escribe la dirección.".into());
                } else if changed && self.token().is_some() {
                    // otro LabNAS: solo se adopta si reconoce este computador
                    let ok = self.call("GET", "/api/auth/device-tokens/me", None).is_some_and(|r| r.status == 200);
                    if !ok {
                        let found = self.base.as_ref().map(Url::base).unwrap_or_default();
                        (self.base, self.cfg.url) = before;
                        config::save(&self.cfg);
                        self.offline = false;
                        self.status.error = Some(format!("Encontré {found}, pero no reconoce este computador. Usa «Usar otra dirección» para cambiarte a él."));
                    }
                }
            }
            Cmd::ChangeUrl => {
                self.force_setup = true;
                self.status.error = None;
            }
            Cmd::SetUrl(raw) => {
                self.status.error = None;
                match normalize_url(&raw) {
                    None => self.status.error = Some("Esa dirección no es válida".into()),
                    Some(url) => {
                        self.searching = true;
                        self.publish();
                        let res = self.try_url(&url);
                        self.searching = false;
                        match res {
                            Ok(()) => self.force_setup = false,
                            Err(e) => {
                                self.status.suggest = Some(raw);
                                self.status.error = Some(format!("No pude conectar: {}", shorten(&e, 120)));
                            }
                        }
                    }
                }
            }
            Cmd::TrustNewCert => {
                if let Some((host, _, _)) = self.status.cert_changed.take() {
                    config::remove_pin(&host);
                    self.find_server(self.cfg.token.is_none());
                }
            }
            Cmd::Refresh => {}
        }
        self.refresh();
        self.publish();
    }

    fn run(mut self, rx: Receiver<Cmd>) {
        let first_run = !config::exists();
        self.status.suggest = config::bundled_url();
        self.searching = true;
        self.publish();
        self.find_server(self.cfg.token.is_none());
        self.searching = false;
        self.refresh();
        self.publish();
        // Asistente: si falta algo para funcionar, se abre el widget solo la primera vez
        // (o si nunca se encontro el servidor)
        let needs_setup = self.base.is_none() || self.cfg.token.is_none();
        if needs_setup && (first_run || self.cfg.url.is_none()) {
            let _ = self.proxy.send_event(UserEvent::ShowWidget);
        }
        loop {
            match rx.recv_timeout(POLL) {
                Ok(cmd) => self.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {
                    if self.token().is_some() {
                        self.refresh();
                        self.publish();
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }
}

// ─── Mensajes del widget ───

/// Id de lista tal como va en la ruta (sin `/`, `?` ni nada raro)
fn playlist_id(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(64).collect()
}

fn ipc_to_cmd(msg: &Value) -> Option<Cmd> {
    let int = |k: &str| msg[k].as_u64().map(|v| v as usize);
    Some(match msg["cmd"].as_str()? {
        "toggle" => Cmd::Toggle,
        "next" => Cmd::Next,
        "previous" => Cmd::Previous,
        "volume" => Cmd::SetVolume(msg["value"].as_u64()?.min(100) as u8),
        "shuffle" => Cmd::Shuffle,
        "repeat" => Cmd::Repeat,
        "search" => Cmd::Search(shorten(msg["q"].as_str()?.trim(), 200)),
        "play" => Cmd::Play { id: msg["id"].as_str()?.chars().take(64).collect(), now: msg["now"].as_bool().unwrap_or(false) },
        "queuePlay" => Cmd::QueuePlay(int("index")?),
        "queueRemove" => Cmd::QueueRemove(int("index")?),
        "queueMove" => Cmd::QueueMove(int("from")?, int("to")?),
        "playlists" => Cmd::Playlists,
        "loadPlaylist" => Cmd::LoadPlaylist(msg["id"].as_str()?.to_string()),
        "mix" => Cmd::Mix,
        "seek" => Cmd::Seek(u32::try_from(msg["pos"].as_u64()?).ok()?),
        "saveTo" => Cmd::SaveTo { id: playlist_id(msg["id"].as_str()?), name: shorten(msg["name"].as_str().unwrap_or(""), 60) },
        "notify" => Cmd::SetNotify(msg["on"].as_bool()?),
        "notices" => Cmd::SetNotices(msg["key"].as_str().filter(|k| ["personal", "lab", "admin"].contains(k))?.to_string(), msg["on"].as_bool()?),
        "link" => Cmd::Link,
        "unlink" => Cmd::Unlink,
        "rediscover" => Cmd::Rediscover,
        "changeUrl" => Cmd::ChangeUrl,
        "setUrl" => Cmd::SetUrl(msg["url"].as_str()?.to_string()),
        "trustCert" => Cmd::TrustNewCert,
        "ready" => Cmd::Refresh,
        _ => return None,
    })
}

// ─── Widget ───

struct Widget {
    window: Window,
    webview: WebView,
    _context: WebContext,
    /// Cuando se oculto por perder el foco (el clic en el icono que lo causo no debe reabrirlo)
    hidden_at: Option<Instant>,
    /// Cuando se mostro (en Windows el foco tarda un instante en llegar)
    shown_at: Option<Instant>,
    /// Alto logico actual (compacto o con la cola / busqueda abierta)
    height: f64,
    /// Donde estaba el icono al abrirlo, para reubicarlo al cambiar de alto
    rect: Option<tray_icon::Rect>,
    /// Ultimo alto real (fisico) informado por el sistema
    seen_h: u32,
}

/// Build de pruebas con `LABNAS_TRAY_TEST_JS`: el widget no se oculta solo
fn testing() -> bool {
    cfg!(debug_assertions) && std::env::var_os("LABNAS_TRAY_TEST_JS").is_some()
}

/// La pagina del widget con el logo de LabNAS incrustado
fn widget_html() -> String {
    let logo = format!("data:image/svg+xml,{}", http::pct_encode(include_str!("../assets/logo.svg")));
    let html = include_str!("../assets/widget.html").replace("__LOGO__", &logo);
    // Windows: la ventana es opaca (sombra y esquinas las pone el sistema) y la tarjeta la llena
    if cfg!(windows) { html.replace("<body>", "<body class=\"flat\">") } else { html }
}

fn build_widget(target: &EventLoopWindowTarget<UserEvent>, proxy: &EventLoopProxy<UserEvent>) -> Option<Widget> {
    let builder = WindowBuilder::new()
        .with_title("LabNAS")
        .with_inner_size(LogicalSize::new(WIDGET_W, WIDGET_H))
        // GTK no cambia el tamano de una ventana fija ni desde el programa; sin bordes igual
        // no se puede estirar con el raton
        .with_resizable(cfg!(target_os = "linux"))
        .with_decorations(false)
        // Windows: con transparencia la sombra del sistema dibuja un marco cuadrado alrededor
        // de la tarjeta redondeada; ahi va opaca con sombra y esquinas de Windows 11
        .with_transparent(!cfg!(windows))
        .with_always_on_top(true)
        .with_visible(false)
        .with_window_icon(window_icon());
    #[cfg(windows)]
    let builder = {
        use tao::platform::windows::WindowBuilderExtWindows;
        builder.with_skip_taskbar(true).with_undecorated_shadow(true)
    };
    #[cfg(target_os = "linux")]
    let builder = {
        use tao::platform::unix::WindowBuilderExtUnix;
        builder.with_skip_taskbar(true)
    };
    let window = builder
        .build(target)
        .map_err(|e| log!("no se pudo crear el widget: {e}"))
        .ok()?;
    #[cfg(windows)]
    round_corners(&window);

    // Datos del webview fuera de la carpeta del .exe (puede no tener permiso de escritura)
    let mut context = WebContext::new(dirs::data_local_dir().map(|d| d.join("labnas-tray").join("webview")));
    let ipc_proxy = proxy.clone();
    let wb = WebViewBuilder::new_with_web_context(&mut context)
        .with_html(widget_html())
        .with_transparent(!cfg!(windows))
        .with_devtools(cfg!(debug_assertions))
        .with_ipc_handler(move |req| {
            let _ = ipc_proxy.send_event(UserEvent::Ipc(req.body().clone()));
        })
        // enlaces: al navegador del sistema, nunca dentro del widget
        .with_navigation_handler(|url| url.starts_with("about:") || url.starts_with("data:"))
        .with_new_window_req_handler(|url, _| {
            let _ = open::that_detached(url);
            wry::NewWindowResponse::Deny
        });
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox()?;
        wb.build_gtk(vbox)
    };
    #[cfg(not(target_os = "linux"))]
    let webview = wb.build(&window);
    let webview = webview.map_err(|e| log!("no se pudo crear el webview: {e}")).ok()?;
    Some(Widget { window, webview, _context: context, hidden_at: None, shown_at: None, height: WIDGET_H, rect: None, seen_h: 0 })
}

/// Ubica el widget junto al icono (Windows da su posicion) o arriba a la derecha
fn place_widget(w: &Window, target: &EventLoopWindowTarget<UserEvent>, rect: Option<tray_icon::Rect>, height: f64) {
    let point = rect.map(|r| (r.position.x + f64::from(r.size.width) / 2.0, r.position.y + f64::from(r.size.height) / 2.0));
    let monitor = point
        .and_then(|(x, y)| {
            target.available_monitors().find(|m| {
                let (p, s) = (m.position(), m.size());
                x >= f64::from(p.x) && x < f64::from(p.x) + f64::from(s.width) && y >= f64::from(p.y) && y < f64::from(p.y) + f64::from(s.height)
            })
        })
        .or_else(|| target.primary_monitor())
        .or_else(|| w.current_monitor());
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let (ww, wh) = (WIDGET_W * scale, height * scale);
    let (mx, my) = (f64::from(m.position().x), f64::from(m.position().y));
    let (mw, mh) = (f64::from(m.size().width), f64::from(m.size().height));
    let margin = 10.0 * scale;
    let (x, y) = match (rect, point) {
        (Some(r), Some((cx, _))) => {
            let x = (cx - ww / 2.0).clamp(mx + margin, mx + mw - ww - margin);
            // barra de tareas abajo (lo comun) o arriba
            let y = if r.position.y > my + mh / 2.0 {
                r.position.y - wh - margin
            } else {
                r.position.y + f64::from(r.size.height) + margin
            };
            (x, y.clamp(my, my + mh - wh))
        }
        // Linux no da la posicion del icono: esquina de la bandeja segun el escritorio
        // (GNOME arriba a la derecha; KDE, XFCE, Cinnamon... abajo a la derecha)
        _ if panel_on_top() => (mx + mw - ww - margin, my + 44.0 * scale),
        _ => (mx + mw - ww - margin, my + mh - wh - 56.0 * scale),
    };
    if std::env::var_os("LABNAS_TRAY_DEBUG").is_some() {
        log!("monitor {:?} {:?} escala {scale} -> widget en ({x:.0}, {y:.0})", m.position(), m.size());
    }
    w.set_outer_position(PhysicalPosition::new(x, y));
}

/// Windows: oculta el widget si otra ventana paso a ser la activa; mientras se ve, el
/// bucle despierta cada 200 ms para revisarlo
#[cfg(windows)]
fn auto_hide(widget: &mut Option<Widget>, control_flow: &mut ControlFlow) {
    if *control_flow == ControlFlow::Exit {
        return;
    }
    let Some(w) = widget.as_mut() else { return };
    if !w.window.is_visible() || testing() {
        return;
    }
    let settled = w.shown_at.is_none_or(|t| t.elapsed() > Duration::from_millis(400));
    if settled && !is_foreground(&w.window) {
        w.window.set_visible(false);
        w.hidden_at = Some(Instant::now());
    } else {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(200));
    }
}

/// Windows 11: esquinas redondeadas del sistema en la ventana sin bordes (en 10 no hace nada)
#[cfg(windows)]
fn round_corners(w: &Window) {
    use tao::platform::windows::WindowExtWindows;
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
    let pref = DWMWCP_ROUND;
    // SAFETY: hwnd valido de esta ventana; el puntero apunta a un i32 vivo durante la llamada
    unsafe {
        DwmSetWindowAttribute(w.hwnd() as _, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&raw const pref).cast(), size_of::<i32>() as u32);
    }
}

/// El widget (o su webview) es la ventana activa del sistema
#[cfg(windows)]
fn is_foreground(w: &Window) -> bool {
    use tao::platform::windows::WindowExtWindows;
    // SAFETY: GetForegroundWindow no tiene precondiciones
    let fg = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    fg as isize == w.hwnd()
}

fn panel_on_top() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_ascii_uppercase().contains("GNOME"))
}

/// En Wayland una ventana no puede elegir su posicion: el widget saldria al centro. Con
/// XWayland (GTK por X11) KWin y Mutter si la respetan; el icono va por D-Bus y no cambia.
/// `LABNAS_TRAY_WAYLAND=1` deja Wayland nativo.
#[cfg(target_os = "linux")]
fn prefer_xwayland() {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let chosen = std::env::var_os("GDK_BACKEND").is_some();
    let native = std::env::var("LABNAS_TRAY_WAYLAND").is_ok_and(|v| v == "1");
    if wayland && !chosen && !native && std::env::var_os("DISPLAY").is_some() {
        // SAFETY: al inicio de main, antes de crear hilos o iniciar GTK
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }
}

fn push_status(w: &Widget, s: &Status) {
    let _ = w.webview.evaluate_script(&format!("window.labnas && window.labnas.update({})", s.to_json()));
}

fn push_event(w: &Widget, ev: &Value) {
    let _ = w.webview.evaluate_script(&format!("window.labnas && window.labnas.event({ev})"));
}

// ─── Bandeja ───

fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("../assets/tray.png").as_slice()));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    (info.color_type == png::ColorType::Rgba).then_some((buf, info.width, info.height))
}

fn window_icon() -> Option<tao::window::Icon> {
    icon_rgba().and_then(|(rgba, w, h)| tao::window::Icon::from_rgba(rgba, w, h).ok())
}

struct Items {
    open_widget: MenuItem,
    headline: MenuItem,
    toggle: MenuItem,
    next: MenuItem,
    previous: MenuItem,
    vol_up: MenuItem,
    vol_down: MenuItem,
    open_web: MenuItem,
    autostart: CheckMenuItem,
    quit: MenuItem,
}

impl Items {
    fn new() -> Self {
        Items {
            open_widget: MenuItem::new("Abrir control de música", true, None),
            headline: MenuItem::new("Buscando LabNAS…", false, None),
            toggle: MenuItem::new("⏸  Pausar", false, None),
            next: MenuItem::new("⏭  Siguiente", false, None),
            previous: MenuItem::new("⏮  Anterior", false, None),
            vol_up: MenuItem::new(format!("🔊  Subir volumen (+{VOLUME_STEP})"), false, None),
            vol_down: MenuItem::new(format!("🔉  Bajar volumen (−{VOLUME_STEP})"), false, None),
            open_web: MenuItem::new("Abrir LabNAS en el navegador", false, None),
            autostart: CheckMenuItem::new("Iniciar con el sistema", true, autostart::enabled(), None),
            quit: MenuItem::new("Salir", true, None),
        }
    }

    fn menu(&self) -> Option<Menu> {
        let menu = Menu::new();
        menu.append_items(&[
            &self.open_widget,
            &PredefinedMenuItem::separator(),
            &self.headline,
            &self.toggle,
            &self.next,
            &self.previous,
            &self.vol_up,
            &self.vol_down,
            &PredefinedMenuItem::separator(),
            &self.open_web,
            &self.autostart,
            &PredefinedMenuItem::separator(),
            &self.quit,
        ])
        .ok()?;
        Some(menu)
    }

    fn show(&self, s: &Status, tray: Option<&tray_icon::TrayIcon>) {
        self.headline.set_text(&s.headline);
        let ready = s.phase == "listo";
        self.toggle.set_text(if s.playing && s.paused { "▶  Reanudar" } else { "⏸  Pausar" });
        self.toggle.set_enabled(ready && s.playing);
        self.next.set_enabled(ready);
        self.previous.set_enabled(ready && s.playing);
        self.vol_up.set_enabled(ready);
        self.vol_down.set_enabled(ready);
        self.open_web.set_enabled(s.base.is_some());
        if let Some(t) = tray {
            let _ = t.set_tooltip(Some(format!("LabNAS\n{}", s.headline)));
        }
    }
}

/// `labnas-tray --probar <url>`: comprueba la conexion (redireccion a HTTPS y certificado)
fn probe(url: &str) -> ! {
    let Some(u) = normalize_url(url) else {
        eprintln!("URL invalida: {url}");
        std::process::exit(2);
    };
    match resolve(&u) {
        Ok(base) => {
            println!("LabNAS responde en {}", base.base());
            std::process::exit(0);
        }
        Err(http::Error::CertChanged(h, old, new)) => {
            eprintln!("El certificado de {h} cambio.\n  guardado: {old}\n  actual:   {new}");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{}: {e}", u.base());
            std::process::exit(1);
        }
    }
}

/// Una sola copia por usuario: si ya hay una, se le pide abrir el widget y se sale.
/// Devuelve el listener de la copia principal.
fn single_instance() -> Option<std::net::TcpListener> {
    use std::io::Write;
    // pruebas: convivir con la copia instalada
    #[cfg(debug_assertions)]
    if std::env::var_os("LABNAS_TRAY_TEST_JS").is_some() {
        return None;
    }
    match std::net::TcpListener::bind(("127.0.0.1", SINGLE_INSTANCE_PORT)) {
        Ok(l) => Some(l),
        Err(_) => {
            if let Ok(mut s) = std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], SINGLE_INSTANCE_PORT).into(), Duration::from_secs(1)) {
                let _ = s.write_all(b"labnas-tray abrir\n");
                std::process::exit(0);
            }
            // el puerto lo usa otro programa: seguir sin instancia unica
            None
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--probar") {
        probe(args.get(2).map(String::as_str).unwrap_or(""));
    }
    let listener = single_instance();
    logfile::init();
    log!("labnas-tray {} iniciado", env!("CARGO_PKG_VERSION"));
    #[cfg(target_os = "linux")]
    prefer_xwayland();

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = menu_proxy.send_event(UserEvent::Menu(e.id));
    }));
    let click_proxy = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, mut rect, position, .. } = e {
            // Linux (StatusNotifierItem) no da el rectangulo del icono, solo donde se hizo clic
            if rect.size.width == 0 && (position.x != 0.0 || position.y != 0.0) {
                rect = tray_icon::Rect { position, size: tray_icon::dpi::PhysicalSize::new(1, 1) };
            }
            let rect = (rect.size.width > 0).then_some(rect);
            let _ = click_proxy.send_event(UserEvent::TrayClick(rect));
        }
    }));

    let items = Items::new();
    let tray = match (icon_rgba().and_then(|(rgba, w, h)| tray_icon::Icon::from_rgba(rgba, w, h).ok()), items.menu()) {
        (Some(icon), Some(menu)) => tray_icon::TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("LabNAS")
            .with_menu(Box::new(menu))
            // clic izquierdo: widget; el menu va con el derecho
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| log!("no se pudo crear el icono de la bandeja: {e}"))
            .ok(),
        _ => None,
    };
    if tray.is_none() {
        log!("este escritorio no tiene bandeja del sistema (en GNOME instala la extension AppIndicator)");
        std::process::exit(1);
    }

    let (tx, rx) = mpsc::channel::<Cmd>();
    // teclas multimedia y reproductor del sistema
    let media_tx = tx.clone();
    let media_proxy = proxy.clone();
    let mut media = media::Media::new(&event_loop, move |a| {
        let cmd = match a {
            media::Action::SetPaused(p) => Cmd::SetPaused(p),
            media::Action::Toggle => Cmd::Toggle,
            media::Action::Next => Cmd::Next,
            media::Action::Previous => Cmd::Previous,
            media::Action::SeekTo(s) => Cmd::Seek(s),
            media::Action::SeekBy(d) => Cmd::SeekBy(d),
            media::Action::Volume(v) => Cmd::SetVolume(v),
            media::Action::Raise => {
                let _ = media_proxy.send_event(UserEvent::ShowWidget);
                return;
            }
        };
        let _ = media_tx.send(cmd);
    });
    #[cfg(windows)]
    {
        // los avisos de LabNAS vienen activados: Windows necesita la identidad desde ya
        let c = config::load();
        if c.notify || !c.quiet_personal || c.lab || !c.quiet_admin {
            std::thread::spawn(media::register_notifications);
        }
    }
    let worker = Worker {
        cfg: config::load(),
        base: None,
        status: Status::default(),
        proxy: proxy.clone(),
        tx: tx.clone(),
        last_discovery: None,
        searching: false,
        offline: false,
        linking: false,
        force_setup: false,
        last_look: None,
        last_track: None,
        notices: notices::Shared::default(),
    };
    notices::spawn(worker.notices.clone());
    std::thread::spawn(move || worker.run(rx));
    if let Some(listener) = listener {
        let p = proxy.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            for stream in listener.incoming().flatten() {
                let mut buf = [0u8; 32];
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                let n = (&stream).read(&mut buf).unwrap_or(0);
                if buf[..n].starts_with(b"labnas-tray abrir") {
                    let _ = p.send_event(UserEvent::ShowWidget);
                }
            }
        });
    }
    // `--abrir`: abre el widget al iniciar (acceso directo del escritorio)
    if args.iter().any(|a| a == "--abrir") {
        let _ = proxy.send_event(UserEvent::ShowWidget);
    }

    let mut status = Status::default();
    let mut widget: Option<Widget> = None;
    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        let send = |c: Cmd| {
            let _ = tx.send(c);
        };
        let show = |widget: &mut Option<Widget>, rect: Option<tray_icon::Rect>, status: &Status| {
            if widget.is_none() {
                *widget = build_widget(target, &proxy);
            }
            let Some(w) = widget.as_mut() else { return };
            w.rect = rect;
            place_widget(&w.window, target, rect, w.height);
            w.window.set_visible(true);
            w.window.set_focus();
            w.hidden_at = None;
            w.shown_at = Some(Instant::now());
            push_status(w, status);
            push_event(w, &json!({ "type": "shown" }));
        };
        match event {
            // Windows: tao avisa "sin foco" tambien cuando el foco pasa al webview (ventana
            // hija); ahi se mira la ventana activa del sistema (ver mas abajo)
            #[cfg(not(windows))]
            Event::WindowEvent { event: WindowEvent::Focused(false), .. } => {
                if let Some(w) = widget.as_mut()
                    && w.window.is_visible()
                    && !testing()
                {
                    w.window.set_visible(false);
                    w.hidden_at = Some(Instant::now());
                }
            }
            // el gestor de ventanas puede aplicar el cambio de alto despues de mover la
            // ventana: se vuelve a anclar junto a la barra cuando el alto ya cambio
            // (en GTK mover tambien avisa Resized: solo cuenta si cambio el alto)
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                if let Some(w) = widget.as_mut()
                    && size.height != std::mem::replace(&mut w.seen_h, size.height)
                    && w.window.is_visible()
                {
                    place_widget(&w.window, target, w.rect, w.height);
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                if let Some(w) = widget.as_ref() {
                    w.window.set_visible(false);
                }
            }
            Event::UserEvent(ev) => match ev {
                UserEvent::Status(s) => {
                    status = *s;
                    items.show(&status, tray.as_ref());
                    if let Some(m) = media.as_mut() {
                        m.update(status.music.as_ref().filter(|_| status.phase == "listo"));
                    }
                    if let Some(w) = widget.as_ref() {
                        push_status(w, &status);
                    }
                }
                UserEvent::Widget(ev) => {
                    if let Some(w) = widget.as_ref() {
                        push_event(w, &ev);
                    }
                }
                UserEvent::Ipc(body) => {
                    let Ok(msg) = serde_json::from_str::<Value>(&body) else { return };
                    match msg["cmd"].as_str() {
                        Some("hide") => {
                            if let Some(w) = widget.as_ref() {
                                w.window.set_visible(false);
                            }
                        }
                        // la pagina pide alto: compacta o con la cola / busqueda abierta
                        Some("size") => {
                            let Some(h) = msg["h"].as_f64() else { return };
                            let h = h.clamp(WIDGET_H_RANGE.0, WIDGET_H_RANGE.1).round();
                            if let Some(w) = widget.as_mut()
                                && (h - w.height).abs() >= 1.0
                            {
                                w.height = h;
                                w.window.set_inner_size(LogicalSize::new(WIDGET_W, h));
                                if w.window.is_visible() {
                                    place_widget(&w.window, target, w.rect, h);
                                }
                            }
                        }
                        Some("openWeb") => {
                            if let Some(b) = &status.base {
                                let _ = open::that_detached(b);
                            }
                        }
                        Some("ready") => {
                            if let Some(w) = widget.as_ref() {
                                push_status(w, &status);
                                // pruebas: LABNAS_TRAY_TEST_JS=<archivo> se ejecuta en el widget
                                #[cfg(debug_assertions)]
                                if let Some(js) = std::env::var_os("LABNAS_TRAY_TEST_JS").and_then(|p| std::fs::read_to_string(p).ok()) {
                                    let _ = w.webview.evaluate_script(&js);
                                }
                            }
                        }
                        _ => {
                            if let Some(c) = ipc_to_cmd(&msg) {
                                send(c);
                            }
                        }
                    }
                }
                UserEvent::TrayClick(rect) => {
                    let visible = widget.as_ref().is_some_and(|w| w.window.is_visible());
                    // el clic que le quito el foco ya lo cerro: no reabrir
                    let just_hidden = widget
                        .as_ref()
                        .and_then(|w| w.hidden_at)
                        .is_some_and(|t| t.elapsed() < Duration::from_millis(350));
                    if visible {
                        if let Some(w) = widget.as_ref() {
                            w.window.set_visible(false);
                        }
                    } else if !just_hidden {
                        show(&mut widget, rect, &status);
                    }
                }
                UserEvent::ShowWidget => show(&mut widget, None, &status),
                UserEvent::Menu(id) => {
                    if id == *items.open_widget.id() {
                        show(&mut widget, None, &status);
                    } else if id == *items.toggle.id() {
                        send(Cmd::Toggle);
                    } else if id == *items.next.id() {
                        send(Cmd::Next);
                    } else if id == *items.previous.id() {
                        send(Cmd::Previous);
                    } else if id == *items.vol_up.id() {
                        send(Cmd::Volume(VOLUME_STEP));
                    } else if id == *items.vol_down.id() {
                        send(Cmd::Volume(-VOLUME_STEP));
                    } else if id == *items.open_web.id() {
                        if let Some(b) = &status.base {
                            let _ = open::that_detached(b);
                        }
                    } else if id == *items.autostart.id() {
                        let want = items.autostart.is_checked();
                        if let Err(e) = autostart::set(want) {
                            log!("iniciar con el sistema: {e}");
                            items.autostart.set_checked(!want);
                        }
                    } else if id == *items.quit.id() {
                        *control_flow = ControlFlow::Exit;
                    }
                }
            },
            _ => {}
        }
        // Windows: se cierra cuando otra ventana pasa a ser la activa (clic afuera)
        #[cfg(windows)]
        auto_hide(&mut widget, control_flow);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acortar() {
        assert_eq!(shorten("hola", 10), "hola");
        assert_eq!(shorten("ñandú volador", 6), "ñandú…");
    }

    #[test]
    fn direcciones_escritas() {
        assert_eq!(normalize_url("192.168.1.10:3001").map(|u| u.base()).as_deref(), Some("http://192.168.1.10:3001"));
        assert_eq!(normalize_url(" https://labnas.local:3443/ ").map(|u| u.base()).as_deref(), Some("https://labnas.local:3443"));
        assert!(normalize_url("   ").is_none());
    }

    #[test]
    fn pieles_al_dia() {
        // cada piel de la web debe estar en el widget (si se agrega una, regenerar THEMES)
        let ts = include_str!("../../frontend/src/themes/themes.ts");
        let html = widget_html();
        let union = ts.split("export type ThemeName =").nth(1).and_then(|r| r.lines().next()).unwrap_or("");
        let names: Vec<&str> = union.split('|').map(|n| n.trim().trim_matches('\'')).filter(|n| !n.is_empty()).collect();
        assert!(names.len() >= 9, "{names:?}");
        for n in names {
            assert!(html.contains(&format!("\"{n}\":{{")), "falta la piel {n} en widget.html");
        }
        assert!(html.contains("data:image/svg+xml,") && !html.contains("__LOGO__"));
    }

    #[test]
    fn mensajes_del_widget() {
        assert!(matches!(ipc_to_cmd(&json!({ "cmd": "volume", "value": 140 })), Some(Cmd::SetVolume(100))));
        assert!(matches!(ipc_to_cmd(&json!({ "cmd": "queueMove", "from": 3, "to": 0 })), Some(Cmd::QueueMove(3, 0))));
        assert!(matches!(ipc_to_cmd(&json!({ "cmd": "play", "id": "abc", "now": true })), Some(Cmd::Play { now: true, .. })));
        assert!(ipc_to_cmd(&json!({ "cmd": "queuePlay" })).is_none());
        assert!(ipc_to_cmd(&json!({ "cmd": "rm -rf" })).is_none());
    }
}
