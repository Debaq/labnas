//! Historial de temperaturas exportable (idea 7): CSV de lecturas e informe resumido
//! (minimo, maximo, promedio, tiempo fuera de rango por alerta) para auditorias.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use chrono::{DateTime, Local, NaiveDate, SecondsFormat, TimeZone, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::db_op_status;
use crate::error::{internal, ApiError};
use crate::models::sensors::{row_to_alert, ALERT_COLS};
use crate::state::{AppState, SessionInfo};

const MAX_DIAS: i64 = 366;
/// Tope de un hueco entre lecturas al estimar el tiempo fuera de rango
const MAX_HUECO_MIN: f64 = 60.0;

#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub format: Option<String>,
}

/// Limites UTC (RFC 3339) del rango: desde las 00:00 locales de `from` hasta el fin de `to`
fn bounds(q: &RangeQuery) -> Result<(String, String), ApiError> {
    let parse = |s: &str| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|_| (StatusCode::BAD_REQUEST, "Fecha invalida (use AAAA-MM-DD)".to_string()))
    };
    let from = parse(&q.from)?;
    let to = parse(&q.to)?;
    if to < from {
        return Err((StatusCode::BAD_REQUEST, "El rango esta invertido".into()));
    }
    if (to - from).num_days() + 1 > MAX_DIAS {
        return Err((StatusCode::BAD_REQUEST, format!("El rango maximo es de {} dias", MAX_DIAS)));
    }
    let start = Local
        .from_local_datetime(&from.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Fecha invalida".to_string()))?;
    let end = Local
        .from_local_datetime(&to.succ_opt().unwrap_or(to).and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Fecha invalida".to_string()))?;
    let f = |d: DateTime<Local>| d.with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Secs, true);
    // Las lecturas se guardan como "...+00:00": normalizar los limites a ese formato
    Ok((f(start).replace('Z', "+00:00"), f(end).replace('Z', "+00:00")))
}

fn device_name(conn: &rusqlite::Connection, id: &str) -> Result<String, ApiError> {
    conn.query_row("SELECT name FROM sensor_devices WHERE id = ?1", params![id], |r| r.get::<_, String>(0))
        .map_err(|_| (StatusCode::NOT_FOUND, "Sensor no encontrado".to_string()))
}

fn local_text(ts: &str) -> String {
    DateTime::parse_from_rfc3339(ts)
        .map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|_| ts.to_string())
}

fn csv_cell(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@']) && s.parse::<f64>().is_err() { format!("'{}", s) } else { s.to_string() };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

pub async fn export_csv(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Query(q): Query<RangeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if let Some(f) = &q.format {
        if f != "csv" {
            return Err((StatusCode::BAD_REQUEST, "Formato no soportado (solo csv)".into()));
        }
    }
    let (from, to) = bounds(&q)?;
    let (name, rows) = db_op_status(&state.db, move |conn| {
        let name = device_name(conn, &id)?;
        let mut stmt = conn
            .prepare(
                "SELECT timestamp, key, value, unit FROM sensor_readings
                 WHERE device_id = ?1 AND timestamp >= ?2 AND timestamp < ?3 ORDER BY timestamp ASC",
            )
            .map_err(internal)?;
        let rows = stmt
            .query_map(params![id, from, to], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?, r.get::<_, String>(3)?))
            })
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal)?;
        Ok((name, rows))
    })
    .await?;

    state.log_access(&format!("Exporto historial de {}", name), &session.username).await;

    // BOM para que Excel lea UTF-8
    let mut out = String::from("\u{feff}fecha_hora_local,clave,valor,unidad\r\n");
    for (ts, key, val, unit) in rows {
        out.push_str(&format!("{},{},{},{}\r\n", local_text(&ts), csv_cell(&key), val, csv_cell(&unit)));
    }
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let file = format!("historial-{}-{}-a-{}.csv", safe, q.from, q.to);
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8"));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", file)) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok((headers, out))
}

#[derive(Serialize)]
pub struct KeySummary {
    pub key: String,
    pub unit: String,
    pub count: i64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub first_at: String,
    pub last_at: String,
}

#[derive(Serialize)]
pub struct AlertSummary {
    pub alert_id: String,
    pub key: String,
    pub condition: String,
    pub threshold: f64,
    pub critical: bool,
    /// Minutos aproximados fuera de rango (hueco entre lecturas, tope 60 min)
    pub minutes_out_of_range: f64,
    pub readings_out_of_range: i64,
    pub incidents: i64,
}

#[derive(Serialize)]
pub struct Report {
    pub device_id: String,
    pub device_name: String,
    pub from: String,
    pub to: String,
    pub generated_at: String,
    pub total_readings: i64,
    pub keys: Vec<KeySummary>,
    pub alerts: Vec<AlertSummary>,
}

pub async fn report(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Report>, ApiError> {
    let (from, to) = bounds(&q)?;
    let (qf, qt) = (q.from.clone(), q.to.clone());
    let rep = db_op_status(&state.db, move |conn| {
        let name = device_name(conn, &id)?;
        let mut stmt = conn
            .prepare(
                "SELECT key, MAX(unit), COUNT(*), MIN(value), MAX(value), AVG(value), MIN(timestamp), MAX(timestamp)
                 FROM sensor_readings WHERE device_id = ?1 AND timestamp >= ?2 AND timestamp < ?3
                 GROUP BY key ORDER BY key",
            )
            .map_err(internal)?;
        let keys = stmt
            .query_map(params![id, from, to], |r| {
                Ok(KeySummary {
                    key: r.get(0)?,
                    unit: r.get(1)?,
                    count: r.get(2)?,
                    min: r.get(3)?,
                    max: r.get(4)?,
                    avg: r.get(5)?,
                    first_at: local_text(&r.get::<_, String>(6)?),
                    last_at: local_text(&r.get::<_, String>(7)?),
                })
            })
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal)?;

        let mut astmt = conn
            .prepare(&format!("SELECT {} FROM sensor_alerts WHERE device_id = ?1 ORDER BY key", ALERT_COLS))
            .map_err(internal)?;
        let alerts = astmt
            .query_map(params![id], row_to_alert)
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal)?;

        let mut summaries = Vec::new();
        for a in alerts {
            let mut rs = conn
                .prepare(
                    "SELECT timestamp, value FROM sensor_readings
                     WHERE device_id = ?1 AND key = ?2 AND timestamp >= ?3 AND timestamp < ?4 ORDER BY timestamp ASC",
                )
                .map_err(internal)?;
            let pts: Vec<(DateTime<Utc>, f64)> = rs
                .query_map(params![id, a.key, from, to], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))
                .map_err(internal)?
                .filter_map(|r| r.ok())
                .filter_map(|(t, v)| DateTime::parse_from_rfc3339(&t).ok().map(|d| (d.with_timezone(&Utc), v)))
                .collect();
            let out = |v: f64| if a.condition == "above" { v > a.threshold } else { v < a.threshold };
            let (mut minutes, mut n) = (0.0_f64, 0_i64);
            for (i, (t, v)) in pts.iter().enumerate() {
                if out(*v) {
                    n += 1;
                    if let Some((next, _)) = pts.get(i + 1) {
                        minutes += ((*next - *t).num_seconds() as f64 / 60.0).clamp(0.0, MAX_HUECO_MIN);
                    }
                }
            }
            let incidents: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sensor_incidents WHERE alert_id = ?1 AND started_at >= ?2 AND started_at < ?3",
                    params![a.id, from, to],
                    |r| r.get(0),
                )
                .map_err(internal)?;
            summaries.push(AlertSummary {
                alert_id: a.id,
                key: a.key,
                condition: a.condition,
                threshold: a.threshold,
                critical: a.critical,
                minutes_out_of_range: (minutes * 10.0).round() / 10.0,
                readings_out_of_range: n,
                incidents,
            });
        }
        Ok(Report {
            device_id: id,
            device_name: name,
            from: qf,
            to: qt,
            generated_at: Local::now().format("%Y-%m-%d %H:%M").to_string(),
            total_readings: keys.iter().map(|k| k.count).sum(),
            keys,
            alerts: summaries,
        })
    })
    .await?;
    state.log_access(&format!("Exporto historial de {}", rep.device_name), &session.username).await;
    Ok(Json(rep))
}
