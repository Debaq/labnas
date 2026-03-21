mod config;
mod handlers;
mod middleware;
mod models;
mod state;

use axum::{
    http::Method,
    middleware as axum_mw,
    routing::{delete, get, post, put},
    Router,
};
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::{Mutex, Notify};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

use config::load_config;
use state::AppState;

#[tokio::main]
async fn main() {
    let config = load_config().await;
    let mdns_enabled = config.mdns_enabled;
    let mdns_hostname = if config.mdns_hostname.is_empty() { "labnas".to_string() } else { config.mdns_hostname.clone() };
    let shutdown = Arc::new(Notify::new());

    let state = AppState {
        scanned_hosts: Arc::new(Mutex::new(Vec::new())),
        start_time: Instant::now(),
        config: Arc::new(Mutex::new(config)),
        http_client: reqwest::Client::new(),
        shutdown: shutdown.clone(),
        activity_log: Arc::new(Mutex::new(Vec::new())),
        sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        link_codes: Arc::new(Mutex::new(std::collections::HashMap::new())),
        share_links: Arc::new(Mutex::new(std::collections::HashMap::new())),
        tg_terminals: Arc::new(Mutex::new(std::collections::HashMap::new())),
        email_inbox: Arc::new(Mutex::new(std::collections::HashMap::new())),
        mdns_service: Arc::new(Mutex::new(None)),
    };

    // Start mDNS if enabled
    if mdns_enabled {
        let hostname = mdns_hostname.clone();
        match handlers::system::start_mdns_service(&hostname) {
            Ok(svc) => {
                println!("[mDNS] Activo: http://{}.local:3001", hostname);
                *state.mdns_service.lock().await = Some(svc);
            }
            Err(e) => eprintln!("[mDNS] Error: {}", e),
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any);

    let api = Router::new()
        // Auth
        .route("/api/auth/has-users", get(handlers::auth::has_users))
        .route("/api/auth/register", post(handlers::auth::register))
        .route("/api/auth/login", post(handlers::auth::login))
        .route("/api/auth/me", get(handlers::auth::me))
        .route("/api/auth/logout", post(handlers::auth::logout))
        .route("/api/auth/password", post(handlers::auth::change_password))
        .route("/api/auth/users", get(handlers::auth::list_users))
        .route("/api/auth/users/{username}/role", post(handlers::auth::set_user_role))
        .route("/api/auth/users/{username}", delete(handlers::auth::delete_user))
        .route("/api/auth/link-code", post(handlers::auth::generate_link_code))
        .route("/api/notifications/telegram/chat/{chat_id}/link", post(handlers::auth::admin_link_chat))
        // Health
        .route("/api/health", get(handlers::system::health_handler))
        // Files
        .route("/api/files", get(handlers::files::list_files))
        .route("/api/files", delete(handlers::files::delete_file))
        .route("/api/files/upload", post(handlers::files::upload_file))
        .route("/api/files/download", get(handlers::files::download_file))
        .route("/api/files/directory", post(handlers::files::create_directory))
        .route("/api/files/quickaccess", get(handlers::files::quick_access))
        // Storage & System
        .route("/api/storage", get(handlers::system::storage_info))
        .route("/api/system/disks", get(handlers::system::system_disks))
        .route("/api/system/info", get(handlers::system::system_info_handler))
        .route("/api/system/shutdown", post(handlers::system::shutdown_handler))
        .route("/api/system/autostart", get(handlers::system::autostart_status))
        .route("/api/system/update/check", get(handlers::system::check_update))
        .route("/api/system/update/do", post(handlers::system::do_update))
        .route("/api/system/branding", get(handlers::system::get_branding))
        .route("/api/system/branding", post(handlers::system::set_branding))
        .route("/api/system/mdns", get(handlers::system::get_mdns_status))
        .route("/api/system/mdns", post(handlers::system::set_mdns))
        // Network
        .route("/api/network/scan", post(handlers::network::scan_network))
        .route("/api/network/hosts", get(handlers::network::get_hosts))
        .route("/api/network/device/{mac}", post(handlers::network::label_host))
        .route("/api/network/device/{mac}", delete(handlers::network::unlabel_host))
        // Terminal
        .route("/api/terminal", get(handlers::terminal::terminal_handler))
        // Printers 3D
        .route("/api/printers3d", get(handlers::printers3d::list_printers))
        .route("/api/printers3d", post(handlers::printers3d::add_printer))
        .route("/api/printers3d/detect", post(handlers::printers3d::detect_printers))
        .route("/api/printers3d/{id}", delete(handlers::printers3d::delete_printer))
        .route("/api/printers3d/{id}/status", get(handlers::printers3d::printer_status))
        .route("/api/printers3d/{id}/upload", post(handlers::printers3d::upload_gcode))
        .route("/api/printers3d/{id}/control", post(handlers::printers3d::control_print))
        .route("/api/printers3d/{id}/preheat", post(handlers::printers3d::preheat))
        .route("/api/printers3d/{id}/home", post(handlers::printers3d::home_axes))
        .route("/api/printers3d/{id}/jog", post(handlers::printers3d::jog))
        .route("/api/printers3d/{id}/gcode", post(handlers::printers3d::send_gcode))
        .route("/api/printers3d/{id}/files", get(handlers::printers3d::list_printer_files))
        .route("/api/printers3d/{id}/files/{filename}/print", post(handlers::printers3d::print_file))
        .route("/api/printers3d/{id}/files/{filename}", delete(handlers::printers3d::delete_printer_file))
        .route("/api/printers3d/{id}/camera", get(handlers::printers3d::camera_snapshot))
        // CUPS Printing
        .route("/api/printing/printers", get(handlers::printing::list_printers))
        .route("/api/printing/printers/{name}/options", get(handlers::printing::printer_options))
        .route("/api/printing/printers/{name}/enable", post(handlers::printing::enable_printer))
        .route("/api/printing/printers/{name}/disable", post(handlers::printing::disable_printer))
        .route("/api/printing/print", post(handlers::printing::print_upload))
        .route("/api/printing/print-file", post(handlers::printing::print_file_path))
        .route("/api/printing/jobs", get(handlers::printing::list_jobs))
        .route("/api/printing/jobs/{id}", delete(handlers::printing::cancel_job))
        // Notifications (Telegram)
        .route("/api/notifications/telegram", get(handlers::notifications::get_config))
        .route("/api/notifications/telegram/token", post(handlers::notifications::set_bot_token))
        .route("/api/notifications/telegram/token", delete(handlers::notifications::delete_bot_token))
        .route("/api/notifications/telegram/chat/{chat_id}", delete(handlers::notifications::delete_chat))
        .route("/api/notifications/telegram/chat/{chat_id}/role", post(handlers::notifications::set_chat_role))
        .route("/api/notifications/telegram/test", post(handlers::notifications::send_test))
        .route("/api/notifications/schedule", post(handlers::notifications::set_schedule))
        // Tasks & Projects
        .route("/api/projects", get(handlers::tasks::list_projects))
        .route("/api/projects", post(handlers::tasks::create_project))
        .route("/api/projects/{id}", delete(handlers::tasks::delete_project))
        .route("/api/tasks", get(handlers::tasks::list_tasks))
        .route("/api/tasks", post(handlers::tasks::create_task))
        .route("/api/tasks/{id}", put(handlers::tasks::update_task))
        .route("/api/tasks/{id}", delete(handlers::tasks::delete_task))
        .route("/api/tasks/{id}/confirm", post(handlers::tasks::confirm_task))
        .route("/api/tasks/{id}/reject", post(handlers::tasks::reject_task))
        .route("/api/tasks/{id}/done", post(handlers::tasks::done_task))
        // Calendar Events
        .route("/api/events", get(handlers::tasks::list_events))
        .route("/api/events", post(handlers::tasks::create_event))
        .route("/api/events/{id}", delete(handlers::tasks::delete_event))
        .route("/api/events/{id}/accept", post(handlers::tasks::accept_event))
        .route("/api/events/{id}/decline", post(handlers::tasks::decline_event))
        // File sharing
        .route("/api/shares", get(handlers::extras::list_shares))
        .route("/api/shares", post(handlers::extras::create_share))
        .route("/api/shares/{token}", delete(handlers::extras::delete_share))
        .route("/api/share/{token}", get(handlers::extras::download_share))
        // Download from URL
        .route("/api/download-url", post(handlers::extras::download_url))
        // Notes
        .route("/api/notes", get(handlers::extras::list_notes))
        .route("/api/notes", post(handlers::extras::create_note))
        .route("/api/notes/{id}", put(handlers::extras::update_note))
        .route("/api/notes/{id}", delete(handlers::extras::delete_note))
        // Email / IMAP
        .route("/api/email/account", post(handlers::email::configure_account))
        .route("/api/email/account", delete(handlers::email::delete_account))
        .route("/api/email/inbox", get(handlers::email::get_inbox))
        .route("/api/email/check", post(handlers::email::check_now))
        .route("/api/email/classify/{uid}", post(handlers::email::classify_email))
        .route("/api/email/to-task/{uid}", post(handlers::email::email_to_task))
        .route("/api/email/groq-key", post(handlers::email::set_groq_key))
        .route("/api/email/filters", get(handlers::email::list_filters))
        .route("/api/email/filters", post(handlers::email::add_filter))
        .route("/api/email/filters/{pattern}", delete(handlers::email::delete_filter))
        .layer(axum_mw::from_fn_with_state(state.clone(), middleware::permission_check))
        .layer(cors)
        .with_state(state.clone());

    // Spawn Telegram bot polling loop + daily scheduler + task reminders + email check
    tokio::spawn(handlers::notifications::telegram_bot_loop(state.clone()));
    tokio::spawn(handlers::notifications::task_reminder_loop(state.clone()));
    tokio::spawn(handlers::email::email_check_loop(state.clone()));
    tokio::spawn(handlers::notifications::daily_notification_loop(state.clone()));
    tokio::spawn(handlers::printers3d::printer_monitor_loop(state.clone()));
    tokio::spawn(handlers::system::update_check_loop(state));

    // Static files
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let static_dir = std::env::var("LABNAS_STATIC")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            let candidates = [
                exe_dir.join("dist"),
                exe_dir.join("../frontend/dist"),
                PathBuf::from("../frontend/dist"),
                PathBuf::from("frontend/dist"),
            ];
            candidates
                .into_iter()
                .find(|p| p.join("index.html").exists())
        });

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));

    let app = if let Some(static_path) = static_dir {
        let static_path = std::fs::canonicalize(&static_path).unwrap_or(static_path);
        println!("Sirviendo frontend desde: {}", static_path.display());
        let index = static_path.join("index.html");
        let serve_dir = ServeDir::new(&static_path).not_found_service(ServeFile::new(&index));
        api.fallback_service(serve_dir)
    } else {
        println!("No se encontro directorio de frontend estatico.");
        println!("  Usa LABNAS_STATIC=/ruta/a/dist o ejecuta en modo desarrollo.");
        api
    };

    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "0.0.0.0".to_string());

    // Check firewall
    check_firewall().await;

    // Check Tailscale
    let tailscale_ip = check_tailscale().await;

    println!("LabNAS corriendo en:");
    println!("  Local:  http://localhost:3001");
    println!("  Red:    http://{}:3001", local_ip);

    // Try to also listen on port 80 (requires root/sudo)
    let has_port_80 = match tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 80))).await {
        Ok(listener_80) => {
            let app_80 = app.clone();
            let shutdown_80 = shutdown.clone();
            tokio::spawn(async move {
                axum::serve(listener_80, app_80)
                    .with_graceful_shutdown(async move { shutdown_80.notified().await; })
                    .await
                    .ok();
            });
            println!("  \x1b[32mWeb:    http://{}\x1b[0m (puerto 80)", local_ip);
            true
        }
        Err(_) => {
            println!("  \x1b[33m(Puerto 80 no disponible - ejecuta con sudo para habilitarlo)\x1b[0m");
            false
        }
    };

    if mdns_enabled {
        if has_port_80 {
            println!("  \x1b[32mLocal:  http://{}.local\x1b[0m", mdns_hostname);
        } else {
            println!("  Local:  http://{}.local:3001", mdns_hostname);
        }
    }
    if let Some(ref ts_ip) = tailscale_ip {
        println!("  \x1b[32mRemoto: http://{}:3001 (Tailscale)\x1b[0m", ts_ip);
    }

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.notified().await;
            println!("LabNAS apagandose...");
        })
        .await
        .unwrap();
}

async fn check_tailscale() -> Option<String> {
    let output = tokio::process::Command::new("tailscale")
        .args(["ip", "-4"])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if ip.is_empty() {
        println!("\n  \x1b[33mTailscale instalado pero no conectado.\x1b[0m");
        println!("  \x1b[36mEjecuta: sudo tailscale up\x1b[0m\n");
        return None;
    }

    println!("\n  \x1b[32m✓ Tailscale activo: {}\x1b[0m", ip);
    Some(ip)
}

async fn check_firewall() {
    // Check if ufw is active
    let Ok(output) = tokio::process::Command::new("ufw")
        .arg("status")
        .output()
        .await
    else {
        return; // ufw not installed, no problem
    };

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if !text.contains("Status: active") {
        return; // ufw inactive
    }

    let needs_3001 = !text.contains("3001");
    let needs_80 = !text.contains("80/tcp") && !text.contains(" 80 ");

    if !needs_3001 && !needs_80 {
        return;
    }

    println!("\n  \x1b[33m⚠ FIREWALL: ufw esta activo, abriendo puertos necesarios...\x1b[0m");

    // Try to open automatically if running as root
    if std::env::var("USER").unwrap_or_default() == "root"
        || std::env::var("SUDO_USER").is_ok()
    {
        if needs_3001 {
            let result = tokio::process::Command::new("ufw")
                .args(["allow", "3001"])
                .output()
                .await;
            match result {
                Ok(out) if out.status.success() => println!("  \x1b[32m✓ Puerto 3001 abierto\x1b[0m"),
                _ => println!("  \x1b[31m✗ No se pudo abrir 3001\x1b[0m"),
            }
        }
        if needs_80 {
            let result = tokio::process::Command::new("ufw")
                .args(["allow", "80"])
                .output()
                .await;
            match result {
                Ok(out) if out.status.success() => println!("  \x1b[32m✓ Puerto 80 abierto\x1b[0m"),
                _ => println!("  \x1b[31m✗ No se pudo abrir 80\x1b[0m"),
            }
        }
        println!();
        return;
    }

    println!("  \x1b[36m  Ejecuta: sudo ufw allow 3001 && sudo ufw allow 80\x1b[0m\n");
}
