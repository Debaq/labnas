mod config;
mod handlers;
mod models;
mod state;

use axum::{
    http::Method,
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
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any);

    let api = Router::new()
        // Auth
        .route("/api/auth/register", post(handlers::auth::register))
        .route("/api/auth/login", post(handlers::auth::login))
        .route("/api/auth/me", get(handlers::auth::me))
        .route("/api/auth/logout", post(handlers::auth::logout))
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
        .layer(cors)
        .with_state(state.clone());

    // Spawn Telegram bot polling loop + daily scheduler + task reminders
    tokio::spawn(handlers::notifications::telegram_bot_loop(state.clone()));
    tokio::spawn(handlers::notifications::task_reminder_loop(state.clone()));
    tokio::spawn(handlers::notifications::daily_notification_loop(state));

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

    println!("LabNAS corriendo en:");
    println!("  Local:  http://localhost:3001");
    println!("  Red:    http://{}:3001", local_ip);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.notified().await;
            println!("LabNAS apagandose...");
        })
        .await
        .unwrap();
}
