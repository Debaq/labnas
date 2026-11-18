//! Persistencia de correos procesados y propuestas de la IA (SQLite).
//! Funciones sincronas: llamarlas dentro de `db_op`.

use super::*;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashSet;

use super::ai::ProposedItem;

/// Correos que muestra la bandeja
pub const INBOX_LIMIT: i64 = 50;
pub const PREVIEW_CHARS: usize = 500;

pub fn load_account(conn: &Connection, username: &str) -> Result<Option<EmailAccount>, String> {
    conn.query_row(
        "SELECT username, host, port, protocol, email, labnas_decrypt(password), filters FROM email_accounts WHERE username = ?1",
        params![username],
        row_to_account,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn load_accounts(conn: &Connection) -> Result<Vec<EmailAccount>, String> {
    let mut stmt = conn
        .prepare("SELECT username, host, port, protocol, email, labnas_decrypt(password), filters FROM email_accounts")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row_to_account).map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn row_to_account(row: &rusqlite::Row) -> rusqlite::Result<EmailAccount> {
    let proto: String = row.get(3)?;
    let filters: String = row.get(6)?;
    Ok(EmailAccount {
        username: row.get(0)?,
        host: row.get(1)?,
        port: row.get::<_, i64>(2)? as u16,
        protocol: if proto == "pop3" { MailProtocol::Pop3 } else { MailProtocol::Imap },
        email: row.get(4)?,
        password: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        filters: serde_json::from_str(&filters).unwrap_or_default(),
    })
}

pub fn known_uids(conn: &Connection, username: &str) -> Result<HashSet<u32>, String> {
    let mut stmt = conn.prepare("SELECT uid FROM emails WHERE username = ?1").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![username], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).map(|u| u as u32).collect())
}

pub struct NewEmail<'a> {
    pub username: &'a str,
    pub msg: &'a EmailMessage,
    pub message_id: Option<&'a str>,
    pub body_text: &'a str,
}

pub fn insert_email(conn: &Connection, e: &NewEmail) -> Result<(), String> {
    let m = e.msg;
    conn.execute(
        "INSERT OR IGNORE INTO emails (username, uid, message_id, from_addr, subject, date, body_text,
            ai_classification, ai_summary, ai_action, filter_label, filter_action, processed, task_created, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            e.username, m.uid as i64, e.message_id, m.from, m.subject, m.date, e.body_text,
            m.ai_classification, m.ai_summary, m.ai_action, m.filter_label,
            m.filter_action.as_ref().map(filter_action_str),
            m.processed, m.task_created, m.fetched_at.to_rfc3339(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn filter_action_str(a: &FilterAction) -> &'static str {
    match a {
        FilterAction::Prioritario => "prioritario",
        FilterAction::Normal => "normal",
        FilterAction::Silencioso => "silencioso",
        FilterAction::Ignorar => "ignorar",
    }
}

fn parse_filter_action(s: &str) -> Option<FilterAction> {
    Some(match s {
        "prioritario" => FilterAction::Prioritario,
        "normal" => FilterAction::Normal,
        "silencioso" => FilterAction::Silencioso,
        "ignorar" => FilterAction::Ignorar,
        _ => return None,
    })
}

const EMAIL_COLS: &str = "uid, from_addr, subject, date, body_text, ai_classification, ai_summary, ai_action,
    filter_label, filter_action, processed, task_created, fetched_at";

fn row_to_email(row: &rusqlite::Row) -> rusqlite::Result<(EmailMessage, String)> {
    let body: String = row.get(4)?;
    let fetched: String = row.get(12)?;
    let msg = EmailMessage {
        uid: row.get::<_, i64>(0)? as u32,
        from: row.get(1)?,
        subject: row.get(2)?,
        date: row.get(3)?,
        body_preview: crate::textutil::ellipsize(&body, PREVIEW_CHARS),
        ai_classification: row.get(5)?,
        ai_summary: row.get(6)?,
        ai_action: row.get(7)?,
        filter_label: row.get(8)?,
        filter_action: row.get::<_, Option<String>>(9)?.as_deref().and_then(parse_filter_action),
        processed: row.get(10)?,
        task_created: row.get(11)?,
        fetched_at: chrono::DateTime::parse_from_rfc3339(&fetched)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    };
    Ok((msg, body))
}

/// Bandeja: los ultimos correos procesados (sin los ignorados por filtro), mas antiguos primero
pub fn list_inbox(conn: &Connection, username: &str) -> Result<Vec<EmailMessage>, String> {
    let sql = format!(
        "SELECT {} FROM (SELECT * FROM emails WHERE username = ?1 AND IFNULL(filter_action, '') != 'ignorar'
            ORDER BY fetched_at DESC, uid DESC LIMIT ?2) ORDER BY fetched_at ASC, uid ASC",
        EMAIL_COLS
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![username, INBOX_LIMIT], row_to_email)
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).map(|(m, _)| m).collect())
}

/// Un correo con su texto completo guardado
pub fn get_email(conn: &Connection, username: &str, uid: u32) -> Result<Option<(EmailMessage, String)>, String> {
    let sql = format!("SELECT {} FROM emails WHERE username = ?1 AND uid = ?2", EMAIL_COLS);
    conn.query_row(&sql, params![username, uid as i64], row_to_email)
        .optional()
        .map_err(|e| e.to_string())
}

pub fn set_ai(conn: &Connection, username: &str, uid: u32, class: &str, summary: &str, action: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE emails SET ai_classification = ?3, ai_summary = ?4, ai_action = ?5, processed = 1
         WHERE username = ?1 AND uid = ?2",
        params![username, uid as i64, class, summary, action],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Marca el correo como convertido en tarea; `false` si ya lo estaba (atomico)
pub fn claim_task_created(conn: &Connection, username: &str, uid: u32) -> Result<bool, String> {
    let n = conn
        .execute(
            "UPDATE emails SET task_created = 1 WHERE username = ?1 AND uid = ?2 AND task_created = 0",
            params![username, uid as i64],
        )
        .map_err(|e| e.to_string())?;
    Ok(n == 1)
}

pub fn delete_user_emails(conn: &Connection, username: &str) -> Result<(), String> {
    conn.execute("DELETE FROM emails WHERE username = ?1", params![username]).map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM ai_proposals WHERE username = ?1 AND status = 'pendiente'",
        params![username],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Borra correos y propuestas resueltas mas viejos que `days`
pub fn purge_old(conn: &Connection, days: u32) -> Result<usize, String> {
    let cutoff = (Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
    let a = conn.execute("DELETE FROM emails WHERE fetched_at < ?1", params![cutoff]).map_err(|e| e.to_string())?;
    let b = conn
        .execute(
            "DELETE FROM ai_proposals WHERE created_at < ?1 AND status != 'pendiente'",
            params![cutoff],
        )
        .map_err(|e| e.to_string())?;
    Ok(a + b)
}

// ─── Propuestas ───

#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub title: String,
    pub description: String,
    pub date: Option<String>,
    pub time: Option<String>,
    pub end_time: Option<String>,
    pub location: Option<String>,
    pub status: String,
    pub email_uid: Option<u32>,
    pub email_subject: Option<String>,
    pub email_from: Option<String>,
    pub created_at: String,
}

/// Id corto, comodo para escribir en Telegram (`/aprobar 1a2b3c4d`)
fn new_proposal_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

pub fn insert_proposal(
    conn: &Connection,
    username: &str,
    email_uid: Option<u32>,
    source: &str,
    p: &ProposedItem,
) -> Result<String, String> {
    let id = new_proposal_id();
    conn.execute(
        "INSERT INTO ai_proposals (id, username, email_uid, source, kind, title, description, date, time, end_time, location, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'pendiente', ?12)",
        params![
            id, username, email_uid.map(|u| u as i64), source, p.kind, p.title, p.description,
            p.date, p.time, p.end_time, p.location, Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

const PROPOSAL_SELECT: &str = "SELECT p.id, p.kind, p.source, p.title, p.description, p.date, p.time, p.end_time,
    p.location, p.status, p.email_uid, e.subject, e.from_addr, p.created_at
    FROM ai_proposals p LEFT JOIN emails e ON e.username = p.username AND e.uid = p.email_uid";

fn row_to_proposal(row: &rusqlite::Row) -> rusqlite::Result<Proposal> {
    Ok(Proposal {
        id: row.get(0)?,
        kind: row.get(1)?,
        source: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        date: row.get(5)?,
        time: row.get(6)?,
        end_time: row.get(7)?,
        location: row.get(8)?,
        status: row.get(9)?,
        email_uid: row.get::<_, Option<i64>>(10)?.map(|u| u as u32),
        email_subject: row.get(11)?,
        email_from: row.get(12)?,
        created_at: row.get(13)?,
    })
}

pub fn list_proposals(conn: &Connection, username: &str, status: &str) -> Result<Vec<Proposal>, String> {
    let sql = format!(
        "{} WHERE p.username = ?1 AND p.status = ?2 ORDER BY p.date IS NULL, p.date, p.time, p.created_at LIMIT 100",
        PROPOSAL_SELECT
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![username, status], row_to_proposal).map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_proposal(conn: &Connection, username: &str, id: &str) -> Result<Option<Proposal>, String> {
    let sql = format!("{} WHERE p.username = ?1 AND p.id = ?2", PROPOSAL_SELECT);
    conn.query_row(&sql, params![username, id], row_to_proposal)
        .optional()
        .map_err(|e| e.to_string())
}

/// Cierra una propuesta pendiente; `false` si ya no estaba pendiente (atomico)
pub fn resolve_proposal(
    conn: &Connection,
    username: &str,
    id: &str,
    status: &str,
    created_ref: Option<&str>,
) -> Result<bool, String> {
    let n = conn
        .execute(
            "UPDATE ai_proposals SET status = ?3, created_ref = ?4, resolved_at = ?5
             WHERE username = ?1 AND id = ?2 AND status = 'pendiente'",
            params![username, id, status, created_ref, Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
    Ok(n == 1)
}

pub fn count_pending(conn: &Connection, username: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM ai_proposals WHERE username = ?1 AND status = 'pendiente'",
        params![username],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}
