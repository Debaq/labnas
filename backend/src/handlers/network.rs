use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    time::{Duration, Instant},
};

use crate::config::save_config;
use crate::models::network::{KnownDevice, LabelRequest, NetworkHost};
use crate::state::AppState;

pub async fn scan_network(
    State(state): State<AppState>,
) -> Result<Json<Vec<NetworkHost>>, (StatusCode, String)> {
    let local_ip = local_ip_address::local_ip().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("No se pudo obtener IP local: {}", e),
        )
    })?;

    let subnet_base = match local_ip {
        IpAddr::V4(ipv4) => {
            let octets = ipv4.octets();
            format!("{}.{}.{}", octets[0], octets[1], octets[2])
        }
        _ => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Solo IPv4 soportado".to_string(),
            ))
        }
    };

    // Ping all hosts
    let mut handles = Vec::new();
    for i in 1..=254u8 {
        let ip_str = format!("{}.{}", subnet_base, i);
        let ip: Ipv4Addr = ip_str.parse().unwrap();
        handles.push(tokio::spawn(async move { ping_host(ip).await }));
    }

    let mut hosts = Vec::new();
    for handle in handles {
        if let Ok(host) = handle.await {
            hosts.push(host);
        }
    }

    hosts.retain(|h| h.is_alive);

    // Get MAC addresses from ARP table
    let mac_map = get_arp_table().await;

    // Load known devices
    let config = state.config.lock().await;
    let known = &config.known_devices;

    // Enrich hosts with MAC, vendor, known status
    for host in &mut hosts {
        if let Some(mac) = mac_map.get(&host.ip) {
            host.mac = Some(mac.clone());
            host.vendor = mac_vendor(mac);
            // Check if known
            let mac_upper = mac.to_uppercase();
            if let Some(device) = known.iter().find(|d| d.mac.to_uppercase() == mac_upper) {
                host.is_known = true;
                host.label = Some(device.label.clone());
            }
        }
    }

    // Find new unknown devices to alert
    let new_unknown: Vec<&NetworkHost> = hosts
        .iter()
        .filter(|h| !h.is_known && h.mac.is_some())
        .collect();

    // Get previous scan to detect truly new devices
    let prev_hosts = state.scanned_hosts.lock().await;
    let prev_macs: Vec<String> = prev_hosts
        .iter()
        .filter_map(|h| h.mac.clone())
        .collect();
    drop(prev_hosts);

    let brand_new: Vec<&&NetworkHost> = new_unknown
        .iter()
        .filter(|h| {
            h.mac
                .as_ref()
                .map(|m| !prev_macs.contains(m))
                .unwrap_or(false)
        })
        .collect();

    // Alert via Telegram for brand new unknown devices
    if !brand_new.is_empty() {
        let token = config.notifications.bot_token.clone();
        let chats = config.notifications.telegram_chats.clone();
        drop(config);

        if let Some(token) = token {
            if !chats.is_empty() {
                let mut msg = String::from("*Nuevo dispositivo en la red*\n");
                for h in &brand_new {
                    let mac = h.mac.as_deref().unwrap_or("?");
                    let vendor = h.vendor.as_deref().unwrap_or("Desconocido");
                    let hostname = h.hostname.as_deref().unwrap_or("-");
                    msg.push_str(&format!(
                        "\nIP: `{}`\nMAC: `{}`\nFabricante: {}\nHostname: {}\n",
                        h.ip, mac, vendor, hostname
                    ));
                }
                let client = &state.http_client;
                for chat in &chats {
                    let _ = send_tg(client, &token, chat.chat_id, &msg).await;
                }
            }
        }
    } else {
        drop(config);
    }

    hosts.sort_by(|a, b| {
        let a_ip: Ipv4Addr = a.ip.parse().unwrap_or(Ipv4Addr::UNSPECIFIED);
        let b_ip: Ipv4Addr = b.ip.parse().unwrap_or(Ipv4Addr::UNSPECIFIED);
        a_ip.cmp(&b_ip)
    });

    let mut stored = state.scanned_hosts.lock().await;
    *stored = hosts.clone();

    Ok(Json(hosts))
}

pub async fn get_hosts(State(state): State<AppState>) -> Json<Vec<NetworkHost>> {
    let hosts = state.scanned_hosts.lock().await;
    Json(hosts.clone())
}

// --- Label / unlabel devices ---

pub async fn label_host(
    State(state): State<AppState>,
    Path(mac): Path<String>,
    Json(req): Json<LabelRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mac_upper = mac.to_uppercase();
    let mut config = state.config.lock().await;

    let label = req.label;

    // Update or add
    if let Some(existing) = config
        .known_devices
        .iter_mut()
        .find(|d| d.mac.to_uppercase() == mac_upper)
    {
        existing.label = label.clone();
    } else {
        config.known_devices.push(KnownDevice {
            mac: mac_upper.clone(),
            label: label.clone(),
        });
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Update in-memory hosts
    let mut hosts = state.scanned_hosts.lock().await;
    for host in hosts.iter_mut() {
        if host.mac.as_ref().map(|m| m.to_uppercase()) == Some(mac_upper.clone()) {
            host.is_known = true;
            host.label = Some(label.clone());
        }
    }

    Ok(StatusCode::OK)
}

pub async fn unlabel_host(
    State(state): State<AppState>,
    Path(mac): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mac_upper = mac.to_uppercase();
    let mut config = state.config.lock().await;

    let before = config.known_devices.len();
    config
        .known_devices
        .retain(|d| d.mac.to_uppercase() != mac_upper);

    if config.known_devices.len() == before {
        return Err((StatusCode::NOT_FOUND, "Dispositivo no encontrado".to_string()));
    }

    save_config(&config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Update in-memory hosts
    let mut hosts = state.scanned_hosts.lock().await;
    for host in hosts.iter_mut() {
        if host.mac.as_ref().map(|m| m.to_uppercase()) == Some(mac_upper.clone()) {
            host.is_known = false;
            host.label = None;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- Helpers ---

async fn ping_host(ip: Ipv4Addr) -> NetworkHost {
    let ip_str = ip.to_string();
    let addr = IpAddr::V4(ip);
    let start = Instant::now();

    let client = match surge_ping::Client::new(&surge_ping::Config::default()) {
        Ok(c) => c,
        Err(_) => {
            return NetworkHost {
                ip: ip_str,
                hostname: None,
                mac: None,
                vendor: None,
                is_alive: false,
                is_known: false,
                label: None,
                last_seen: Utc::now(),
                response_time_ms: None,
            };
        }
    };

    let payload = [0u8; 8];
    let mut pinger = client
        .pinger(addr, surge_ping::PingIdentifier(rand_id()))
        .await;
    pinger.timeout(Duration::from_millis(1500));

    let is_alive = matches!(
        pinger.ping(surge_ping::PingSequence(0), &payload).await,
        Ok(_)
    );

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let hostname = if is_alive {
        let addr_clone = addr;
        tokio::task::spawn_blocking(move || dns_lookup::lookup_addr(&addr_clone).ok())
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    NetworkHost {
        ip: ip_str,
        hostname,
        mac: None,
        vendor: None,
        is_alive,
        is_known: false,
        label: None,
        last_seen: Utc::now(),
        response_time_ms: if is_alive {
            Some((elapsed * 100.0).round() / 100.0)
        } else {
            None
        },
    }
}

fn rand_id() -> u16 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let s = RandomState::new();
    let mut h = s.build_hasher();
    h.write_u8(0);
    h.finish() as u16
}

async fn get_arp_table() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Try `ip neigh` first
    if let Ok(output) = tokio::process::Command::new("ip")
        .args(["neigh", "show"])
        .output()
        .await
    {
        let text = String::from_utf8_lossy(&output.stdout);
        // Format: "192.168.1.1 dev wlan0 lladdr aa:bb:cc:dd:ee:ff REACHABLE"
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(lladdr_idx) = parts.iter().position(|&p| p == "lladdr") {
                if let (Some(ip), Some(mac)) = (parts.first(), parts.get(lladdr_idx + 1)) {
                    map.insert(ip.to_string(), mac.to_uppercase());
                }
            }
        }
    }

    map
}

async fn send_tg(client: &reqwest::Client, token: &str, chat_id: i64, text: &str) -> Result<(), ()> {
    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
    let _ = client
        .post(&url)
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "Markdown"
        }))
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    Ok(())
}

// --- MAC Vendor lookup ---

fn mac_vendor(mac: &str) -> Option<String> {
    let prefix = mac
        .to_uppercase()
        .replace(['-', '.'], ":")
        .chars()
        .take(8)
        .collect::<String>();

    let vendor = match prefix.as_str() {
        // Apple
        p if p.starts_with("00:1C:B3") || p.starts_with("3C:22:FB") || p.starts_with("A4:83:E7")
            || p.starts_with("AC:DE:48") || p.starts_with("F0:18:98") || p.starts_with("14:7D:DA")
            || p.starts_with("D0:03:4B") || p.starts_with("A8:5C:2C") || p.starts_with("78:7B:8A")
            || p.starts_with("DC:A9:04") || p.starts_with("F0:D4:F6") || p.starts_with("8C:85:90")
            || p.starts_with("3C:E0:72") || p.starts_with("BC:D0:74") || p.starts_with("28:6A:BA")
            || p.starts_with("E0:B5:2D") || p.starts_with("7C:D1:C3") => "Apple",
        // Samsung
        p if p.starts_with("00:26:37") || p.starts_with("84:25:DB") || p.starts_with("CC:07:AB")
            || p.starts_with("A0:82:1F") || p.starts_with("50:01:D9") || p.starts_with("C0:BD:D1")
            || p.starts_with("8C:F5:A3") || p.starts_with("BC:72:B1") || p.starts_with("94:35:0A")
            || p.starts_with("34:23:BA") => "Samsung",
        // Xiaomi
        p if p.starts_with("28:6C:07") || p.starts_with("64:CC:2E") || p.starts_with("78:11:DC")
            || p.starts_with("9C:99:A0") || p.starts_with("0C:1D:AF") || p.starts_with("74:23:44")
            || p.starts_with("50:64:2B") || p.starts_with("AC:C1:EE") || p.starts_with("FC:64:BA")
            || p.starts_with("58:A0:23") => "Xiaomi",
        // Intel
        p if p.starts_with("00:1B:21") || p.starts_with("3C:97:0E") || p.starts_with("8C:EC:4B")
            || p.starts_with("DC:71:96") || p.starts_with("48:51:B7") || p.starts_with("A4:34:D9")
            || p.starts_with("70:CF:49") || p.starts_with("9C:29:76") || p.starts_with("34:CF:F6")
            || p.starts_with("80:86:F2") => "Intel",
        // Realtek
        p if p.starts_with("00:E0:4C") || p.starts_with("52:54:00") || p.starts_with("00:1A:3F")
            || p.starts_with("48:5B:39") || p.starts_with("00:0C:E7") => "Realtek",
        // TP-Link
        p if p.starts_with("50:C7:BF") || p.starts_with("E8:48:B8") || p.starts_with("C0:06:C3")
            || p.starts_with("14:EB:B6") || p.starts_with("60:32:B1") || p.starts_with("98:DA:C4")
            || p.starts_with("B0:4E:26") || p.starts_with("AC:84:C6") => "TP-Link",
        // Huawei
        p if p.starts_with("00:E0:FC") || p.starts_with("48:46:FB") || p.starts_with("CC:A2:23")
            || p.starts_with("88:3F:D3") || p.starts_with("70:8C:B6") || p.starts_with("D4:6E:5C")
            || p.starts_with("5C:C3:07") || p.starts_with("AC:CF:85") => "Huawei",
        // Dell
        p if p.starts_with("00:14:22") || p.starts_with("F8:BC:12") || p.starts_with("18:DB:F2")
            || p.starts_with("D4:BE:D9") || p.starts_with("B0:83:FE") || p.starts_with("34:17:EB") => "Dell",
        // HP
        p if p.starts_with("00:1E:0B") || p.starts_with("3C:D9:2B") || p.starts_with("10:60:4B")
            || p.starts_with("94:57:A5") || p.starts_with("A0:D3:C1") || p.starts_with("FC:15:B4") => "HP",
        // Lenovo
        p if p.starts_with("00:06:1B") || p.starts_with("E8:6A:64") || p.starts_with("50:5B:C2")
            || p.starts_with("98:FA:9B") || p.starts_with("54:E1:AD") || p.starts_with("C8:5B:76") => "Lenovo",
        // Raspberry Pi
        p if p.starts_with("B8:27:EB") || p.starts_with("DC:A6:32") || p.starts_with("E4:5F:01")
            || p.starts_with("28:CD:C1") || p.starts_with("D8:3A:DD") => "Raspberry Pi",
        // Google
        p if p.starts_with("F4:F5:D8") || p.starts_with("54:60:09") || p.starts_with("30:FD:38")
            || p.starts_with("A4:77:33") || p.starts_with("44:07:0B") => "Google",
        // Amazon
        p if p.starts_with("44:65:0D") || p.starts_with("68:54:FD") || p.starts_with("40:B4:CD")
            || p.starts_with("F0:F0:A4") || p.starts_with("74:C2:46") => "Amazon",
        // Microsoft
        p if p.starts_with("00:15:5D") || p.starts_with("28:18:78") || p.starts_with("7C:1E:52")
            || p.starts_with("DC:53:60") => "Microsoft",
        // ASUS
        p if p.starts_with("00:1A:92") || p.starts_with("1C:87:2C") || p.starts_with("2C:FD:A1")
            || p.starts_with("04:92:26") || p.starts_with("AC:9E:17") => "ASUS",
        // Netgear
        p if p.starts_with("00:1F:33") || p.starts_with("28:C6:8E") || p.starts_with("C4:3D:C7")
            || p.starts_with("B0:B9:8A") => "Netgear",
        // Cisco / Linksys
        p if p.starts_with("00:1A:A2") || p.starts_with("00:23:69") || p.starts_with("58:6D:8F")
            || p.starts_with("E8:BA:70") => "Cisco",
        // Sony
        p if p.starts_with("00:1D:0D") || p.starts_with("FC:0F:E6") || p.starts_with("78:C8:81")
            || p.starts_with("A8:E3:EE") => "Sony",
        // LG
        p if p.starts_with("00:1E:75") || p.starts_with("10:68:3F") || p.starts_with("CC:2D:83")
            || p.starts_with("A8:23:FE") => "LG",
        // Motorola
        p if p.starts_with("00:1A:66") || p.starts_with("D8:96:85") || p.starts_with("EC:22:80") => "Motorola",
        // Espressif (ESP32/ESP8266)
        p if p.starts_with("24:0A:C4") || p.starts_with("30:AE:A4") || p.starts_with("CC:50:E3")
            || p.starts_with("AC:67:B2") || p.starts_with("A4:CF:12") || p.starts_with("24:6F:28")
            || p.starts_with("84:CC:A8") || p.starts_with("B4:E6:2D") || p.starts_with("EC:FA:BC") => "Espressif (ESP)",
        // D-Link
        p if p.starts_with("00:26:5A") || p.starts_with("28:10:7B") || p.starts_with("1C:7E:E5")
            || p.starts_with("C8:D3:A3") => "D-Link",
        // Ubiquiti
        p if p.starts_with("04:18:D6") || p.starts_with("24:5A:4C") || p.starts_with("FC:EC:DA")
            || p.starts_with("78:8A:20") || p.starts_with("B4:FB:E4") => "Ubiquiti",
        // MikroTik
        p if p.starts_with("00:0C:42") || p.starts_with("48:8F:5A") || p.starts_with("D4:01:C3")
            || p.starts_with("6C:3B:6B") || p.starts_with("CC:2D:E0") => "MikroTik",
        // Qualcomm
        p if p.starts_with("00:03:7F") || p.starts_with("78:02:F8") => "Qualcomm",
        // MediaTek
        p if p.starts_with("00:0C:E7") || p.starts_with("00:08:A2") => "MediaTek",
        // VMware
        p if p.starts_with("00:0C:29") || p.starts_with("00:50:56") => "VMware",
        // Creality (3D printers)
        p if p.starts_with("68:E7:4A") => "Creality",
        // Prusa
        p if p.starts_with("10:9C:70") => "Prusa",
        _ => return None,
    };

    Some(vendor.to_string())
}
