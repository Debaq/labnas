//! /cmd: terminal remota por Telegram

use super::*;

// =====================
// Command responses
// =====================

// =====================
// Remote terminal via Telegram
// =====================

pub(super) async fn handle_cmd(state: &AppState, chat_id: i64, user: &str, text: &str) -> String {
    use tokio::io::AsyncBufReadExt;
    use tokio::process::Command as TokioCmd;
    use std::process::Stdio;

    let cmd = text.strip_prefix("/cmd ").unwrap_or("").trim();
    if cmd.is_empty() {
        return "Uso: `/cmd <comando>`\nEj: `/cmd df -h`\n`/cmd sudo pacman -Syu`\n\nSi pide input, envia texto normal (sin /).\n`/kill` para terminar proceso.".to_string();
    }

    // Kill existing session if any
    {
        let mut terms = state.tg_terminals.lock().await;
        if let Some(mut old) = terms.remove(&chat_id) {
            let _ = old.child.kill().await;
        }
    }

    state.log_activity("Terminal TG", cmd, user).await;

    // Nunca como root: si el servicio corre como root, bajar al usuario de la sesion
    // (igual que la terminal web)
    let mut command = match (crate::config::is_root(), crate::config::detect_session_user()) {
        (true, Some(session_user)) => {
            let mut c = TokioCmd::new("su");
            c.args(["-", &session_user, "-c", cmd]);
            c
        }
        (true, None) => {
            return "Terminal deshabilitada: LabNAS corre como root y no hay un usuario de sesion al cual bajar.".to_string();
        }
        (false, _) => {
            let mut c = TokioCmd::new("bash");
            c.args(["-c", cmd]);
            c
        }
    };

    // Spawn process with piped I/O
    let child_result = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child_result {
        Ok(c) => c,
        Err(e) => return format!("Error: {}", e),
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let stdin = child.stdin.take().unwrap();

    // Channel for output
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(100);

    // Read stdout
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx2.send(line).await.is_err() { break; }
        }
    });

    // Read stderr
    tokio::spawn(async move {
        let reader = tokio::io::BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(format!("[err] {}", line)).await.is_err() { break; }
        }
    });

    // Wait a bit for initial output
    let output = collect_output(rx, child, stdin, state, chat_id, cmd).await;
    output
}

pub(super) async fn collect_output(
    mut rx: tokio::sync::mpsc::Receiver<String>,
    mut child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    state: &AppState,
    chat_id: i64,
    cmd: &str,
) -> String {
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    loop {
        // Si el proceso ya termino, usar un timeout corto para drenar output restante
        let wait_until = if child.try_wait().map(|s| s.is_some()).unwrap_or(false) {
            std::cmp::min(deadline, tokio::time::Instant::now() + Duration::from_millis(100))
        } else {
            deadline
        };
        let timeout = tokio::time::timeout_at(wait_until, rx.recv()).await;
        match timeout {
            Ok(Some(line)) => lines.push(line),
            Ok(None) => break,  // Canal cerrado, proceso termino
            Err(_) => break,    // Timeout
        }
    }

    // Check if process is still running
    let mut terms = state.tg_terminals.lock().await;
    let still_alive = child.try_wait().map(|s| s.is_none()).unwrap_or(false);

    let mut msg = format!("$ `{}`\n", cmd);
    if !lines.is_empty() {
        let output = lines.join("\n");
        let output = if output.len() > 3500 {
            format!("{}...(truncado)", &output[..3500])
        } else {
            output
        };
        msg.push_str(&format!("```\n{}```", output));
    }

    if still_alive {
        // Process waiting for input - save session
        terms.insert(chat_id, crate::state::TgTerminal {
            stdin,
            output_rx: rx,
            child,
            created_at: std::time::Instant::now(),
        });
        msg.push_str("\n_Proceso activo. Envia texto para input o /kill para terminar._");
    } else {
        let code = child.wait().await.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        if lines.is_empty() {
            msg.push_str("_(sin salida)_");
        }
        if code != 0 {
            msg.push_str(&format!("\nExit: {}", code));
        }
    }

    msg
}

pub(super) async fn pipe_terminal_input(state: &AppState, chat_id: i64, input: &str) -> Option<String> {
    use tokio::io::AsyncWriteExt;

    let mut terms = state.tg_terminals.lock().await;
    let session = terms.get_mut(&chat_id)?;

    // Check timeout (5 min)
    if session.created_at.elapsed().as_secs() > 300 {
        let mut session = terms.remove(&chat_id).unwrap();
        let _ = session.child.kill().await;
        return Some("Sesion expirada (5 min).".to_string());
    }

    // Write input to stdin
    let write_result = session.stdin.write_all(format!("{}\n", input).as_bytes()).await;
    if write_result.is_err() {
        let mut session = terms.remove(&chat_id).unwrap();
        let _ = session.child.kill().await;
        return Some("Proceso termino.".to_string());
    }

    // Collect output
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    loop {
        let timeout = tokio::time::timeout_at(deadline, session.output_rx.recv()).await;
        match timeout {
            Ok(Some(line)) => lines.push(line),
            _ => break,
        }
    }

    // Check if still alive
    let still_alive = session.child.try_wait().map(|s| s.is_none()).unwrap_or(false);

    let mut msg = String::new();
    if !lines.is_empty() {
        let output = lines.join("\n");
        let output = if output.len() > 3500 {
            format!("{}...(truncado)", &output[..3500])
        } else {
            output
        };
        msg.push_str(&format!("```\n{}```", output));
    }

    if !still_alive {
        let mut session = terms.remove(&chat_id).unwrap();
        let code = session.child.wait().await.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        if code != 0 {
            msg.push_str(&format!("\nExit: {}", code));
        }
        if msg.is_empty() {
            msg = "Proceso termino.".to_string();
        }
    }

    if msg.is_empty() {
        msg = "_Esperando..._".to_string();
    }

    Some(msg)
}
