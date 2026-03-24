use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

use crate::config::save_config;
use crate::models::email::{EmailAccount, EmailFilter, EmailMessage, FilterAction, MailProtocol};
use crate::models::notifications::UserRole;
use crate::state::AppState;

/// Extrae el username de la sesion a partir del header Authorization
fn extract_username(
    sessions: &HashMap<String, crate::state::SessionInfo>,
    headers: &HeaderMap,
) -> Option<(String, UserRole)> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())?;
    let session = sessions.get(&token)?;
    Some((session.username.clone(), session.role.clone()))
}

// =====================
// Request types
// =====================

#[derive(Debug, Deserialize)]
pub struct ConfigureAccountRequest {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub protocol: crate::models::email::MailProtocol,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SetGroqKeyRequest {
    pub key: String,
}

// =====================
// API Handlers
// =====================

/// POST /api/email/account - Configurar cuenta IMAP
pub async fn configure_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ConfigureAccountRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    if req.host.is_empty() || req.email.is_empty() || req.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Host, email y password son requeridos".to_string(),
        ));
    }

    let protocol_label = match req.protocol {
        MailProtocol::Imap => "IMAP",
        MailProtocol::Pop3 => "POP3",
    };

    let account = EmailAccount {
        username: username.clone(),
        host: req.host.trim().to_string(),
        port: req.port,
        protocol: req.protocol,
        email: req.email.trim().to_string(),
        password: req.password,
        filters: Vec::new(),
    };

    // Verificar conexion antes de guardar
    let test_account = account.clone();
    let test_result = tokio::task::spawn_blocking(move || fetch_emails_dispatch(&test_account))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error interno: {}", e),
            )
        })?;

    if let Err(e) = test_result {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("No se pudo conectar al servidor {}: {}", protocol_label, e),
        ));
    }

    let mut config = state.config.lock().await;
    // Reemplazar cuenta existente del usuario o agregar nueva
    config.email.accounts.retain(|a| a.username != username);
    config.email.accounts.push(account);
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity(
            "email_configurado",
            &format!("Cuenta {}: {}", protocol_label, req.email),
            &username,
        )
        .await;

    Ok((StatusCode::OK, format!("Cuenta {} configurada correctamente", protocol_label)))
}

/// DELETE /api/email/account - Eliminar mi cuenta IMAP
pub async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let mut config = state.config.lock().await;
    let before = config.email.accounts.len();
    config.email.accounts.retain(|a| a.username != username);

    if config.email.accounts.len() == before {
        return Err((
            StatusCode::NOT_FOUND,
            "No tienes cuenta de correo configurada".to_string(),
        ));
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);

    // Limpiar inbox del usuario
    let mut inbox = state.email_inbox.lock().await;
    inbox.remove(&username);

    state
        .log_activity("email_eliminado", "Cuenta de correo eliminada", &username)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/email/inbox - Ver mi bandeja
pub async fn get_inbox(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<EmailMessage>>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let inbox = state.email_inbox.lock().await;
    let emails = inbox.get(&username).cloned().unwrap_or_default();
    Ok(Json(emails))
}

/// POST /api/email/check - Forzar revision de correo ahora
pub async fn check_now(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let config = state.config.lock().await;
    let account = config
        .email
        .accounts
        .iter()
        .find(|a| a.username == username)
        .cloned();
    let groq_key = config.email.groq_api_key.clone();
    drop(config);

    let Some(account) = account else {
        return Err((
            StatusCode::NOT_FOUND,
            "No tienes cuenta de correo configurada".to_string(),
        ));
    };

    let cloned_account = account.clone();
    let emails_result = tokio::task::spawn_blocking(move || fetch_emails_dispatch(&cloned_account))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error interno: {}", e),
            )
        })?;

    let mut emails = emails_result.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error obteniendo correos: {}", e),
        )
    })?;

    // Si hay groq_key, clasificar emails nuevos
    if let Some(ref key) = groq_key {
        for email in &mut emails {
            if email.ai_classification.is_none() {
                match classify_with_groq(&state.http_client, key, email).await {
                    Ok((classification, summary, action)) => {
                        email.ai_classification = Some(classification);
                        email.ai_summary = Some(summary);
                        email.ai_action = Some(action);
                        email.processed = true;
                    }
                    Err(e) => {
                        eprintln!("[Email] Error clasificando con Groq: {}", e);
                    }
                }
            }
        }
    }

    let count = emails.len();
    let mut inbox = state.email_inbox.lock().await;
    inbox.insert(username.clone(), emails);

    state
        .log_activity(
            "email_check",
            &format!("{} correos obtenidos", count),
            &username,
        )
        .await;

    Ok((
        StatusCode::OK,
        format!("{} correos obtenidos", count),
    ))
}

/// POST /api/email/classify/{uid} - Clasificar un email con IA
pub async fn classify_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(uid): Path<u32>,
) -> Result<Json<EmailMessage>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let config = state.config.lock().await;
    let groq_key = config
        .email
        .groq_api_key
        .clone()
        .ok_or((StatusCode::BAD_REQUEST, "Groq API key no configurada".to_string()))?;
    drop(config);

    let mut inbox = state.email_inbox.lock().await;
    let emails = inbox
        .get_mut(&username)
        .ok_or((StatusCode::NOT_FOUND, "No tienes correos".to_string()))?;
    let email = emails
        .iter_mut()
        .find(|e| e.uid == uid)
        .ok_or((StatusCode::NOT_FOUND, "Correo no encontrado".to_string()))?;

    // Clasificar con Groq (necesitamos clonar para la llamada async)
    let email_clone = email.clone();
    drop(inbox);

    let (classification, summary, action) =
        classify_with_groq(&state.http_client, &groq_key, &email_clone)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error Groq: {}", e)))?;

    // Guardar resultado
    let mut inbox = state.email_inbox.lock().await;
    let emails = inbox.get_mut(&username).unwrap();
    let email = emails.iter_mut().find(|e| e.uid == uid).unwrap();
    email.ai_classification = Some(classification);
    email.ai_summary = Some(summary);
    email.ai_action = Some(action);
    email.processed = true;

    Ok(Json(email.clone()))
}

/// POST /api/email/to-task/{uid} - Convertir email en tarea
pub async fn email_to_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(uid): Path<u32>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    // Buscar el email
    let mut inbox = state.email_inbox.lock().await;
    let emails = inbox
        .get_mut(&username)
        .ok_or((StatusCode::NOT_FOUND, "No tienes correos".to_string()))?;
    let email = emails
        .iter_mut()
        .find(|e| e.uid == uid)
        .ok_or((StatusCode::NOT_FOUND, "Correo no encontrado".to_string()))?;

    if email.task_created {
        return Err((
            StatusCode::CONFLICT,
            "Ya se creo una tarea para este correo".to_string(),
        ));
    }

    let title = if let Some(ref summary) = email.ai_summary {
        format!("[Email] {} - {}", email.subject, summary)
    } else {
        format!("[Email] {}", email.subject)
    };
    let title = if title.len() > 200 {
        format!("{}...", &title[..197])
    } else {
        title
    };

    email.task_created = true;
    drop(inbox);

    // Crear la tarea
    let task = crate::models::tasks::Task {
        id: uuid::Uuid::new_v4().to_string()[..6].to_string(),
        project_id: None,
        title: title.clone(),
        description: format!("De: {}\nFecha: {}\n\n{}", email_from_str(uid, &state, &username).await, "", ""),
        assigned_to: vec![username.clone()],
        status: crate::models::tasks::TaskStatus::Pendiente,
        created_by: username.clone(),
        due_date: None,
        requires_confirmation: false,
        insistent: false,
        reminder_minutes: 8,
        confirmed_by: Vec::new(),
        rejected_by: Vec::new(),
        created_at: Utc::now(),
        last_reminder: None,
    };

    let mut config = state.config.lock().await;
    config.tasks.tasks.push(task);
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity("email_a_tarea", &title, &username)
        .await;

    Ok((StatusCode::CREATED, format!("Tarea creada: {}", title)))
}

/// Auxiliar para obtener el from de un email
async fn email_from_str(uid: u32, state: &AppState, username: &str) -> String {
    let inbox = state.email_inbox.lock().await;
    inbox
        .get(username)
        .and_then(|emails| emails.iter().find(|e| e.uid == uid))
        .map(|e| e.from.clone())
        .unwrap_or_default()
}

/// POST /api/email/groq-key - Configurar API key de Groq (admin)
pub async fn set_groq_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetGroqKeyRequest>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _role) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let key = req.key.trim().to_string();
    if key.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Key vacia".to_string()));
    }

    let mut config = state.config.lock().await;
    config.email.groq_api_key = Some(key);
    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    drop(config);
    state
        .log_activity("groq_key", "API key de Groq configurada", &username)
        .await;

    Ok((StatusCode::OK, "Groq API key configurada".to_string()))
}

// =====================
// Email filters
// =====================

pub async fn list_filters(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<EmailFilter>>, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let config = state.config.lock().await;
    let account = config.email.accounts.iter().find(|a| a.username == username);
    Ok(Json(account.map(|a| a.filters.clone()).unwrap_or_default()))
}

#[derive(Debug, Deserialize)]
pub struct AddFilterRequest {
    pub pattern: String,
    pub action: FilterAction,
    pub label: String,
    #[serde(default)]
    pub auto_tag: Option<String>,
}

pub async fn add_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AddFilterRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let mut config = state.config.lock().await;
    let account = config.email.accounts.iter_mut().find(|a| a.username == username)
        .ok_or((StatusCode::NOT_FOUND, "Cuenta de correo no configurada".to_string()))?;

    // No duplicar
    if account.filters.iter().any(|f| f.pattern == req.pattern) {
        return Err((StatusCode::CONFLICT, "Filtro ya existe".to_string()));
    }

    account.filters.push(EmailFilter {
        pattern: req.pattern,
        action: req.action,
        label: req.label,
        auto_tag: req.auto_tag,
    });

    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::CREATED)
}

pub async fn delete_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(pattern): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.lock().await;
    let (username, _) = extract_username(&sessions, &headers)
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    drop(sessions);

    let mut config = state.config.lock().await;
    let account = config.email.accounts.iter_mut().find(|a| a.username == username)
        .ok_or((StatusCode::NOT_FOUND, "Cuenta no configurada".to_string()))?;

    let decoded = urlencoding::decode(&pattern).unwrap_or_default().to_string();
    let before = account.filters.len();
    account.filters.retain(|f| f.pattern != decoded);
    if account.filters.len() == before {
        return Err((StatusCode::NOT_FOUND, "Filtro no encontrado".to_string()));
    }

    save_config(&config).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Aplica filtros del usuario a un email. Devuelve (label, action) o None si no matchea.
fn apply_filters(filters: &[EmailFilter], from: &str) -> Option<(String, FilterAction, Option<String>)> {
    let from_lower = from.to_lowercase();
    for f in filters {
        if from_lower.contains(&f.pattern.to_lowercase()) {
            return Some((f.label.clone(), f.action.clone(), f.auto_tag.clone()));
        }
    }
    None
}

// =====================
// Dispatch por protocolo
// =====================

pub fn fetch_emails_dispatch(account: &EmailAccount) -> Result<Vec<EmailMessage>, String> {
    match account.protocol {
        MailProtocol::Imap => fetch_emails_imap(account),
        MailProtocol::Pop3 => fetch_emails_pop3(account),
    }
}

// =====================
// IMAP - Fetch emails (BLOCKING)
// =====================

fn fetch_emails_imap(account: &EmailAccount) -> Result<Vec<EmailMessage>, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("Error TLS: {}", e))?;

    let addr = (&*account.host, account.port);
    let client = imap::connect(addr, &account.host, &tls)
        .map_err(|e| format!("Error conectando a IMAP: {}", e))?;

    let mut session = client
        .login(&account.email, &account.password)
        .map_err(|e| format!("Error de login IMAP: {}", e.0))?;

    session
        .select("INBOX")
        .map_err(|e| format!("Error seleccionando INBOX: {}", e))?;

    // Buscar no leidos
    let search_result = session
        .search("UNSEEN")
        .map_err(|e| format!("Error buscando correos: {}", e))?;

    let mut seqs: Vec<u32> = search_result.into_iter().collect();
    seqs.sort();

    // Tomar los ultimos 20
    let seqs: Vec<u32> = if seqs.len() > 20 {
        seqs[seqs.len() - 20..].to_vec()
    } else {
        seqs
    };

    if seqs.is_empty() {
        let _ = session.logout();
        return Ok(Vec::new());
    }

    let seq_str = seqs
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let messages = session
        .fetch(&seq_str, "( UID ENVELOPE BODY.PEEK[] )")
        .map_err(|e| format!("Error obteniendo correos: {}", e))?;

    let mut emails = Vec::new();

    for msg in messages.iter() {
        let uid = msg.uid.unwrap_or(0);
        if uid == 0 {
            continue;
        }

        let envelope = msg.envelope();
        let subject = envelope
            .map(|env| {
                env.subject
                    .map(|s| String::from_utf8_lossy(s).to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Decodificar subject MIME si es necesario
        let subject = decode_mime_header(&subject);

        let from = if let Some(env) = envelope {
            if let Some(addrs) = &env.from {
                if let Some(addr) = addrs.first() {
                    let name = addr
                        .name
                        .map(|n| decode_mime_header(&String::from_utf8_lossy(n).to_string()))
                        .unwrap_or_default();
                    let mailbox = addr
                        .mailbox
                        .map(|m| String::from_utf8_lossy(m).to_string())
                        .unwrap_or_default();
                    let host = addr
                        .host
                        .map(|h| String::from_utf8_lossy(h).to_string())
                        .unwrap_or_default();
                    if name.is_empty() {
                        format!("{}@{}", mailbox, host)
                    } else {
                        format!("{} <{}@{}>", name, mailbox, host)
                    }
                } else {
                    "desconocido".to_string()
                }
            } else {
                "desconocido".to_string()
            }
        } else {
            "desconocido".to_string()
        };

        let date = envelope
            .map(|env| {
                env.date
                    .map(|d| String::from_utf8_lossy(d).to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Parsear body con mailparse
        let body_preview = msg
            .body()
            .and_then(|body| {
                let parsed = mailparse::parse_mail(body).ok()?;
                let text = extract_text_body(&parsed);
                Some(text)
            })
            .unwrap_or_default();

        // Limitar a 500 chars
        let body_preview = if body_preview.len() > 500 {
            format!("{}...", &body_preview[..497])
        } else {
            body_preview
        };

        emails.push(EmailMessage {
            uid,
            from,
            subject,
            date,
            body_preview,
            ai_classification: None,
            ai_summary: None,
            ai_action: None,
            filter_label: None,
            filter_action: None,
            processed: false,
            task_created: false,
            fetched_at: Utc::now(),
        });
    }

    let _ = session.logout();
    Ok(emails)
}

/// Extrae el texto plano de un email parseado
fn extract_text_body(parsed: &mailparse::ParsedMail) -> String {
    // Si tiene subpartes (multipart), buscar text/plain
    if !parsed.subparts.is_empty() {
        for part in &parsed.subparts {
            let ct = part
                .ctype
                .mimetype
                .to_lowercase();
            if ct == "text/plain" {
                return part.get_body().unwrap_or_default();
            }
        }
        // Si no hay text/plain, buscar text/html y limpiar tags
        for part in &parsed.subparts {
            let ct = part
                .ctype
                .mimetype
                .to_lowercase();
            if ct == "text/html" {
                let html = part.get_body().unwrap_or_default();
                return strip_html_tags(&html);
            }
        }
        // Buscar recursivamente en subpartes
        for part in &parsed.subparts {
            let text = extract_text_body(part);
            if !text.is_empty() {
                return text;
            }
        }
    }

    // Es un mensaje simple
    let ct = parsed.ctype.mimetype.to_lowercase();
    if ct == "text/plain" {
        parsed.get_body().unwrap_or_default()
    } else if ct == "text/html" {
        let html = parsed.get_body().unwrap_or_default();
        strip_html_tags(&html)
    } else {
        parsed.get_body().unwrap_or_default()
    }
}

/// Remueve tags HTML de forma basica
fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Limpiar whitespace excesivo
    result
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Decodifica headers MIME (=?UTF-8?Q?...?= o =?UTF-8?B?...?=)
fn decode_mime_header(input: &str) -> String {
    // Intentar decodificar con mailparse
    match mailparse::parse_header(format!("Subject: {}", input).as_bytes()) {
        Ok((header, _)) => header.get_value(),
        Err(_) => input.to_string(),
    }
}

// =====================
// POP3 - Fetch emails (BLOCKING)
// =====================

fn fetch_emails_pop3(account: &EmailAccount) -> Result<Vec<EmailMessage>, String> {
    use std::io::{BufRead, BufReader, Write};

    let tls = native_tls::TlsConnector::new().map_err(|e| format!("Error TLS: {}", e))?;
    let tcp = std::net::TcpStream::connect((&*account.host, account.port))
        .map_err(|e| format!("Error conectando a POP3 {}:{}: {}", account.host, account.port, e))?;
    tcp.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
    tcp.set_write_timeout(Some(std::time::Duration::from_secs(15))).ok();

    let stream = tls
        .connect(&account.host, tcp)
        .map_err(|e| format!("Error TLS POP3: {}", e))?;

    let mut reader = BufReader::new(stream);

    fn pop3_read_line(reader: &mut BufReader<native_tls::TlsStream<std::net::TcpStream>>) -> Result<String, String> {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| format!("Error leyendo POP3: {}", e))?;
        Ok(line)
    }

    fn pop3_send(reader: &mut BufReader<native_tls::TlsStream<std::net::TcpStream>>, cmd: &str) -> Result<(), String> {
        reader.get_mut().write_all(format!("{}\r\n", cmd).as_bytes())
            .map_err(|e| format!("Error enviando POP3: {}", e))?;
        reader.get_mut().flush().map_err(|e| format!("Error flush POP3: {}", e))?;
        Ok(())
    }

    fn pop3_cmd(reader: &mut BufReader<native_tls::TlsStream<std::net::TcpStream>>, cmd: &str) -> Result<String, String> {
        pop3_send(reader, cmd)?;
        pop3_read_line(reader)
    }

    // Leer greeting
    let greeting = pop3_read_line(&mut reader)?;
    if !greeting.starts_with("+OK") {
        return Err(format!("POP3 greeting inesperado: {}", greeting.trim()));
    }

    // AUTH
    let resp = pop3_cmd(&mut reader, &format!("USER {}", account.email))?;
    if !resp.starts_with("+OK") {
        return Err(format!("POP3 USER rechazado: {}", resp.trim()));
    }

    let resp = pop3_cmd(&mut reader, &format!("PASS {}", account.password))?;
    if !resp.starts_with("+OK") {
        return Err(format!("POP3 login fallido: {}", resp.trim()));
    }

    // STAT para obtener cantidad de mensajes
    let resp = pop3_cmd(&mut reader, "STAT")?;
    if !resp.starts_with("+OK") {
        return Err(format!("POP3 STAT error: {}", resp.trim()));
    }
    let total: usize = resp.trim()
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if total == 0 {
        let _ = pop3_cmd(&mut reader, "QUIT");
        return Ok(Vec::new());
    }

    // UIDL para IDs unicos
    let resp = pop3_cmd(&mut reader, "UIDL")?;
    if !resp.starts_with("+OK") {
        return Err(format!("POP3 UIDL error: {}", resp.trim()));
    }
    let mut uidl_map: Vec<(usize, String)> = Vec::new();
    loop {
        let line = pop3_read_line(&mut reader)?;
        let line = line.trim().to_string();
        if line == "." { break; }
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() == 2 {
            if let Ok(num) = parts[0].parse::<usize>() {
                uidl_map.push((num, parts[1].to_string()));
            }
        }
    }

    // Tomar los ultimos 20
    let start = if uidl_map.len() > 20 { uidl_map.len() - 20 } else { 0 };
    let to_fetch: Vec<(usize, String)> = uidl_map[start..].to_vec();

    let mut emails = Vec::new();

    for (msg_num, uidl) in &to_fetch {
        // RETR para obtener el mensaje completo
        let resp = pop3_cmd(&mut reader, &format!("RETR {}", msg_num))?;
        if !resp.starts_with("+OK") {
            continue;
        }

        let mut raw_msg = Vec::new();
        loop {
            let line = pop3_read_line(&mut reader)?;
            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
            if trimmed == "." { break; }
            // Byte-stuffing: lineas que empiezan con ".." se decodifican a "."
            let decoded = if trimmed.starts_with("..") { &trimmed[1..] } else { trimmed };
            raw_msg.extend_from_slice(decoded.as_bytes());
            raw_msg.push(b'\n');
        }

        // Parsear con mailparse
        let parsed = match mailparse::parse_mail(&raw_msg) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Generar UID numerico a partir del UIDL string
        let uid: u32 = uidl.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));

        let subject = parsed.headers.iter()
            .find(|h| h.get_key().eq_ignore_ascii_case("subject"))
            .map(|h| decode_mime_header(&h.get_value()))
            .unwrap_or_default();

        let from = parsed.headers.iter()
            .find(|h| h.get_key().eq_ignore_ascii_case("from"))
            .map(|h| decode_mime_header(&h.get_value()))
            .unwrap_or_else(|| "desconocido".to_string());

        let date = parsed.headers.iter()
            .find(|h| h.get_key().eq_ignore_ascii_case("date"))
            .map(|h| h.get_value())
            .unwrap_or_default();

        let body_preview = extract_text_body(&parsed);
        let body_preview = if body_preview.len() > 500 {
            format!("{}...", &body_preview[..497])
        } else {
            body_preview
        };

        emails.push(EmailMessage {
            uid,
            from,
            subject,
            date,
            body_preview,
            ai_classification: None,
            ai_summary: None,
            ai_action: None,
            filter_label: None,
            filter_action: None,
            processed: false,
            task_created: false,
            fetched_at: Utc::now(),
        });
    }

    let _ = pop3_cmd(&mut reader, "QUIT");
    Ok(emails)
}

// =====================
// Groq AI classification
// =====================

/// Clasificar email con Groq LLM
pub async fn classify_with_groq(
    client: &reqwest::Client,
    api_key: &str,
    email: &EmailMessage,
) -> Result<(String, String, String), String> {
    let body = serde_json::json!({
        "model": "llama-3.3-70b-versatile",
        "messages": [
            {
                "role": "system",
                "content": "Clasifica este correo electronico en una de estas categorias: urgente, tarea, informativo, spam.\nResponde SOLO con un JSON valido (sin markdown, sin ```): {\"clasificacion\": \"...\", \"resumen\": \"...\", \"accion\": \"...\"}\nDonde:\n- clasificacion: urgente, tarea, informativo o spam\n- resumen: resumen en 1-2 oraciones en espanol\n- accion: accion sugerida en espanol (ej: 'Responder con informacion solicitada', 'Archivar', 'Crear tarea de seguimiento')"
            },
            {
                "role": "user",
                "content": format!("De: {}\nAsunto: {}\n\n{}", email.from, email.subject, email.body_preview)
            }
        ],
        "temperature": 0.1,
        "max_tokens": 300
    });

    let resp = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Error conectando a Groq: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Groq respondio {}: {}", status, text));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Error parseando respuesta Groq: {}", e))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "Respuesta de Groq sin contenido".to_string())?;

    // Parsear el JSON de la respuesta
    let parsed: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| format!("Error parseando JSON de Groq: {} - Contenido: {}", e, content))?;

    let clasificacion = parsed["clasificacion"]
        .as_str()
        .unwrap_or("informativo")
        .to_string();
    let resumen = parsed["resumen"]
        .as_str()
        .unwrap_or("Sin resumen")
        .to_string();
    let accion = parsed["accion"]
        .as_str()
        .unwrap_or("Sin accion sugerida")
        .to_string();

    Ok((clasificacion, resumen, accion))
}

// =====================
// Background loop
// =====================

/// Loop que revisa correos cada 5 minutos
pub async fn email_check_loop(state: AppState) {
    // Esperar 30 segundos antes de la primera revision
    tokio::time::sleep(Duration::from_secs(30)).await;

    loop {
        let config = state.config.lock().await;
        let accounts = config.email.accounts.clone();
        let groq_key = config.email.groq_api_key.clone();
        let token = config.notifications.bot_token.clone();
        let chats = config.notifications.telegram_chats.clone();
        drop(config);

        for account in &accounts {
            let account_clone = account.clone();
            let result =
                tokio::task::spawn_blocking(move || fetch_emails_dispatch(&account_clone)).await;

            let emails_result = match result {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "[Email] Error en spawn_blocking para {}: {}",
                        account.email, e
                    );
                    continue;
                }
            };

            let mut emails = match emails_result {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("[Email] Error {:?} para {}: {}", account.protocol, account.email, e);
                    continue;
                }
            };

            // Aplicar filtros del usuario
            let filters = &account.filters;
            emails.retain_mut(|email| {
                if let Some((label, action, auto_tag)) = apply_filters(filters, &email.from) {
                    email.filter_label = Some(label);
                    email.filter_action = Some(action.clone());
                    match action {
                        FilterAction::Ignorar => return false, // descartar
                        FilterAction::Prioritario => {
                            email.ai_classification = Some("urgente".to_string());
                            email.ai_summary = Some(format!("Correo prioritario ({})", email.filter_label.as_deref().unwrap_or("filtro")));
                            email.ai_action = auto_tag.or(Some("Responder".to_string()));
                            email.processed = true;
                        }
                        FilterAction::Silencioso => {
                            // Se clasificará con IA pero no notificará
                        }
                        FilterAction::Normal => {
                            // Clasificar normalmente con IA
                        }
                    }
                }
                true
            });

            // Clasificar con Groq los que no fueron procesados por filtros
            if let Some(ref key) = groq_key {
                for email in &mut emails {
                    if email.ai_classification.is_none() {
                        match classify_with_groq(&state.http_client, key, email).await {
                            Ok((classification, summary, action)) => {
                                email.ai_classification = Some(classification);
                                email.ai_summary = Some(summary);
                                email.ai_action = Some(action);
                                email.processed = true;
                            }
                            Err(e) => {
                                eprintln!("[Email] Error Groq para {}: {}", account.email, e);
                            }
                        }
                    }
                }
            }

            // Guardar en inbox
            let username = account.username.clone();
            let mut inbox = state.email_inbox.lock().await;

            // Detectar nuevos emails comparando UIDs
            let existing_uids: Vec<u32> = inbox
                .get(&username)
                .map(|existing| existing.iter().map(|e| e.uid).collect())
                .unwrap_or_default();
            let new_emails: Vec<&EmailMessage> = emails
                .iter()
                .filter(|e| !existing_uids.contains(&e.uid))
                .collect();
            let new_count = new_emails.len();
            let new_urgent: Vec<String> = new_emails
                .iter()
                .filter(|e| {
                    e.ai_classification.as_deref() == Some("urgente")
                        && e.filter_action != Some(FilterAction::Silencioso)
                })
                .map(|e| {
                    let label = e.filter_label.as_deref().map(|l| format!(" [{}]", l)).unwrap_or_default();
                    format!("- {}{}: {}", e.from, label, e.subject)
                })
                .collect();

            inbox.insert(username.clone(), emails);
            drop(inbox);

            // Notificar por Telegram si hay correos urgentes nuevos
            if !new_urgent.is_empty() {
                if let Some(ref token) = token {
                    // Buscar chat_id del usuario via linked_web_user
                    let target_chat = chats
                        .iter()
                        .find(|c| c.linked_web_user.as_deref() == Some(&username));

                    if let Some(chat) = target_chat {
                        let msg = format!(
                            "*Correo urgente!*\n\n{} correo(s) nuevo(s), {} urgente(s):\n{}",
                            new_count,
                            new_urgent.len(),
                            new_urgent.join("\n")
                        );
                        let _ = send_telegram_notification(
                            &state.http_client,
                            token,
                            chat.chat_id,
                            &msg,
                        )
                        .await;
                    }
                }
            }
        }

        // Esperar 5 minutos
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

/// Enviar notificacion por Telegram (wrapper simple)
async fn send_telegram_notification(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "Markdown"
    });
    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
    client
        .post(&url)
        .json(&body)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Error Telegram: {}", e))?;
    Ok(())
}

// =====================
// Telegram command helpers (publicas para notifications.rs)
// =====================

/// Resumen de correos para un usuario de Telegram
pub async fn get_emails_summary(state: &AppState, chat_name: &str) -> String {
    // Buscar el web user vinculado a este chat
    let config = state.config.lock().await;
    let linked_user = config
        .notifications
        .telegram_chats
        .iter()
        .find(|c| c.name == chat_name)
        .and_then(|c| c.linked_web_user.clone());
    drop(config);

    let Some(username) = linked_user else {
        return "No tienes cuenta web vinculada. Usa `/vincular CODIGO` primero.".to_string();
    };

    let inbox = state.email_inbox.lock().await;
    let emails = match inbox.get(&username) {
        Some(e) if !e.is_empty() => e.clone(),
        _ => {
            return format!(
                "*Correos de {}*\n\nNo hay correos. Configura tu cuenta desde la web o usa `/correos check` para revisar.",
                username
            );
        }
    };
    drop(inbox);

    let urgent = emails
        .iter()
        .filter(|e| e.ai_classification.as_deref() == Some("urgente"))
        .count();
    let tasks = emails
        .iter()
        .filter(|e| e.ai_classification.as_deref() == Some("tarea"))
        .count();
    let info = emails
        .iter()
        .filter(|e| e.ai_classification.as_deref() == Some("informativo"))
        .count();
    let spam = emails
        .iter()
        .filter(|e| e.ai_classification.as_deref() == Some("spam"))
        .count();
    let unclassified = emails
        .iter()
        .filter(|e| e.ai_classification.is_none())
        .count();

    let mut msg = format!(
        "*Correos* ({} total)\n\nUrgentes: {}\nTareas: {}\nInformativos: {}\nSpam: {}\nSin clasificar: {}\n\n*Ultimos correos:*\n",
        emails.len(),
        urgent,
        tasks,
        info,
        spam,
        unclassified
    );

    // Mostrar los ultimos 10
    let start = if emails.len() > 10 {
        emails.len() - 10
    } else {
        0
    };
    for email in &emails[start..] {
        let classification = email
            .ai_classification
            .as_deref()
            .unwrap_or("?");
        let icon = match classification {
            "urgente" => "!",
            "tarea" => "T",
            "informativo" => "i",
            "spam" => "S",
            _ => "?",
        };
        let subject_short = if email.subject.len() > 40 {
            format!("{}...", &email.subject[..37])
        } else {
            email.subject.clone()
        };
        msg.push_str(&format!(
            "\n[{}] `{}` {}\n  De: {}",
            icon, email.uid, subject_short, email.from
        ));
        if let Some(ref summary) = email.ai_summary {
            let summary_short = if summary.len() > 60 {
                format!("{}...", &summary[..57])
            } else {
                summary.clone()
            };
            msg.push_str(&format!("\n  _{}_", summary_short));
        }
        msg.push('\n');
    }

    msg.push_str("\nUsa `/leer UID` para ver detalle o `/correo2tarea UID` para crear tarea.");
    msg
}

/// Leer detalle de un email por UID
pub async fn get_email_detail(state: &AppState, chat_name: &str, uid_str: &str) -> String {
    let config = state.config.lock().await;
    let linked_user = config
        .notifications
        .telegram_chats
        .iter()
        .find(|c| c.name == chat_name)
        .and_then(|c| c.linked_web_user.clone());
    drop(config);

    let Some(username) = linked_user else {
        return "No tienes cuenta web vinculada. Usa `/vincular CODIGO` primero.".to_string();
    };

    let uid: u32 = match uid_str.trim().parse() {
        Ok(u) => u,
        Err(_) => return "UID invalido. Uso: `/leer 12345`".to_string(),
    };

    let inbox = state.email_inbox.lock().await;
    let emails = match inbox.get(&username) {
        Some(e) => e,
        None => return "No tienes correos.".to_string(),
    };

    let email = match emails.iter().find(|e| e.uid == uid) {
        Some(e) => e.clone(),
        None => return format!("Correo con UID {} no encontrado.", uid),
    };
    drop(inbox);

    let mut msg = format!("*Correo {}*\n\n", email.uid);
    msg.push_str(&format!("*De:* {}\n", email.from));
    msg.push_str(&format!("*Asunto:* {}\n", email.subject));
    msg.push_str(&format!("*Fecha:* {}\n", email.date));

    if let Some(ref classification) = email.ai_classification {
        msg.push_str(&format!("\n*Clasificacion:* {}\n", classification));
    }
    if let Some(ref summary) = email.ai_summary {
        msg.push_str(&format!("*Resumen IA:* {}\n", summary));
    }
    if let Some(ref action) = email.ai_action {
        msg.push_str(&format!("*Accion sugerida:* {}\n", action));
    }

    // Body preview (limitado para Telegram)
    let preview = if email.body_preview.len() > 1000 {
        format!("{}...", &email.body_preview[..997])
    } else {
        email.body_preview.clone()
    };
    msg.push_str(&format!("\n```\n{}\n```", preview));

    if !email.task_created {
        msg.push_str(&format!("\n`/correo2tarea {}`", email.uid));
    }

    msg
}

/// Convertir email a tarea insistente desde Telegram
pub async fn telegram_email_to_task(state: &AppState, chat_name: &str, uid_str: &str) -> String {
    let config = state.config.lock().await;
    let linked_user = config
        .notifications
        .telegram_chats
        .iter()
        .find(|c| c.name == chat_name)
        .and_then(|c| c.linked_web_user.clone());
    drop(config);

    let Some(username) = linked_user else {
        return "No tienes cuenta web vinculada. Usa `/vincular CODIGO` primero.".to_string();
    };

    let uid: u32 = match uid_str.trim().parse() {
        Ok(u) => u,
        Err(_) => return "UID invalido. Uso: `/correo2tarea 12345`".to_string(),
    };

    let mut inbox = state.email_inbox.lock().await;
    let emails = match inbox.get_mut(&username) {
        Some(e) => e,
        None => return "No tienes correos.".to_string(),
    };

    let email = match emails.iter_mut().find(|e| e.uid == uid) {
        Some(e) => e,
        None => return format!("Correo con UID {} no encontrado.", uid),
    };

    if email.task_created {
        return "Ya se creo una tarea para este correo.".to_string();
    }

    let title = format!("[Email] {}", email.subject);
    let title = if title.len() > 200 {
        format!("{}...", &title[..197])
    } else {
        title
    };

    email.task_created = true;
    let from = email.from.clone();
    drop(inbox);

    // Crear tarea insistente
    let task = crate::models::tasks::Task {
        id: uuid::Uuid::new_v4().to_string()[..6].to_string(),
        project_id: None,
        title: title.clone(),
        description: format!("De: {}", from),
        assigned_to: vec![chat_name.to_string()],
        status: crate::models::tasks::TaskStatus::Pendiente,
        created_by: chat_name.to_string(),
        due_date: None,
        requires_confirmation: true,
        insistent: true,
        reminder_minutes: 8,
        confirmed_by: Vec::new(),
        rejected_by: Vec::new(),
        created_at: Utc::now(),
        last_reminder: None,
    };

    let task_id = task.id.clone();
    let mut config = state.config.lock().await;
    config.tasks.tasks.push(task);
    let _ = save_config(&config).await;

    format!(
        "Tarea insistente creada!\n*{}*\nID: `{}`\n\nSe te recordara cada 8 min hasta confirmar.",
        title, task_id
    )
}
