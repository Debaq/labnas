use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserReport {
    pub id: String,
    pub report_type: String, // "bug" | "solicitud"
    pub title: String,
    pub description: String,
    pub submitted_by: String,
    pub status: String, // "pendiente" | "visto" | "resuelto"
    pub admin_response: Option<String>,
    pub created_at: String,
    pub responded_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateReportRequest {
    pub report_type: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct RespondReportRequest {
    pub status: String,
    pub admin_response: Option<String>,
}

// ── Config ──

pub async fn get_reports_config(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let enabled = crate::db::db_op(&state.db, |conn| {
        Ok(crate::db::get_setting_bool(conn, "reports_enabled"))
    })
    .await
    .unwrap_or(false);

    Json(serde_json::json!({ "enabled": enabled }))
}

pub async fn set_reports_config(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    let enabled = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    crate::db::db_op(&state.db, move |conn| {
        crate::db::set_setting(conn, "reports_enabled", if enabled { "true" } else { "false" })
    })
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ── CRUD ──

pub async fn list_reports(
    State(state): State<AppState>,
) -> Result<Json<Vec<UserReport>>, (StatusCode, String)> {
    crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, report_type, title, description, submitted_by, status, admin_response, created_at, responded_at
                 FROM user_reports ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let reports = stmt
            .query_map([], |row| {
                Ok(UserReport {
                    id: row.get(0)?,
                    report_type: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    submitted_by: row.get(4)?,
                    status: row.get(5)?,
                    admin_response: row.get(6)?,
                    created_at: row.get(7)?,
                    responded_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(Json(reports))
    })
    .await
}

pub async fn create_report(
    State(state): State<AppState>,
    Json(req): Json<CreateReportRequest>,
) -> Result<(StatusCode, Json<UserReport>), (StatusCode, String)> {
    // Check if reports are enabled
    let enabled = crate::db::db_op(&state.db, |conn| {
        Ok(crate::db::get_setting_bool(conn, "reports_enabled"))
    })
    .await
    .unwrap_or(false);

    if !enabled {
        return Err((
            StatusCode::FORBIDDEN,
            "Los reportes no estan habilitados".to_string(),
        ));
    }

    let username = "usuario".to_string(); // Will be set from session in middleware

    let report = UserReport {
        id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
        report_type: req.report_type,
        title: req.title,
        description: req.description,
        submitted_by: username,
        status: "pendiente".to_string(),
        admin_response: None,
        created_at: Utc::now().to_rfc3339(),
        responded_at: None,
    };

    let r = report.clone();
    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO user_reports (id, report_type, title, description, submitted_by, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![r.id, r.report_type, r.title, r.description, r.submitted_by, r.status, r.created_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;

    // Notify admin via Telegram
    let title = report.title.clone();
    let rtype = report.report_type.clone();
    let by = report.submitted_by.clone();
    let http = state.http_client.clone();

    // Read DB synchronously before spawning async task
    let tg_data: Option<(String, Vec<i64>)> = crate::db::get_conn(&state.db)
        .ok()
        .and_then(|conn| {
            let token: Option<String> = conn
                .query_row("SELECT bot_token FROM notification_config WHERE id = 1", [], |row| row.get(0))
                .ok()?;
            let token = token?;
            let mut stmt = conn.prepare("SELECT chat_id FROM telegram_chats WHERE role = 'admin'").ok()?;
            let chats: Vec<i64> = stmt
                .query_map([], |row| row.get(0))
                .ok()?
                .filter_map(|r| r.ok())
                .collect();
            Some((token, chats))
        });

    if let Some((token, chats)) = tg_data {
        tokio::spawn(async move {
            let emoji = if rtype == "bug" { "[Bug]" } else { "[Solicitud]" };
            let msg = format!(
                "{} *Nuevo reporte*\n\nTipo: {}\nDe: {}\nTitulo: {}",
                emoji, rtype, by, title
            );
            for chat_id in chats {
                let _ = http
                    .post(format!("https://api.telegram.org/bot{}/sendMessage", token))
                    .json(&serde_json::json!({
                        "chat_id": chat_id,
                        "text": msg,
                        "parse_mode": "Markdown"
                    }))
                    .send()
                    .await;
            }
        });
    }

    state
        .log_activity(
            "Reportes",
            &format!("Nuevo {}: {}", report.report_type, report.title),
            &report.submitted_by,
        )
        .await;

    Ok((StatusCode::CREATED, Json(report)))
}

pub async fn respond_report(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RespondReportRequest>,
) -> Result<Json<UserReport>, (StatusCode, String)> {
    let now = Utc::now().to_rfc3339();

    crate::db::db_op_status(&state.db, move |conn| {
        conn.execute(
            "UPDATE user_reports SET status = ?1, admin_response = ?2, responded_at = ?3 WHERE id = ?4",
            params![req.status, req.admin_response, now, id],
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        conn.query_row(
            "SELECT id, report_type, title, description, submitted_by, status, admin_response, created_at, responded_at
             FROM user_reports WHERE id = ?1",
            params![id],
            |row| {
                Ok(Json(UserReport {
                    id: row.get(0)?,
                    report_type: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    submitted_by: row.get(4)?,
                    status: row.get(5)?,
                    admin_response: row.get(6)?,
                    created_at: row.get(7)?,
                    responded_at: row.get(8)?,
                }))
            },
        )
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))
    })
    .await
}

pub async fn delete_report(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::db::db_op(&state.db, move |conn| {
        conn.execute("DELETE FROM user_reports WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

// ── My reports (for non-admin users) ──

pub async fn my_reports(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<UserReport>>, (StatusCode, String)> {
    let username = params.get("user").cloned().unwrap_or_default();

    crate::db::db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, report_type, title, description, submitted_by, status, admin_response, created_at, responded_at
                 FROM user_reports WHERE submitted_by = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let reports = stmt
            .query_map(params![username], |row| {
                Ok(UserReport {
                    id: row.get(0)?,
                    report_type: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    submitted_by: row.get(4)?,
                    status: row.get(5)?,
                    admin_response: row.get(6)?,
                    created_at: row.get(7)?,
                    responded_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(Json(reports))
    })
    .await
}
