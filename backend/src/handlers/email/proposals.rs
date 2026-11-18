//! Propuestas de la IA: el usuario las aprueba (se crea el evento o la tarea) o las descarta.

use super::*;
use axum::extract::Query;
use axum::Extension;
use chrono::{NaiveDate, NaiveTime};
use serde::Serialize;

use super::store::{self, Proposal};
use crate::state::SessionInfo;

#[derive(Deserialize)]
pub struct ProposalsQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// GET /api/email/proposals?status=pendiente|aprobada|descartada
pub async fn list_proposals(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Query(q): Query<ProposalsQuery>,
) -> Result<Json<Vec<Proposal>>, (StatusCode, String)> {
    let status = q.status.unwrap_or_else(|| "pendiente".to_string());
    if !["pendiente", "aprobada", "descartada"].contains(&status.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Estado invalido".to_string()));
    }
    let user = session.username.clone();
    db_op(&state.db, move |conn| store::list_proposals(conn, &user, &status)).await.map(Json)
}

/// Cambios que el usuario hace antes de aprobar
#[derive(Debug, Default, Deserialize)]
pub struct ProposalEdits {
    pub title: Option<String>,
    pub description: Option<String>,
    pub date: Option<String>,
    pub time: Option<String>,
    pub end_time: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Approved {
    pub kind: String,
    pub id: String,
    pub message: String,
}

/// POST /api/email/proposals/{id}/approve (cuerpo opcional con ediciones)
pub async fn approve_proposal_handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    body: Option<Json<ProposalEdits>>,
) -> Result<Json<Approved>, (StatusCode, String)> {
    let edits = body.map(|Json(e)| e).unwrap_or_default();
    approve(&state, &session.username, &id, edits).await.map(Json)
}

/// POST /api/email/proposals/{id}/dismiss
pub async fn dismiss_proposal_handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    dismiss(&state, &session.username, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn dismiss(state: &AppState, username: &str, id: &str) -> Result<(), (StatusCode, String)> {
    let (u, i) = (username.to_string(), id.to_string());
    let ok = db_op(&state.db, move |conn| store::resolve_proposal(conn, &u, &i, "descartada", None)).await?;
    if !ok {
        return Err((StatusCode::NOT_FOUND, "Propuesta no encontrada o ya resuelta".to_string()));
    }
    publish_count(state, username).await;
    Ok(())
}

/// Crea el evento o la tarea de una propuesta pendiente del usuario
pub async fn approve(
    state: &AppState,
    username: &str,
    id: &str,
    edits: ProposalEdits,
) -> Result<Approved, (StatusCode, String)> {
    let (u, i) = (username.to_string(), id.to_string());
    let result = crate::db::db_op_status(&state.db, move |conn| {
        let bad = |m: &str| (StatusCode::BAD_REQUEST, m.to_string());
        let internal = |e: String| (StatusCode::INTERNAL_SERVER_ERROR, e);

        let p = store::get_proposal(conn, &u, &i)
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Propuesta no encontrada".to_string()))?;
        if p.status != "pendiente" {
            return Err((StatusCode::CONFLICT, format!("La propuesta ya fue {}", p.status)));
        }

        let title = edits.title.unwrap_or(p.title).trim().to_string();
        if title.is_empty() || title.chars().count() > 200 {
            return Err(bad("Titulo invalido"));
        }
        let description = edits.description.unwrap_or(p.description);
        let date = pick(edits.date, p.date);
        let time = pick(edits.time, p.time);
        let end_time = pick(edits.end_time, p.end_time);
        let location = pick(edits.location, p.location);
        if date.as_deref().is_some_and(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err()) {
            return Err(bad("Fecha invalida (AAAA-MM-DD)"));
        }
        for t in [&time, &end_time].into_iter().flatten() {
            if NaiveTime::parse_from_str(t, "%H:%M").is_err() {
                return Err(bad("Hora invalida (HH:MM)"));
            }
        }

        let mut origin = String::new();
        if let (Some(s), Some(f)) = (&p.email_subject, &p.email_from) {
            origin = format!("\n\nDesde el correo \"{}\" de {}", s, f);
        }
        let description = format!("{}{}", description.trim(), origin).trim().to_string();
        let now = Utc::now().to_rfc3339();
        let new_id = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();

        let tx = conn.unchecked_transaction().map_err(|e| internal(e.to_string()))?;
        let message = if p.kind == "evento" {
            let (Some(date), Some(time)) = (&date, &time) else {
                return Err(bad("Un evento necesita fecha y hora"));
            };
            let end_time = end_time.filter(|e| e > time);
            tx.execute(
                "INSERT INTO calendar_events (id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '[]', '[]', '[]', 15, 0, 1, '', NULL, ?9, NULL)",
                params![new_id, title, description, date, time, end_time, location, u, now],
            )
            .map_err(|e| internal(e.to_string()))?;
            format!("Evento agendado: {} ({} {})", title, date, time)
        } else {
            let assigned = serde_json::to_string(&[&u]).unwrap_or_else(|_| "[]".into());
            let time = date.as_ref().and(time);
            tx.execute(
                "INSERT INTO tasks (id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder)
                 VALUES (?1, NULL, ?2, ?3, ?4, 'pendiente', ?5, ?6, ?7, 0, 0, 8, '[]', '[]', ?8, NULL)",
                params![new_id, title, description, assigned, u, date, time, now],
            )
            .map_err(|e| internal(e.to_string()))?;
            match &date {
                Some(d) => format!("Tarea creada: {} (vence {})", title, d),
                None => format!("Tarea creada: {}", title),
            }
        };
        if !store::resolve_proposal(&tx, &u, &i, "aprobada", Some(&new_id)).map_err(internal)? {
            return Err((StatusCode::CONFLICT, "La propuesta ya fue resuelta".to_string()));
        }
        tx.commit().map_err(|e| internal(e.to_string()))?;
        Ok(Approved { kind: p.kind, id: new_id, message })
    })
    .await?;

    state.log_activity("propuesta_aprobada", &result.message, username).await;
    publish_count(state, username).await;
    Ok(result)
}

/// Un valor editado vacio borra el campo; ausente deja el de la propuesta
fn pick(edit: Option<String>, original: Option<String>) -> Option<String> {
    match edit {
        Some(v) if v.trim().is_empty() => None,
        Some(v) => Some(v.trim().to_string()),
        None => original,
    }
}

/// Avisa a las pestañas del usuario cuantas propuestas pendientes tiene
pub async fn publish_count(state: &AppState, username: &str) {
    let u = username.to_string();
    let count = db_op(&state.db, move |conn| store::count_pending(conn, &u)).await.unwrap_or(0);
    state.events.publish(
        "email.proposals",
        serde_json::json!({ "count": count }),
        crate::events::Audience::User(username.to_string()),
        Some("email"),
    );
}
