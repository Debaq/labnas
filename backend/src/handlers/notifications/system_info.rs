//! /estado, /discos, /ram, /cpu, /uptime, /red, /actividad

use super::*;

pub(super) async fn build_activity_message(state: &AppState) -> String {
    let events = crate::handlers::audit::recent(&state.db, 15).await;

    if events.is_empty() {
        return "*Actividad*\n\nNo hay actividad registrada aun.".to_string();
    }

    let mut msg = String::from("*Actividad reciente*\n");
    for event in events.iter().rev() {
        let time = chrono::DateTime::parse_from_rfc3339(&event.timestamp)
            .map(|t| t.with_timezone(&chrono::Local).format("%d/%m %H:%M").to_string())
            .unwrap_or_default();
        msg.push_str(&format!("\n`{}` {} - {} ({})", time, event.action, event.details, event.username));
    }

    msg
}

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
        format!("{}% ({} / {})", pct, format_bytes(used), format_bytes(total))
    })
    .await
    .unwrap_or_else(|_| "N/A".to_string());

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

    let hosts = state.scanned_hosts.lock().await;
    let active_hosts = hosts.iter().filter(|h| h.is_alive).count();
    drop(hosts);

    let printer_count = {
        let conn = crate::db::get_conn(&state.db);
        match conn {
            Ok(c) => {
                c.query_row("SELECT COUNT(*) FROM printers3d", [], |row| row.get::<_, i64>(0)).unwrap_or(0)
            }
            Err(_) => 0,
        }
    };

    format!(
        "*LabNAS - Reporte*\n\n\
         Host: `{}`\n\
         IP: `{}`\n\
         Uptime: {}\n\n\
         Disco: {}\n\
         RAM: {}\n\
         Red: {} hosts activos\n\
         Impresoras 3D: {}",
        sys_info.0, local_ip, uptime_str, disk_info, sys_info.1, active_hosts, printer_count
    )
}

pub(super) async fn build_disks_message() -> String {
    tokio::task::spawn_blocking(|| {
        let disks = Disks::new_with_refreshed_list();
        let mut msg = String::from("*Discos*\n");

        for d in disks.list() {
            let mount = d.mount_point().to_string_lossy();
            let total = d.total_space();
            let available = d.available_space();
            let used = total.saturating_sub(available);
            let pct = if total > 0 {
                (used as f64 / total as f64 * 100.0) as u64
            } else {
                0
            };
            let bar = progress_bar(pct as f64);

            msg.push_str(&format!(
                "\n`{}`\n{} {}%\n{} / {}\n",
                mount,
                bar,
                pct,
                format_bytes(used),
                format_bytes(total),
            ));
        }
        msg
    })
    .await
    .unwrap_or_else(|_| "Error obteniendo discos".to_string())
}

pub(super) async fn build_ram_message() -> String {
    tokio::task::spawn_blocking(|| {
        let mut sys = System::new_all();
        sys.refresh_all();
        let total = sys.total_memory();
        let used = sys.used_memory();
        let pct = if total > 0 {
            (used as f64 / total as f64 * 100.0) as u64
        } else {
            0
        };
        let bar = progress_bar(pct as f64);
        let swap_total = sys.total_swap();
        let swap_used = sys.used_swap();

        format!(
            "*RAM*\n\n\
             {} {}%\n\
             {} / {}\n\n\
             Swap: {} / {}",
            bar,
            pct,
            format_bytes(used),
            format_bytes(total),
            format_bytes(swap_used),
            format_bytes(swap_total),
        )
    })
    .await
    .unwrap_or_else(|_| "Error obteniendo RAM".to_string())
}

pub(super) async fn build_cpu_message() -> String {
    tokio::task::spawn_blocking(|| {
        let mut sys = System::new_all();
        sys.refresh_all();
        // Need a small delay for CPU usage to be accurate
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu_all();

        let cpu_count = sys.cpus().len();
        let global_usage = sys.global_cpu_usage();
        let cpu_name = sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "?".to_string());
        let bar = progress_bar(global_usage as f64);

        format!(
            "*CPU*\n\n\
             {} {:.0}%\n\
             {} nucleos\n\
             `{}`",
            bar, global_usage, cpu_count, cpu_name
        )
    })
    .await
    .unwrap_or_else(|_| "Error obteniendo CPU".to_string())
}

pub(super) fn build_uptime_message(state: &AppState) -> String {
    let uptime = state.start_time.elapsed();
    let secs = uptime.as_secs();
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;

    let uptime_str = if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    };

    format!("*Uptime*\n\nLabNAS lleva encendido: {}", uptime_str)
}

pub(super) async fn build_network_message(state: &AppState) -> String {
    let hosts = state.scanned_hosts.lock().await;
    let alive: Vec<_> = hosts.iter().filter(|h| h.is_alive).collect();

    if alive.is_empty() {
        return "*Red*\n\nNo hay hosts detectados.\nEjecuta un escaneo desde la UI primero."
            .to_string();
    }

    let mut msg = format!("*Red* - {} hosts activos\n", alive.len());
    for h in &alive {
        let name = h
            .hostname
            .as_deref()
            .unwrap_or("?");
        let ms = h
            .response_time_ms
            .map(|ms| format!(" ({}ms)", ms))
            .unwrap_or_default();
        msg.push_str(&format!("\n`{}` - {}{}", h.ip, name, ms));
    }
    msg
}

pub(super) async fn build_printers_message(state: &AppState) -> String {
    let printers = {
        let conn = match crate::db::get_conn(&state.db) {
            Ok(c) => c,
            Err(_) => return "Error de base de datos.".to_string(),
        };
        read_printers(&conn).unwrap_or_default()
    };

    if printers.is_empty() {
        return "*Impresoras 3D*\n\nNo hay impresoras configuradas.".to_string();
    }

    let mut msg = format!("*Impresoras 3D* - {}\n", printers.len());
    for p in &printers {
        let ptype = match p.printer_type {
            crate::models::printers3d::Printer3DType::OctoPrint => "OctoPrint",
            crate::models::printers3d::Printer3DType::Moonraker => "Moonraker",
            crate::models::printers3d::Printer3DType::CrealityStock => "Creality",
            crate::models::printers3d::Printer3DType::FlashForge => "FlashForge",
        };
        msg.push_str(&format!("\n`{}` - {} ({}:{})", p.name, ptype, p.ip, p.port));
    }
    msg
}
