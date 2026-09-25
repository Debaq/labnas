//! Descarga de correo por IMAP y POP3

use super::*;

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

pub(super) fn fetch_emails_imap(account: &EmailAccount) -> Result<Vec<EmailMessage>, String> {
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
                        .map(|n| decode_mime_header(String::from_utf8_lossy(n).as_ref()))
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
pub(super) fn extract_text_body(parsed: &mailparse::ParsedMail) -> String {
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
pub(super) fn strip_html_tags(html: &str) -> String {
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
pub(super) fn decode_mime_header(input: &str) -> String {
    // Intentar decodificar con mailparse
    match mailparse::parse_header(format!("Subject: {}", input).as_bytes()) {
        Ok((header, _)) => header.get_value(),
        Err(_) => input.to_string(),
    }
}

// =====================
// POP3 - Fetch emails (BLOCKING)
// =====================

pub(super) fn fetch_emails_pop3(account: &EmailAccount) -> Result<Vec<EmailMessage>, String> {
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
    let total: usize = resp.split_whitespace()
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
