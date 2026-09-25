//! Recordatorios de tareas/eventos y reporte diario

use super::*;

// =====================
// Reminder loop for insistent tasks
// =====================

/// Fila de evento de calendario leida para los recordatorios
pub(super) type CalendarEventRow = (String, String, String, String, String, String, String, u32, bool, bool, String, Option<String>);

pub async fn task_reminder_loop(state: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await; // Check every minute

        let conn = match crate::db::get_conn(&state.db) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[Reminders] Error DB: {}", e);
                continue;
            }
        };

        let token = match read_bot_token(&conn) {
            Ok(Some(t)) => t,
            _ => continue,
        };
        let chats = read_all_chats(&conn).unwrap_or_default();

        let now = chrono::Utc::now();
        let local_now = chrono::Local::now();
        let today = local_now.format("%Y-%m-%d").to_string();
        let mut to_remind: Vec<(String, Vec<i64>)> = Vec::new();

        // Read tasks that need reminders
        {
            let mut stmt = match conn.prepare(
                "SELECT id, title, status, assigned_to, created_by, due_date, requires_confirmation, insistent, reminder_minutes, confirmed_by, last_reminder FROM tasks WHERE status NOT IN ('completada', 'rechazada')"
            ) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let rows = match stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, bool>(7)?,
                    row.get::<_, u32>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            }) {
                Ok(r) => r,
                Err(_) => continue,
            };

            for r in rows {
                let Ok((id, title, _status, assigned_json, created_by, due_date, requires_confirmation, insistent, reminder_minutes, confirmed_json, last_reminder_str)) = r else {
                    continue;
                };

                let assigned: Vec<String> = serde_json::from_str(&assigned_json).unwrap_or_default();
                let confirmed: Vec<String> = serde_json::from_str(&confirmed_json).unwrap_or_default();

                let is_due_today = due_date.as_ref().map(|d| d.as_str() == today.as_str()).unwrap_or(false);
                let is_overdue = due_date.as_ref().map(|d| d.as_str() < today.as_str()).unwrap_or(false);

                if !requires_confirmation && !insistent && !is_due_today && !is_overdue {
                    continue;
                }

                let interval = if insistent || requires_confirmation {
                    (reminder_minutes as i64) * 60
                } else {
                    720 * 60
                };

                let last_reminder: Option<chrono::DateTime<chrono::Utc>> = last_reminder_str
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc));

                let should_remind = last_reminder
                    .map(|lr| (now - lr).num_seconds() >= interval)
                    .unwrap_or(true);

                if !should_remind {
                    continue;
                }

                let target_ids: Vec<i64> = if assigned.contains(&"all".to_string()) {
                    chats
                        .iter()
                        .filter(|c| {
                            (c.role == UserRole::Admin || c.role == UserRole::Operador)
                                && !confirmed.contains(&c.name)
                        })
                        .map(|c| c.chat_id)
                        .collect()
                } else {
                    chats
                        .iter()
                        .filter(|c| {
                            (assigned.iter().any(|a| a.to_lowercase() == c.name.to_lowercase())
                                || c.linked_web_user.as_ref().map(|u| {
                                    assigned.iter().any(|a| a.to_lowercase() == u.to_lowercase())
                                }).unwrap_or(false))
                                && !confirmed.contains(&c.name)
                        })
                        .map(|c| c.chat_id)
                        .collect()
                };

                if target_ids.is_empty() {
                    continue;
                }

                let urgency = if is_overdue {
                    "🚨 *TAREA VENCIDA*\n\n"
                } else if is_due_today {
                    "⚠️ *Vence hoy*\n\n"
                } else {
                    ""
                };
                let icon = if insistent { "🔔" } else { "📋" };
                let due_info = due_date
                    .as_ref()
                    .map(|d| format!("\nVence: {}", d))
                    .unwrap_or_default();
                let msg = format!(
                    "{}{} *Recordatorio*\n\nTarea: *{}*\nID: `{}`\nPor: {}{}\n\n`/confirmar {}` o `/rechazar {}`",
                    urgency, icon, title, id, created_by, due_info, id, id
                );

                to_remind.push((msg, target_ids));

                // Update last_reminder
                let now_str = now.to_rfc3339();
                let _ = conn.execute(
                    "UPDATE tasks SET last_reminder = ?1 WHERE id = ?2",
                    params![now_str, id],
                );
            }
        }

        // Check calendar events
        let now_min = local_now.hour() * 60 + local_now.minute();

        // Collect calendar event data into a Vec to avoid borrow conflicts
        let calendar_events: Vec<CalendarEventRow> = {
            let mut events = Vec::new();
            if let Ok(mut stmt) = conn.prepare(
                "SELECT id, title, description, date, time, created_by, invitees, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end FROM calendar_events WHERE date = ?1"
            ) {
                if let Ok(rows) = stmt.query_map(params![today], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, u32>(7)?,
                        row.get::<_, bool>(8)?,
                        row.get::<_, bool>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, Option<String>>(11)?,
                    ))
                }) {
                    for event in rows.flatten() {
                        events.push(event);
                    }
                }
            }
            events
        };

        for (id, title, description, date, time, created_by, invitees_json, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end) in &calendar_events {
            if *reminded || !notify_telegram {
                // Check recurring advancement for already-reminded events
                if !recurrence.is_empty() && recurrence != "none" && *reminded {
                    let parts: Vec<&str> = time.split(':').collect();
                    if parts.len() == 2 {
                        if let (Ok(eh), Ok(em)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                            let event_min = eh * 60 + em;
                            if event_min <= now_min {
                                if let Ok(parsed_date) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                                    use chrono::Datelike;
                                    let next_date = match recurrence.as_str() {
                                        "daily" => Some(parsed_date + chrono::Duration::days(1)),
                                        "weekly" => Some(parsed_date + chrono::Duration::weeks(1)),
                                        "monthly" => {
                                            let m = if parsed_date.month() == 12 { 1 } else { parsed_date.month() + 1 };
                                            let y = if parsed_date.month() == 12 { parsed_date.year() + 1 } else { parsed_date.year() };
                                            chrono::NaiveDate::from_ymd_opt(y, m, parsed_date.day().min(28))
                                        }
                                        "weekdays" => {
                                            // Avanzar al próximo día hábil (Lun-Vie)
                                            let mut d = parsed_date + chrono::Duration::days(1);
                                            while d.weekday().num_days_from_monday() >= 5 {
                                                d += chrono::Duration::days(1);
                                            }
                                            Some(d)
                                        }
                                        r if r.starts_with("days:") => {
                                            // "days:0,2,4" = Lun,Mie,Vie (num_days_from_monday)
                                            let target_days: Vec<u32> = r[5..].split(',')
                                                .filter_map(|s| s.trim().parse().ok())
                                                .collect();
                                            if target_days.is_empty() {
                                                None
                                            } else {
                                                let mut d = parsed_date + chrono::Duration::days(1);
                                                for _ in 0..7 {
                                                    if target_days.contains(&d.weekday().num_days_from_monday()) {
                                                        break;
                                                    }
                                                    d += chrono::Duration::days(1);
                                                }
                                                Some(d)
                                            }
                                        }
                                        _ => None,
                                    };

                                    if let Some(next) = next_date {
                                        let next_str = next.format("%Y-%m-%d").to_string();
                                        let within_range = recurrence_end.as_ref()
                                            .map(|end| next_str.as_str() <= end.as_str())
                                            .unwrap_or(true);

                                        if within_range {
                                            let _ = conn.execute(
                                                "UPDATE calendar_events SET date = ?1, reminded = 0, accepted = '[]', declined = '[]' WHERE id = ?2",
                                                params![next_str, id],
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                continue;
            }

            let parts: Vec<&str> = time.split(':').collect();
            if parts.len() != 2 {
                continue;
            }
            let (eh, em) = match (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                (Ok(h), Ok(m)) => (h, m),
                _ => continue,
            };
            let event_min = eh * 60 + em;
            let remind_at = event_min.saturating_sub(*remind_before_min);

            if now_min >= remind_at && now_min < event_min {
                let mins_left = event_min.saturating_sub(now_min);
                let msg = format!(
                    "📅 *Evento en {} min*\n\n*{}*\nHora: {}\n{}",
                    mins_left, title, time,
                    if description.is_empty() { String::new() } else { format!("\n{}", description) }
                );

                let invitees: Vec<String> = serde_json::from_str(invitees_json).unwrap_or_default();
                let targets: Vec<i64> = if invitees.contains(&"all".to_string()) {
                    chats.iter().filter(|c| c.role != UserRole::Pendiente).map(|c| c.chat_id).collect()
                } else {
                    let mut ids: Vec<i64> = chats.iter()
                        .filter(|c| invitees.iter().any(|i| i.to_lowercase() == c.name.to_lowercase()) || c.name == *created_by)
                        .map(|c| c.chat_id)
                        .collect();
                    ids.dedup();
                    ids
                };

                to_remind.push((msg, targets));
                let _ = conn.execute(
                    "UPDATE calendar_events SET reminded = 1 WHERE id = ?1",
                    params![id],
                );
            }
        }

        drop(conn);

        // Send all reminders
        for (msg, ids) in &to_remind {
            for id in ids {
                let _ = send_telegram_message(&state.http_client, &token, *id, msg).await;
            }
        }
    }
}


// =====================
// Daily scheduler
// =====================

pub async fn daily_notification_loop(state: AppState) {
    // Track last sent date per chat_id
    let mut last_sent: std::collections::HashMap<i64, chrono::NaiveDate> = std::collections::HashMap::new();

    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        let (token, chats) = {
            let conn = match crate::db::get_conn(&state.db) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let token = read_bot_token(&conn).unwrap_or(None);
            let chats = read_all_chats(&conn).unwrap_or_default();
            (token, chats)
        };

        let Some(token) = token else { continue };

        let now = chrono::Local::now();
        let today = now.date_naive();
        let current_hour = now.hour() as u8;
        let current_minute = now.minute() as u8;

        for chat in &chats {
            if !chat.daily_enabled {
                continue;
            }

            // Already sent today?
            if last_sent.get(&chat.chat_id) == Some(&today) {
                continue;
            }

            if current_hour == chat.daily_hour && current_minute >= chat.daily_minute {
                let message = build_status_message(&state).await;
                let activity = build_activity_message(&state).await;
                let full_msg = format!("{}\n\n---\n{}", message, activity);

                if send_telegram_message(&state.http_client, &token, chat.chat_id, &full_msg).await.is_ok() {
                    last_sent.insert(chat.chat_id, today);
                    println!("[LabNAS] Reporte diario enviado a {}", chat.name);
                }
            }
        }
    }
}
