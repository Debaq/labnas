//! Proteccion contra SSRF: peticiones salientes cuyo destino lo elige un usuario
//! (p.ej. "descargar desde URL") solo pueden ir a internet, nunca a localhost,
//! la LAN, Tailscale (100.64/10) ni direcciones link-local/metadatos.
//!
//! El filtro va en el resolver DNS del cliente, asi cubre tambien cada redirect
//! y el DNS rebinding (se valida la IP a la que realmente se conecta). Las URL
//! con IP literal no pasan por el resolver: las revisa `check_url` y la
//! politica de redirects.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

const MAX_REDIRECTS: usize = 5;

/// ¿Es una IP enrutable en internet? (equivale a `IpAddr::is_global`, aun inestable)
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let seg0 = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg0 & 0xfe00) == 0xfc00 // fc00::/7 unique local
                || (seg0 & 0xffc0) == 0xfe80 // fe80::/10 link-local
                || (seg0 == 0x2001 && v6.segments()[1] == 0x0db8) // documentacion
                || (seg0 == 0x0064 && v6.segments()[1] == 0xff9b)) // NAT64: puede apuntar a IPv4 privada
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64/10: CGNAT y Tailscale
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0/24
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18/15 benchmarking
        || o[0] >= 240)
}

/// Valida esquema y, si el host es una IP literal, que sea publica
pub fn check_url(url: &reqwest::Url) -> Result<(), String> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("Solo se permiten URLs http(s)".into());
    }
    match url.host() {
        None => Err("URL sin host".into()),
        Some(url::Host::Ipv4(ip)) if !is_public_v4(ip) => Err("Direccion no permitida (red local)".into()),
        Some(url::Host::Ipv6(ip)) if !is_public_ip(IpAddr::V6(ip)) => {
            Err("Direccion no permitida (red local)".into())
        }
        _ => Ok(()),
    }
}

/// Resolver que descarta las IPs no publicas
struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|a| is_public_ip(a.ip()))
                .collect();
            if addrs.is_empty() {
                return Err(format!("{} no resuelve a una direccion publica", host).into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Cliente HTTP para URLs elegidas por usuarios: solo internet publica
pub fn public_client() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .dns_resolver(Arc::new(PublicOnlyResolver))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    attempt.error("Demasiadas redirecciones")
                } else if let Err(e) = check_url(attempt.url()) {
                    attempt.error(e)
                } else {
                    attempt.follow()
                }
            }))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("cliente HTTP publico")
    });
    &CLIENT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn rechaza_redes_internas() {
        for s in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.50.114", "169.254.169.254",
            "100.100.1.1", "0.0.0.0", "255.255.255.255", "::1", "fd00::1", "fe80::1",
            "::ffff:192.168.1.1", "64:ff9b::a00:1",
        ] {
            assert!(!is_public_ip(ip(s)), "{} deberia ser interna", s);
        }
    }

    #[test]
    fn acepta_internet() {
        for s in ["1.1.1.1", "140.82.112.3", "2606:4700:4700::1111", "100.128.0.1"] {
            assert!(is_public_ip(ip(s)), "{} deberia ser publica", s);
        }
    }

    #[test]
    fn check_url_ip_literal() {
        let u = |s: &str| reqwest::Url::parse(s).unwrap();
        assert!(check_url(&u("http://127.0.0.1:3001/api")).is_err());
        assert!(check_url(&u("http://[::1]/")).is_err());
        assert!(check_url(&u("ftp://example.com/x")).is_err());
        assert!(check_url(&u("https://example.com/x.iso")).is_ok());
        assert!(check_url(&u("http://1.1.1.1/")).is_ok());
    }
}
