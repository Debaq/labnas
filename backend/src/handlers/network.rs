use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use std::{
    net::{IpAddr, Ipv4Addr},
    time::{Duration, Instant},
};

use crate::models::network::NetworkHost;
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
    hosts.sort_by(|a, b| {
        let a_ip: Ipv4Addr = a.ip.parse().unwrap_or(Ipv4Addr::UNSPECIFIED);
        let b_ip: Ipv4Addr = b.ip.parse().unwrap_or(Ipv4Addr::UNSPECIFIED);
        a_ip.cmp(&b_ip)
    });

    let mut stored = state.scanned_hosts.lock().await;
    *stored = hosts.clone();

    Ok(Json(hosts))
}

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
                is_alive: false,
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
        is_alive,
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

pub async fn get_hosts(State(state): State<AppState>) -> Json<Vec<NetworkHost>> {
    let hosts = state.scanned_hosts.lock().await;
    Json(hosts.clone())
}
