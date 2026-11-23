//! LabNAS Viewer: ventana nativa (WebKitGTK via wry) que carga la web UI de LabNAS
//! sin depender de un navegador instalado.
//!
//! URL a cargar (primera que exista):
//!   1. argumento CLI:            labnas-viewer http://192.168.1.10:3001
//!      (`labnas-viewer --buscar` solo muestra el LabNAS encontrado por mDNS)
//!   2. variable de entorno:      LABNAS_URL
//!   3. archivo de config:        ~/.config/labnas-viewer/url
//!   4. LabNAS en la red por mDNS (_labnas._tcp; requiere mDNS activo en el servidor)
//!   5. por defecto:              http://localhost:3001
//!
//! Con bandeja del sistema (libappindicator), cerrar la ventana la oculta y el visor
//! sigue recibiendo notificaciones; "Salir" desde la bandeja o Ctrl+Q lo cierra.
//!
//! HTTPS: si el servidor tiene HTTPS activo, redirige al visor (que se anuncia con
//! `tls-pin` en el User-Agent). Un certificado autofirmado se acepta la primera vez y
//! su huella SHA-256 queda guardada en ~/.local/share/labnas-viewer/tls-pins, como
//! hace SSH. Si despues cambia, el visor no carga y muestra ambas huellas;
//! `labnas-viewer --olvidar <host>` borra la huella guardada.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::unix::{EventLoopBuilderExtUnix, WindowExtUnix};
use tao::window::WindowBuilder;
use sha2::{Digest, Sha256};
use webkit2gtk::{WebContextExt, WebViewExt};
use wry::http::Uri;
use wry::{NewWindowResponse, WebContext, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

const APP_ID: &str = "io.github.debaq.LabNAS";
const DEFAULT_URL: &str = "http://localhost:3001";

enum UserEvent {
    ServerReady,
    Show,
    Quit,
}

const MDNS_TYPE: &str = "_labnas._tcp.local.";
const MDNS_TIMEOUT: Duration = Duration::from_millis(2500);

/// Busca un LabNAS en la red local por mDNS
fn discover_labnas() -> Option<String> {
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

// ─── Certificados: confiar en el primer uso (TOFU) ───

/// Marca del User-Agent: este visor sabe fijar certificados, el servidor puede
/// mandarlo a HTTPS. Debe coincidir con `tls::VIEWER_TLS` del backend.
const VIEWER_TLS: &str = "tls-pin";

fn pins_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("labnas-viewer/tls-pins"))
}

/// Huella SHA-256 del certificado (DER) en el formato que muestra LabNAS: AA:BB:...
fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

#[derive(Debug, PartialEq)]
enum PinCheck {
    /// Coincide con la huella guardada
    Trusted,
    /// Primera vez que vemos este host
    New,
    /// Hay otra huella guardada para el host (la devuelve)
    Changed(String),
}

/// Lineas `host huella`; se ignoran las vacias y los comentarios
fn check_pin(pins: &str, host: &str, fp: &str) -> PinCheck {
    let saved = pins
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once(' '))
        .find(|(h, _)| *h == host)
        .map(|(_, f)| f.trim());
    match saved {
        Some(f) if f.eq_ignore_ascii_case(fp) => PinCheck::Trusted,
        Some(f) => PinCheck::Changed(f.to_string()),
        None => PinCheck::New,
    }
}

/// Contenido sin la huella de `host`
fn forget_pin(pins: &str, host: &str) -> String {
    pins.lines()
        .filter(|l| l.split_once(' ').map(|(h, _)| h) != Some(host))
        .map(|l| format!("{l}\n"))
        .collect()
}

fn read_pins() -> String {
    pins_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default()
}

fn save_pin(host: &str, fp: &str) {
    let Some(path) = pins_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut pins = forget_pin(&read_pins(), host);
    pins.push_str(&format!("{host} {fp}\n"));
    if let Err(e) = std::fs::write(&path, pins) {
        eprintln!("[labnas-viewer] no se pudo guardar la huella en {}: {e}", path.display());
    }
}

fn cert_changed_page(host: &str, old: &str, new: &str) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>
body{{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;
background:#282a36;color:#f8f8f2;font-family:system-ui,sans-serif}}
main{{max-width:720px;padding:24px}}h1{{color:#ff5555;font-size:1.4em}}
code{{color:#8be9fd;word-break:break-all}}p{{line-height:1.5}}
</style></head><body><main>
<h1>El certificado de {host} cambió</h1>
<p>El servidor presentó un certificado distinto al de la primera conexión. Puede ser que
alguien haya regenerado el certificado en LabNAS, o que otra máquina se esté haciendo pasar por él.
Por seguridad, el visor no se conectó.</p>
<p>Huella guardada:<br><code>{old}</code></p>
<p>Huella actual:<br><code>{new}</code></p>
<p>Compara la huella actual con la que aparece en LabNAS, en Configuración &gt; HTTPS
(míralo desde otro equipo o en el propio servidor). Si coinciden, ejecuta
<code>labnas-viewer --olvidar {host}</code> y vuelve a abrir el visor.</p>
</main></body></html>"#
    )
}

/// Engancha la comprobacion de certificados a WebKit: un autofirmado conocido (o
/// nuevo) se acepta para ese host; uno que cambio, no.
fn pin_certificates(webview: &wry::WebView) {
    webview.webview().connect_load_failed_with_tls_errors(|view, uri, cert, _errors| {
        let Some(host) = uri.parse::<Uri>().ok().and_then(|u| u.host().map(str::to_string)) else {
            return false;
        };
        let Some(der) = webkit2gtk::gio::prelude::TlsCertificateExt::certificate(cert) else {
            return false;
        };
        let fp = fingerprint(&der);
        match check_pin(&read_pins(), &host, &fp) {
            PinCheck::Changed(old) => {
                eprintln!("[labnas-viewer] el certificado de {host} cambio: guardada {old}, actual {fp}");
                notify("dialog-warning", "LabNAS", &format!("El certificado de {host} cambió; no se conectó"));
                view.load_html(&cert_changed_page(&host, &old, &fp), None);
            }
            check => {
                if check == PinCheck::New {
                    println!("[labnas-viewer] certificado de {host} guardado: {fp}");
                    save_pin(&host, &fp);
                }
                if let Some(ctx) = view.context() {
                    ctx.allow_tls_certificate_for_host(cert, &host);
                }
                view.load_uri(uri);
            }
        }
        true
    });
}

/// Icono de LabNAS (PNG embebido) como RGBA
fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("../assets/tray.png").as_slice()));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    (info.color_type == png::ColorType::Rgba).then_some((buf, info.width, info.height))
}

/// Icono en la bandeja con "Abrir" y "Salir". None si el escritorio no tiene bandeja.
fn build_tray(proxy: &tao::event_loop::EventLoopProxy<UserEvent>) -> Option<tray_icon::TrayIcon> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    let (rgba, w, h) = icon_rgba()?;
    let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;
    let open = MenuItem::new("Abrir LabNAS", true, None);
    let quit = MenuItem::new("Salir", true, None);
    let menu = Menu::new();
    menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).ok()?;
    let (open_id, quit_id) = (open.id().clone(), quit.id().clone());

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let ev = if e.id == quit_id { UserEvent::Quit } else if e.id == open_id { UserEvent::Show } else { return };
        let _ = menu_proxy.send_event(ev);
    }));
    let click_proxy = proxy.clone();
    tray_icon::TrayIconEvent::set_event_handler(Some(move |e: tray_icon::TrayIconEvent| {
        if matches!(e, tray_icon::TrayIconEvent::Click { .. } | tray_icon::TrayIconEvent::DoubleClick { .. }) {
            let _ = click_proxy.send_event(UserEvent::Show);
        }
    }));

    match tray_icon::TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("LabNAS")
        .with_menu(Box::new(menu))
        .build()
    {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("[labnas-viewer] sin bandeja del sistema: {e}");
            None
        }
    }
}

fn resolve_url() -> String {
    if let Some(arg) = std::env::args().nth(1) {
        return arg;
    }
    if let Ok(env) = std::env::var("LABNAS_URL")
        && !env.trim().is_empty()
    {
        return env.trim().to_string();
    }
    if let Some(cfg) = dirs::config_dir().map(|d| d.join("labnas-viewer/url"))
        && let Ok(content) = std::fs::read_to_string(cfg)
        && let Some(line) = content.lines().map(str::trim).find(|l| !l.is_empty())
    {
        return line.to_string();
    }
    if let Some(url) = discover_labnas() {
        println!("[labnas-viewer] LabNAS encontrado por mDNS: {url}");
        return url;
    }
    DEFAULT_URL.to_string()
}

/// Host de una URL (sin puerto): HTTP y HTTPS del mismo LabNAS cuentan como propios.
fn url_host(url: &str) -> Option<String> {
    let uri: Uri = url.parse().ok()?;
    uri.host().map(str::to_string)
}

/// host:port del servidor, para comprobar si ya acepta conexiones.
fn server_addr(url: &str) -> Option<String> {
    let uri: Uri = url.parse().ok()?;
    let host = uri.host()?;
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("https") { 443 } else { 80 });
    Some(format!("{host}:{port}"))
}

fn server_up(addr: &str) -> bool {
    addr.to_socket_addrs()
        .map(|mut addrs| {
            addrs.any(|a| TcpStream::connect_timeout(&a, Duration::from_millis(800)).is_ok())
        })
        .unwrap_or(false)
}

fn loading_page(url: &str) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>
body{{margin:0;height:100vh;display:flex;flex-direction:column;align-items:center;justify-content:center;
background:#282a36;color:#f8f8f2;font-family:system-ui,sans-serif}}
.spin{{width:42px;height:42px;border:4px solid #44475a;border-top-color:#bd93f9;border-radius:50%;
animation:s 1s linear infinite;margin-bottom:20px}}@keyframes s{{to{{transform:rotate(360deg)}}}}
code{{color:#8be9fd}}small{{color:#6272a4;margin-top:8px}}
</style></head><body><div class="spin"></div>
<div>Esperando al servidor LabNAS…</div><small><code>{url}</code></small></body></html>"#
    )
}

/// Abre enlaces externos (target="_blank", window.open) en la app por defecto del sistema.
fn open_external(url: &str) {
    if let Err(e) = Command::new("xdg-open").arg(url).spawn() {
        eprintln!("[labnas-viewer] no se pudo abrir {url}: {e}");
    }
}

fn notify(icon: &str, title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "LabNAS", "-i", icon, title, body])
        .spawn();
}

/// Mensajes que manda la web UI por `window.ipc.postMessage`
fn handle_ipc(body: &str, proxy: &tao::event_loop::EventLoopProxy<UserEvent>) {
    if body == "quit" {
        let _ = proxy.send_event(UserEvent::Quit);
        return;
    }
    // Notificaciones del bus de eventos (impresion terminada, respaldo fallido...)
    let Ok(msg) = serde_json::from_str::<serde_json::Value>(body) else { return };
    if msg["type"] == "notify" {
        let title = msg["title"].as_str().unwrap_or("LabNAS");
        let text = msg["body"].as_str().unwrap_or("");
        notify("dialog-information", title, text);
    }
}

// Atajos: F5 recarga, Ctrl+Q cierra.
const INIT_SCRIPT: &str = r#"
addEventListener('keydown', (e) => {
  if (e.key === 'F5') { e.preventDefault(); location.reload(); }
  else if (e.ctrlKey && (e.key === 'q' || e.key === 'Q')) { e.preventDefault(); window.ipc.postMessage('quit'); }
}, true);
"#;

fn main() -> wry::Result<()> {
    // Diagnostico: buscar LabNAS en la red y salir
    if std::env::args().nth(1).as_deref() == Some("--buscar") {
        match discover_labnas() {
            Some(url) => println!("{url}"),
            None => {
                eprintln!("No se encontro LabNAS por mDNS (¿mDNS activado en Configuracion > Red?)");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--olvidar") {
        let Some(host) = std::env::args().nth(2) else {
            eprintln!("Uso: labnas-viewer --olvidar <host>");
            std::process::exit(2);
        };
        if let Some(path) = pins_path() {
            let pins = read_pins();
            let rest = forget_pin(&pins, &host);
            if rest == pins {
                println!("No habia huella guardada para {host}");
            } else if let Err(e) = std::fs::write(&path, rest) {
                eprintln!("No se pudo escribir {}: {e}", path.display());
                std::process::exit(1);
            } else {
                println!("Huella de {host} olvidada; la proxima conexion guarda la nueva");
            }
        }
        return Ok(());
    }

    let url = resolve_url();
    let addr = server_addr(&url);

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event()
        .with_app_id(APP_ID)
        .build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("LabNAS")
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .with_min_inner_size(LogicalSize::new(480.0, 360.0))
        .with_window_icon(icon_rgba().and_then(|(rgba, w, h)| tao::window::Icon::from_rgba(rgba, w, h).ok()))
        .build(&event_loop)
        .expect("no se pudo crear la ventana");

    // Debe crearse en el hilo del event loop (gtk); se mantiene vivo hasta salir
    let tray = build_tray(&proxy);

    // Datos persistentes (localStorage con la sesión, cookies, caché)
    let data_dir: Option<PathBuf> = dirs::data_dir().map(|d| d.join("labnas-viewer"));
    let mut web_context = WebContext::new(data_dir);

    let origin = url_host(&url);
    let ipc_proxy = proxy.clone();

    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
        // `tls-pin`: el servidor puede mandarnos a HTTPS (ver pin_certificates)
        .with_user_agent(format!(
            "Mozilla/5.0 (X11; Linux) AppleWebKit/605.1.15 LabNAS-Viewer/{} {VIEWER_TLS}",
            env!("CARGO_PKG_VERSION")
        ))
        .with_html(loading_page(&url))
        .with_initialization_script(INIT_SCRIPT)
        .with_devtools(cfg!(debug_assertions))
        .with_ipc_handler(move |req| handle_ipc(req.body(), &ipc_proxy))
        .with_new_window_req_handler(|target, _features| {
            open_external(&target);
            NewWindowResponse::Deny
        })
        // Navegación a otro host (enlaces sin target) → navegador del sistema
        .with_navigation_handler(move |target| {
            let same_origin = origin.is_some() && url_host(&target) == origin;
            let internal = target.starts_with("about:")
                || target.starts_with("data:")
                || target.starts_with("blob:");
            if same_origin || internal {
                true
            } else {
                open_external(&target);
                false
            }
        })
        // wry ya propone ~/Descargas/<nombre> sin sobrescribir; aceptamos tal cual
        .with_download_started_handler(|_uri, _dest| true)
        .with_download_completed_handler(|_uri, path, ok| match (ok, path) {
            (true, Some(p)) => notify("folder-download", "LabNAS", &format!(
                "Descargado: {}",
                p.file_name().map(|f| f.to_string_lossy()).unwrap_or_default()
            )),
            _ => notify("dialog-error", "LabNAS", "La descarga falló"),
        });

    let vbox = window.default_vbox().expect("ventana sin gtk::Box");
    let webview = builder.build_gtk(vbox)?;
    pin_certificates(&webview);

    // Espera en segundo plano a que el servidor acepte conexiones
    std::thread::spawn(move || {
        if let Some(addr) = addr {
            while !server_up(&addr) {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        let _ = proxy.send_event(UserEvent::ServerReady);
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UserEvent::ServerReady) => {
                if let Err(e) = webview.load_url(&url) {
                    eprintln!("[labnas-viewer] error cargando {url}: {e}");
                }
            }
            Event::UserEvent(UserEvent::Show) => {
                window.set_visible(true);
                window.set_focus();
            }
            // Con bandeja, cerrar solo oculta (siguen llegando notificaciones)
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } if tray.is_some() => {
                window.set_visible(false);
            }
            Event::UserEvent(UserEvent::Quit)
            | Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn huellas_tofu() {
        let pins = "# comentario\n10.0.0.5 AA:BB\nnas.local CC:DD\n";
        assert_eq!(check_pin(pins, "10.0.0.5", "aa:bb"), PinCheck::Trusted);
        assert_eq!(check_pin(pins, "nas.local", "EE:FF"), PinCheck::Changed("CC:DD".into()));
        assert_eq!(check_pin(pins, "10.0.0.6", "AA:BB"), PinCheck::New);
        assert_eq!(forget_pin(pins, "10.0.0.5"), "# comentario\nnas.local CC:DD\n");
    }

    #[test]
    fn huella_como_la_muestra_labnas() {
        let fp = fingerprint(b"abc");
        assert_eq!(fp.len(), 32 * 3 - 1);
        assert!(fp.starts_with("BA:78:16:BF"), "{fp}");
    }

    #[test]
    fn mismo_host_en_http_y_https() {
        assert_eq!(url_host("http://10.0.0.5:3001/"), url_host("https://10.0.0.5:3443/x"));
        assert_ne!(url_host("http://10.0.0.5:3001/"), url_host("https://evil.com/"));
    }
}
