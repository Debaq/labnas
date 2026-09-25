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

use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::unix::{EventLoopBuilderExtUnix, WindowExtUnix};
use tao::window::WindowBuilder;
use wry::http::Uri;
use wry::{NewWindowResponse, WebContext, WebViewBuilder, WebViewBuilderExtUnix};

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

    let origin = server_addr(&url);
    let ipc_proxy = proxy.clone();

    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
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
            let same_origin = origin.is_some() && server_addr(&target) == origin;
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
