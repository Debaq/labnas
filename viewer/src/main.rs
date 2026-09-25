//! LabNAS Viewer: ventana nativa (WebKitGTK via wry) que carga la web UI de LabNAS
//! sin depender de un navegador instalado.
//!
//! URL a cargar (primera que exista):
//!   1. argumento CLI:            labnas-viewer http://192.168.1.10:3001
//!   2. variable de entorno:      LABNAS_URL
//!   3. archivo de config:        ~/.config/labnas-viewer/url
//!   4. por defecto:              http://localhost:3001

use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

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
    Quit,
}

fn resolve_url() -> String {
    if let Some(arg) = std::env::args().nth(1) {
        return arg;
    }
    if let Ok(env) = std::env::var("LABNAS_URL") {
        if !env.trim().is_empty() {
            return env.trim().to_string();
        }
    }
    if let Some(cfg) = dirs::config_dir().map(|d| d.join("labnas-viewer/url")) {
        if let Ok(content) = std::fs::read_to_string(cfg) {
            if let Some(line) = content.lines().map(str::trim).find(|l| !l.is_empty()) {
                return line.to_string();
            }
        }
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

fn notify(body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "LabNAS", "-i", "folder-download", "LabNAS", body])
        .spawn();
}

// Atajos: F5 recarga, Ctrl+Q cierra.
const INIT_SCRIPT: &str = r#"
addEventListener('keydown', (e) => {
  if (e.key === 'F5') { e.preventDefault(); location.reload(); }
  else if (e.ctrlKey && (e.key === 'q' || e.key === 'Q')) { e.preventDefault(); window.ipc.postMessage('quit'); }
}, true);
"#;

fn main() -> wry::Result<()> {
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
        .build(&event_loop)
        .expect("no se pudo crear la ventana");

    // Datos persistentes (localStorage con la sesión, cookies, caché)
    let data_dir: Option<PathBuf> = dirs::data_dir().map(|d| d.join("labnas-viewer"));
    let mut web_context = WebContext::new(data_dir);

    let origin = server_addr(&url);
    let ipc_proxy = proxy.clone();

    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_html(loading_page(&url))
        .with_initialization_script(INIT_SCRIPT)
        .with_devtools(cfg!(debug_assertions))
        .with_ipc_handler(move |req| {
            if req.body() == "quit" {
                let _ = ipc_proxy.send_event(UserEvent::Quit);
            }
        })
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
            (true, Some(p)) => notify(&format!(
                "Descargado: {}",
                p.file_name().map(|f| f.to_string_lossy()).unwrap_or_default()
            )),
            _ => notify("La descarga falló"),
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
            Event::UserEvent(UserEvent::Quit)
            | Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}
