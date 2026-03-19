use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::time::Duration;
use sysinfo::{Disks, System};

use chrono::Timelike;

use crate::config::save_config;
use crate::models::notifications::*;
use crate::state::AppState;

// --- Contacts ---

pub async fn list_contacts(State(state): State<AppState>) -> Json<NotificationConfig> {
    let config = state.config.lock().await;
    Json(config.notifications.clone())
}

pub async fn add_contact(
    State(state): State<AppState>,
    Json(req): Json<AddContactRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;

    // Avoid duplicates
    if config
        .notifications
        .whatsapp_contacts
        .iter()
        .any(|c| c.phone == req.phone)
    {
        return Err((StatusCode::CONFLICT, "Contacto ya existe".to_string()));
    }

    config.notifications.whatsapp_contacts.push(WhatsAppContact {
        name: req.name,
        phone: req.phone,
        apikey: req.apikey,
    });

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::CREATED)
}

pub async fn delete_contact(
    State(state): State<AppState>,
    Path(phone): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    let before = config.notifications.whatsapp_contacts.len();
    config
        .notifications
        .whatsapp_contacts
        .retain(|c| c.phone != phone);

    if config.notifications.whatsapp_contacts.len() == before {
        return Err((StatusCode::NOT_FOUND, "Contacto no encontrado".to_string()));
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::NO_CONTENT)
}

// --- Schedule ---

pub async fn set_schedule(
    State(state): State<AppState>,
    Json(req): Json<ScheduleRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut config = state.config.lock().await;
    config.notifications.daily_enabled = req.daily_enabled;
    config.notifications.daily_hour = req.daily_hour.min(23);
    config.notifications.daily_minute = req.daily_minute.min(59);

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::OK)
}

// --- Send ---

pub async fn send_test(
    State(state): State<AppState>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let config = state.config.lock().await;
    let contacts = config.notifications.whatsapp_contacts.clone();
    drop(config);

    if contacts.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "No hay contactos configurados".to_string(),
        ));
    }

    let message = build_status_message(&state).await;
    let mut sent = 0;
    let mut errors = Vec::new();

    for contact in &contacts {
        match send_whatsapp(&state.http_client, &contact.phone, &contact.apikey, &message).await {
            Ok(_) => sent += 1,
            Err(e) => errors.push(format!("{}: {}", contact.name, e)),
        }
        // CallMeBot rate limit: wait between messages
        if contacts.len() > 1 {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    if errors.is_empty() {
        Ok((
            StatusCode::OK,
            format!("Mensaje enviado a {} contacto(s)", sent),
        ))
    } else {
        Ok((
            StatusCode::OK,
            format!(
                "Enviado a {}. Errores: {}",
                sent,
                errors.join(", ")
            ),
        ))
    }
}

// --- WhatsApp via CallMeBot ---

pub async fn send_whatsapp(
    client: &reqwest::Client,
    phone: &str,
    apikey: &str,
    message: &str,
) -> Result<(), String> {
    let resp = client
        .get("https://api.callmebot.com/whatsapp.php")
        .query(&[("phone", phone), ("text", message), ("apikey", apikey)])
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Error de conexion: {}", e))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("CallMeBot respondio {}: {}", status, body))
    }
}

// --- Status message ---

pub async fn build_status_message(state: &AppState) -> String {
    let uptime = state.start_time.elapsed();
    let secs = uptime.as_secs();
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;

    let uptime_str = if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else {
        format!("{}h {}m", hours, mins)
    };

    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "?".to_string());

    // Disk info
    let disk_info = tokio::task::spawn_blocking(|| {
        let disks = Disks::new_with_refreshed_list();
        let total: u64 = disks.list().iter().map(|d| d.total_space()).sum();
        let available: u64 = disks.list().iter().map(|d| d.available_space()).sum();
        let used = total.saturating_sub(available);
        let pct = if total > 0 {
            (used as f64 / total as f64 * 100.0) as u64
        } else {
            0
        };
        format!(
            "{}% usado ({} / {})",
            pct,
            format_bytes(used),
            format_bytes(total)
        )
    })
    .await
    .unwrap_or_else(|_| "N/A".to_string());

    // System info
    let sys_info = tokio::task::spawn_blocking(|| {
        let mut sys = System::new_all();
        sys.refresh_all();
        let hostname = System::host_name().unwrap_or_else(|| "?".to_string());
        let mem_used = format_bytes(sys.used_memory());
        let mem_total = format_bytes(sys.total_memory());
        (hostname, format!("{} / {}", mem_used, mem_total))
    })
    .await
    .unwrap_or_else(|_| ("?".to_string(), "N/A".to_string()));

    // Network hosts
    let hosts = state.scanned_hosts.lock().await;
    let active_hosts = hosts.iter().filter(|h| h.is_alive).count();
    drop(hosts);

    // 3D Printers
    let config = state.config.lock().await;
    let printer_count = config.printers3d.len();
    drop(config);

    format!(
        "*LabNAS - Reporte Diario*\n\
         \n\
         Host: {}\n\
         IP: {}\n\
         Uptime: {}\n\
         \n\
         Disco: {}\n\
         RAM: {}\n\
         Red: {} hosts activos\n\
         Impresoras 3D: {} configuradas",
        sys_info.0, local_ip, uptime_str, disk_info, sys_info.1, active_hosts, printer_count
    )
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let k = 1024_f64;
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    let i = (bytes as f64).log(k).floor() as usize;
    let i = i.min(sizes.len() - 1);
    format!("{:.1} {}", bytes as f64 / k.powi(i as i32), sizes[i])
}

// --- Daily scheduler ---

pub async fn daily_notification_loop(state: AppState) {
    let mut last_sent_date: Option<chrono::NaiveDate> = None;

    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        let config = state.config.lock().await;
        let notif = config.notifications.clone();
        drop(config);

        if !notif.daily_enabled || notif.whatsapp_contacts.is_empty() {
            continue;
        }

        let now = chrono::Local::now();
        let today = now.date_naive();

        if last_sent_date == Some(today) {
            continue;
        }

        if now.hour() == notif.daily_hour as u32 && now.minute() >= notif.daily_minute as u32 {
            let message = build_status_message(&state).await;

            for contact in &notif.whatsapp_contacts {
                let _ = send_whatsapp(
                    &state.http_client,
                    &contact.phone,
                    &contact.apikey,
                    &message,
                )
                .await;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }

            last_sent_date = Some(today);
            println!(
                "[LabNAS] Mensaje diario enviado a {} contacto(s)",
                notif.whatsapp_contacts.len()
            );
        }
    }
}
