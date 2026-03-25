use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
struct ResizeMessage {
    cols: u16,
    rows: u16,
}

/// Detecta el usuario con sesión activa (no root)
fn detect_session_user() -> Option<String> {
    let output = std::process::Command::new("who").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("(:0)") || line.contains("tty") {
            let user = line.split_whitespace().next()?;
            if user != "root" {
                return Some(user.to_string());
            }
        }
    }
    None
}

pub async fn terminal_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_terminal_socket)
}

async fn handle_terminal_socket(socket: WebSocket) {
    let pty_system = native_pty_system();

    let pty_pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("Error abriendo PTY: {}", e);
            return;
        }
    };

    // Detectar el usuario real de la sesión (no root)
    let real_user = detect_session_user();
    let (user_name, user_home, user_shell) = if let Some(ref user) = real_user {
        let home = format!("/home/{}", user);
        // Leer shell del usuario desde /etc/passwd
        let shell = std::fs::read_to_string("/etc/passwd").ok()
            .and_then(|passwd| {
                passwd.lines()
                    .find(|l| l.starts_with(&format!("{}:", user)))
                    .and_then(|l| l.rsplit(':').next())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "/bin/bash".to_string());
        (user.clone(), home, shell)
    } else {
        (
            std::env::var("USER").unwrap_or_else(|_| "root".to_string()),
            std::env::var("HOME").unwrap_or_else(|_| "/root".to_string()),
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
        )
    };

    // Usar su para lanzar el shell como el usuario real
    let mut cmd = if real_user.is_some() && user_name != "root" {
        let mut c = CommandBuilder::new("su");
        c.arg("-");
        c.arg(&user_name);
        c
    } else {
        let mut c = CommandBuilder::new(&user_shell);
        c.arg("-l");
        c
    };
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("HOME", &user_home);
    cmd.env("USER", &user_name);
    cmd.env("SHELL", &user_shell);
    cmd.env(
        "LANG",
        std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".to_string()),
    );
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.cwd(&user_home);

    let mut child = match pty_pair.slave.spawn_command(cmd) {
        Ok(child) => child,
        Err(e) => {
            eprintln!("Error spawning shell: {}", e);
            return;
        }
    };

    drop(pty_pair.slave);

    let reader = match pty_pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error cloning PTY reader: {}", e);
            return;
        }
    };

    let writer = match pty_pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Error taking PTY writer: {}", e);
            return;
        }
    };

    let master: Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>> =
        Arc::new(std::sync::Mutex::new(pty_pair.master));

    let (mut ws_sender, mut ws_receiver) = socket.split();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

    let pty_read_handle = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let send_handle = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if ws_sender
                .send(Message::Binary(data.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let writer = Arc::new(std::sync::Mutex::new(writer));

    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                let text_str: &str = &text;
                if text_str.starts_with('\x01') {
                    if let Ok(size) = serde_json::from_str::<ResizeMessage>(&text_str[1..]) {
                        let master_clone = Arc::clone(&master);
                        let _ = tokio::task::spawn_blocking(move || {
                            if let Ok(m) = master_clone.lock() {
                                let _ = m.resize(PtySize {
                                    rows: size.rows,
                                    cols: size.cols,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                });
                            }
                        })
                        .await;
                    }
                } else {
                    let writer_clone = Arc::clone(&writer);
                    let data = text_str.as_bytes().to_vec();
                    let _ = tokio::task::spawn_blocking(move || {
                        use std::io::Write;
                        if let Ok(mut w) = writer_clone.lock() {
                            let _ = w.write_all(&data);
                            let _ = w.flush();
                        }
                    })
                    .await;
                }
            }
            Message::Binary(data) => {
                let writer_clone = Arc::clone(&writer);
                let data = data.to_vec();
                let _ = tokio::task::spawn_blocking(move || {
                    use std::io::Write;
                    if let Ok(mut w) = writer_clone.lock() {
                        let _ = w.write_all(&data);
                        let _ = w.flush();
                    }
                })
                .await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_handle.abort();
    pty_read_handle.abort();
    let _ = child.kill();
}
