//! labnas-tray: control de la musica de LabNAS desde la bandeja del sistema, para los
//! computadores de quienes usan el laboratorio (Windows y Linux).
//!
//! - Encuentra el LabNAS por mDNS (o usa `url=` de su archivo de configuracion).
//! - Se vincula a la cuenta desde el navegador ("Vincular con mi cuenta"): recibe un
//!   token de dispositivo que solo sirve para controlar el reproductor y que se puede
//!   revocar en LabNAS (Configuracion > General > Dispositivos vinculados).
//! - El menu muestra lo que suena y permite pausar, pasar, retroceder y cambiar el volumen.
//!
//! Hilos: el principal maneja la bandeja (tao + tray-icon); uno aparte hace la red y le
//! manda el estado por el `EventLoopProxy`, asi la bandeja nunca se congela.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod autostart;
mod config;
mod http;
mod link;

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

use http::Url;

const MDNS_TYPE: &str = "_labnas._tcp.local.";
const MDNS_TIMEOUT: Duration = Duration::from_millis(2500);
/// Cada cuanto se consulta lo que suena
const POLL: Duration = Duration::from_secs(3);
/// Si el servidor no responde, cada cuanto se lo vuelve a buscar por mDNS (cambio de IP)
const REDISCOVER_EVERY: Duration = Duration::from_secs(60);
const VOLUME_STEP: i64 = 10;

/// Lo que la bandeja muestra
#[derive(Debug, Clone, Default)]
struct Status {
    /// Primera linea del menu (y del tooltip)
    headline: String,
    /// Hay una cancion (habilita pausar y volumen)
    playing: bool,
    paused: bool,
    volume: Option<u8>,
    /// Hay token de dispositivo
    linked: bool,
    user: Option<String>,
    /// Se puede abrir LabNAS en el navegador
    base: Option<String>,
    /// Vinculacion en curso (esperando al navegador)
    linking: bool,
    /// Certificado cambiado: (host, huella guardada, actual)
    cert_changed: Option<(String, String, String)>,
}

enum UserEvent {
    Menu(MenuId),
    TrayLeftClick,
    Status(Box<Status>),
}

enum Cmd {
    Toggle,
    Next,
    Previous,
    Volume(i64),
    Link,
    Unlink,
    Rediscover,
    TrustNewCert,
    Linked(Result<link::Linked, String>),
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

/// Sigue la redireccion a HTTPS (si el servidor la tiene) y comprueba que responde
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

struct Worker {
    cfg: config::Config,
    base: Option<Url>,
    status: Status,
    proxy: EventLoopProxy<UserEvent>,
    tx: Sender<Cmd>,
    last_discovery: Option<Instant>,
}

impl Worker {
    fn publish(&mut self) {
        self.status.linked = self.cfg.token.is_some();
        self.status.user = self.cfg.user.clone();
        self.status.base = self.base.as_ref().map(Url::base);
        let _ = self.proxy.send_event(UserEvent::Status(Box::new(self.status.clone())));
    }

    fn headline(&mut self, text: impl Into<String>) {
        self.status.headline = text.into();
        self.status.playing = false;
        self.status.volume = None;
    }

    /// Busca el servidor: config, y si no responde o no hay, mDNS
    fn find_server(&mut self) {
        self.last_discovery = Some(Instant::now());
        let mut candidates: Vec<String> = Vec::new();
        if let Ok(env) = std::env::var("LABNAS_URL") {
            candidates.push(env);
        }
        if let Some(u) = &self.cfg.url {
            candidates.push(u.clone());
        }
        let mut tried_mdns = false;
        loop {
            for c in candidates.drain(..) {
                let Some(url) = Url::parse(&c) else { continue };
                match resolve(&url) {
                    Ok(base) => {
                        self.status.cert_changed = None;
                        if self.cfg.url.as_deref() != Some(base.base().as_str()) {
                            self.cfg.url = Some(base.base());
                            config::save(&self.cfg);
                        }
                        self.base = Some(base);
                        return;
                    }
                    Err(http::Error::CertChanged(h, old, new)) => {
                        self.base = Some(url);
                        self.status.cert_changed = Some((h.clone(), old, new));
                        self.headline(format!("⚠ El certificado de {h} cambió"));
                        return;
                    }
                    Err(e) => eprintln!("[labnas-tray] {c}: {e}"),
                }
            }
            if tried_mdns {
                break;
            }
            tried_mdns = true;
            match discover() {
                Some(u) => candidates.push(u),
                None => break,
            }
        }
        if self.base.is_none() {
            self.headline("LabNAS no encontrado en la red");
        }
    }

    fn token(&self) -> Option<&str> {
        self.cfg.token.as_deref()
    }

    /// Pedido autenticado a la API; devuelve la respuesta si el servidor contesto
    fn call(&mut self, method: &str, path: &str, body: Option<&str>) -> Option<http::Response> {
        let (Some(base), Some(token)) = (self.base.clone(), self.token().map(str::to_string)) else {
            return None;
        };
        match http::request(&base, method, path, Some(&token), body) {
            Ok(r) => Some(r),
            Err(http::Error::CertChanged(h, old, new)) => {
                self.status.cert_changed = Some((h.clone(), old, new));
                self.headline(format!("⚠ El certificado de {h} cambió"));
                None
            }
            Err(e) => {
                eprintln!("[labnas-tray] {path}: {e}");
                self.headline("LabNAS no responde");
                let stale = self.last_discovery.is_none_or(|t| t.elapsed() > REDISCOVER_EVERY);
                if stale {
                    self.find_server();
                }
                None
            }
        }
    }

    /// Interpreta un MusicState (o el error) y actualiza el estado
    fn apply(&mut self, r: http::Response) {
        match r.status {
            200 => {
                let Some(ms) = r.json() else {
                    self.headline("Respuesta extraña de LabNAS");
                    return;
                };
                self.status.volume = ms["volume"].as_u64().map(|v| v.min(100) as u8);
                self.status.paused = ms["paused"].as_bool().unwrap_or(false);
                match ms["current"].as_object() {
                    Some(cur) => {
                        let title = cur.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        let artist = cur.get("artist").and_then(|v| v.as_str()).unwrap_or("");
                        let icon = if self.status.paused { "⏸" } else { "♪" };
                        let text = if artist.is_empty() { title.to_string() } else { format!("{title} — {artist}") };
                        self.status.headline = format!("{icon} {}", shorten(&text, 60));
                        self.status.playing = true;
                    }
                    None => {
                        self.status.headline = "Nada sonando".into();
                        self.status.playing = false;
                    }
                }
            }
            401 => {
                // revocado desde LabNAS (o la cuenta se borro)
                self.cfg.token = None;
                self.cfg.user = None;
                config::save(&self.cfg);
                self.headline("Vinculación revocada: vuelve a vincular");
            }
            404 => self.headline("La música está desactivada en LabNAS"),
            _ => {
                let msg = r.text();
                self.headline(shorten(if msg.is_empty() { "Error de LabNAS" } else { &msg }, 60));
            }
        }
    }

    fn refresh(&mut self) {
        if self.token().is_none() {
            if self.base.is_some() && !self.status.linking && self.status.cert_changed.is_none() {
                self.headline("Sin vincular a tu cuenta");
            }
            return;
        }
        if self.base.is_none() {
            let stale = self.last_discovery.is_none_or(|t| t.elapsed() > REDISCOVER_EVERY);
            if stale {
                self.find_server();
            }
            if self.base.is_none() {
                return;
            }
        }
        if self.status.cert_changed.is_some() {
            return;
        }
        if let Some(r) = self.call("GET", "/api/music/current", None) {
            self.apply(r);
        }
    }

    fn action(&mut self, path: &str, body: Option<String>) {
        if let Some(r) = self.call("POST", path, Some(body.as_deref().unwrap_or("{}"))) {
            self.apply(r);
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Toggle => self.action("/api/music/pause", None),
            Cmd::Next => self.action("/api/music/next", None),
            Cmd::Previous => self.action("/api/music/previous", None),
            Cmd::Volume(delta) => {
                let cur = i64::from(self.status.volume.unwrap_or(50));
                let v = (cur + delta).clamp(0, 100);
                self.action("/api/music/volume", Some(format!("{{\"volume\":{v}}}")));
            }
            Cmd::Link => {
                if self.base.is_none() {
                    self.find_server();
                }
                let Some(base) = self.base.as_ref().map(Url::base) else {
                    self.publish();
                    return;
                };
                self.status.linking = true;
                self.headline("Autoriza en el navegador…");
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Cmd::Linked(link::link(&base)));
                });
            }
            Cmd::Linked(res) => {
                self.status.linking = false;
                match res {
                    Ok(l) => {
                        self.cfg.token = Some(l.token);
                        self.cfg.user = l.user;
                        config::save(&self.cfg);
                    }
                    Err(e) => {
                        eprintln!("[labnas-tray] vincular: {e}");
                        self.headline(shorten(&format!("No se vinculó: {e}"), 60));
                        self.publish();
                        return;
                    }
                }
            }
            Cmd::Unlink => {
                // avisar a LabNAS para que el token deje de valer; si no responde, igual se olvida
                let _ = self.call("DELETE", "/api/auth/device-tokens/me", None);
                self.cfg.token = None;
                self.cfg.user = None;
                config::save(&self.cfg);
            }
            Cmd::Rediscover => {
                self.base = None;
                self.status.cert_changed = None;
                self.headline("Buscando LabNAS…");
                self.publish();
                let saved = self.cfg.url.take();
                self.find_server();
                if self.base.is_none() {
                    self.cfg.url = saved;
                }
            }
            Cmd::TrustNewCert => {
                if let Some((host, _, _)) = self.status.cert_changed.take() {
                    config::remove_pin(&host);
                    self.find_server();
                }
            }
        }
        self.refresh();
        self.publish();
    }

    fn run(mut self, rx: Receiver<Cmd>) {
        self.headline("Buscando LabNAS…");
        self.publish();
        self.find_server();
        self.refresh();
        self.publish();
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

// ─── Bandeja ───

fn icon() -> Option<tray_icon::Icon> {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("../assets/tray.png").as_slice()));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    tray_icon::Icon::from_rgba(buf, info.width, info.height).ok()
}

struct Items {
    headline: MenuItem,
    toggle: MenuItem,
    next: MenuItem,
    previous: MenuItem,
    volume: Submenu,
    vol_up: MenuItem,
    vol_down: MenuItem,
    open: MenuItem,
    link: MenuItem,
    unlink: MenuItem,
    account: MenuItem,
    rediscover: MenuItem,
    trust: MenuItem,
    autostart: CheckMenuItem,
    quit: MenuItem,
}

impl Items {
    fn new() -> Self {
        Items {
            headline: MenuItem::new("Buscando LabNAS…", false, None),
            toggle: MenuItem::new("⏯  Pausar", false, None),
            next: MenuItem::new("⏭  Siguiente", false, None),
            previous: MenuItem::new("⏮  Anterior", false, None),
            volume: Submenu::new("Volumen", false),
            vol_up: MenuItem::new(format!("Subir (+{VOLUME_STEP})"), true, None),
            vol_down: MenuItem::new(format!("Bajar (−{VOLUME_STEP})"), true, None),
            open: MenuItem::new("Abrir LabNAS", false, None),
            link: MenuItem::new("Vincular con mi cuenta…", false, None),
            unlink: MenuItem::new("Desvincular este computador", false, None),
            account: MenuItem::new("", false, None),
            rediscover: MenuItem::new("Buscar LabNAS en la red", true, None),
            trust: MenuItem::new("Confiar en el certificado nuevo…", false, None),
            autostart: CheckMenuItem::new("Iniciar con el sistema", true, autostart::enabled(), None),
            quit: MenuItem::new("Salir", true, None),
        }
    }

    fn menu(&self) -> Option<Menu> {
        let menu = Menu::new();
        self.volume.append_items(&[&self.vol_up, &self.vol_down]).ok()?;
        menu.append_items(&[
            &self.headline,
            &PredefinedMenuItem::separator(),
            &self.toggle,
            &self.next,
            &self.previous,
            &self.volume,
            &PredefinedMenuItem::separator(),
            &self.open,
            &self.account,
            &self.link,
            &self.unlink,
            &self.trust,
            &self.rediscover,
            &self.autostart,
            &PredefinedMenuItem::separator(),
            &self.quit,
        ])
        .ok()?;
        Some(menu)
    }

    fn show(&self, s: &Status, tray: Option<&tray_icon::TrayIcon>) {
        self.headline.set_text(&s.headline);
        let can_control = s.linked && s.base.is_some() && s.cert_changed.is_none();
        self.toggle.set_text(if s.playing && s.paused { "▶  Reanudar" } else { "⏸  Pausar" });
        self.toggle.set_enabled(can_control && s.playing);
        self.next.set_enabled(can_control);
        self.previous.set_enabled(can_control);
        self.volume.set_text(match s.volume {
            Some(v) => format!("Volumen: {v}%"),
            None => "Volumen".to_string(),
        });
        self.volume.set_enabled(can_control && s.volume.is_some());
        self.open.set_enabled(s.base.is_some());
        self.link.set_enabled(!s.linked && !s.linking && s.cert_changed.is_none());
        self.link.set_text(if s.linking { "Esperando al navegador…" } else { "Vincular con mi cuenta…" });
        self.unlink.set_enabled(s.linked);
        self.account.set_text(match (&s.user, s.linked) {
            (Some(u), true) => format!("Vinculado como {u}"),
            (None, true) => "Vinculado".to_string(),
            _ => "Sin vincular".to_string(),
        });
        self.trust.set_enabled(s.cert_changed.is_some());
        if let Some(t) = tray {
            let tip = match &s.cert_changed {
                Some((h, old, new)) => format!("LabNAS: el certificado de {h} cambió\nGuardado: {old}\nActual: {new}"),
                None => format!("LabNAS\n{}", s.headline),
            };
            let _ = t.set_tooltip(Some(tip));
        }
    }
}

/// `labnas-tray --probar <url>`: comprueba la conexion (redireccion a HTTPS y certificado)
fn probe(url: &str) -> ! {
    let Some(u) = Url::parse(url) else {
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--probar") {
        probe(args.get(2).map(String::as_str).unwrap_or(""));
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = menu_proxy.send_event(UserEvent::Menu(e.id));
    }));
    let click_proxy = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            let _ = click_proxy.send_event(UserEvent::TrayLeftClick);
        }
    }));

    let items = Items::new();
    let tray = match (icon(), items.menu()) {
        (Some(icon), Some(menu)) => tray_icon::TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("LabNAS")
            .with_menu(Box::new(menu))
            // clic izquierdo: pausar/reanudar; el menu va con el derecho
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| eprintln!("[labnas-tray] no se pudo crear el icono de la bandeja: {e}"))
            .ok(),
        _ => None,
    };
    if tray.is_none() {
        eprintln!("[labnas-tray] este escritorio no tiene bandeja del sistema (en GNOME instala la extension AppIndicator)");
        std::process::exit(1);
    }

    let (tx, rx) = mpsc::channel::<Cmd>();
    let worker = Worker {
        cfg: config::load(),
        base: None,
        status: Status::default(),
        proxy: proxy.clone(),
        tx: tx.clone(),
        last_discovery: None,
    };
    std::thread::spawn(move || worker.run(rx));

    let mut status = Status::default();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let Event::UserEvent(ev) = event else { return };
        let send = |c: Cmd| {
            let _ = tx.send(c);
        };
        match ev {
            UserEvent::Status(s) => {
                status = *s;
                items.show(&status, tray.as_ref());
            }
            UserEvent::TrayLeftClick => {
                if status.linked && status.playing {
                    send(Cmd::Toggle);
                }
            }
            UserEvent::Menu(id) => {
                if id == *items.toggle.id() {
                    send(Cmd::Toggle);
                } else if id == *items.next.id() {
                    send(Cmd::Next);
                } else if id == *items.previous.id() {
                    send(Cmd::Previous);
                } else if id == *items.vol_up.id() {
                    send(Cmd::Volume(VOLUME_STEP));
                } else if id == *items.vol_down.id() {
                    send(Cmd::Volume(-VOLUME_STEP));
                } else if id == *items.open.id() {
                    if let Some(b) = &status.base {
                        let _ = open::that_detached(b);
                    }
                } else if id == *items.link.id() {
                    send(Cmd::Link);
                } else if id == *items.unlink.id() {
                    send(Cmd::Unlink);
                } else if id == *items.rediscover.id() {
                    send(Cmd::Rediscover);
                } else if id == *items.trust.id() {
                    send(Cmd::TrustNewCert);
                } else if id == *items.autostart.id() {
                    let want = items.autostart.is_checked();
                    if let Err(e) = autostart::set(want) {
                        eprintln!("[labnas-tray] iniciar con el sistema: {e}");
                        items.autostart.set_checked(!want);
                    }
                } else if id == *items.quit.id() {
                    *control_flow = ControlFlow::Exit;
                }
            }
        }
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
}
