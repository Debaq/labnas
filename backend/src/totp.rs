//! TOTP (RFC 6238, HMAC-SHA1, 6 digitos, pasos de 30 s) compatible con
//! Google Authenticator, Aegis, 2FAS, etc.

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

pub const STEP_SECS: i64 = 30;
const DIGITS: u32 = 6;
/// Pasos de tolerancia por reloj desfasado (±30 s)
const SKEW: i64 = 1;
const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("getrandom");
    buf
}

/// Secreto nuevo (160 bits) en base32, como lo espera la app
pub fn new_secret() -> String {
    base32_encode(&random_bytes::<20>())
}

pub fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(B32[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let v = B32.iter().position(|&x| x as char == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

fn hotp(key: &[u8], counter: u64) -> u32 {
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(key).expect("HMAC acepta cualquier largo");
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let off = (h[h.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([h[off] & 0x7f, h[off + 1], h[off + 2], h[off + 3]]);
    bin % 10u32.pow(DIGITS)
}

pub fn code_at(secret_b32: &str, step: i64) -> Option<String> {
    let key = base32_decode(secret_b32)?;
    Some(format!("{:0width$}", hotp(&key, step as u64), width = DIGITS as usize))
}

pub fn current_step() -> i64 {
    crate::state::now_unix() / STEP_SECS
}

/// Paso en que el codigo es valido (dentro de ±SKEW) y posterior a `last_step`
/// (un codigo ya usado no sirve de nuevo).
pub fn verify(secret_b32: &str, code: &str, now_step: i64, last_step: i64) -> Option<i64> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() != DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    (now_step - SKEW..=now_step + SKEW)
        .filter(|s| *s > last_step)
        .find(|s| code_at(secret_b32, *s).is_some_and(|c| constant_eq(c.as_bytes(), code.as_bytes())))
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// URI para la app (y el QR)
pub fn otpauth_uri(issuer: &str, account: &str, secret_b32: &str) -> String {
    let label = urlencoding::encode(&format!("{}:{}", issuer, account)).into_owned();
    format!(
        "otpauth://totp/{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
        label,
        secret_b32,
        urlencoding::encode(issuer),
        DIGITS,
        STEP_SECS
    )
}

pub fn qr_svg(data: &str) -> Result<String, String> {
    let code = qrcode::QrCode::new(data.as_bytes()).map_err(|e| e.to_string())?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .quiet_zone(true)
        .build())
}

/// Codigo de recuperacion legible: xxxxx-xxxxx (50 bits)
pub fn new_recovery_code() -> String {
    let s = base32_encode(&random_bytes::<7>()).to_lowercase();
    format!("{}-{}", &s[..5], &s[5..10])
}

pub fn normalize_recovery(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

pub fn hash_recovery(code: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(normalize_recovery(code).as_bytes());
    d.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238, apendice B (SHA1, secreto "12345678901234567890")
    const RFC_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn vectores_rfc6238() {
        let secret = base32_encode(RFC_SECRET);
        for (t, expected) in [(59, "94287082"), (1111111109, "07081804"), (1234567890, "89005924"), (2000000000, "69279037")] {
            // el RFC usa 8 digitos: los 6 ultimos son el codigo de 6
            assert_eq!(code_at(&secret, t / STEP_SECS).unwrap(), expected[2..], "t={}", t);
        }
    }

    #[test]
    fn base32_ida_y_vuelta() {
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32_decode("mzxw6ytboi").unwrap(), b"foobar");
        let s = new_secret();
        assert_eq!(s.len(), 32);
        assert_eq!(base32_decode(&s).unwrap().len(), 20);
        assert!(base32_decode("no-valido!").is_none());
    }

    #[test]
    fn verifica_ventana_y_no_reuso() {
        let s = new_secret();
        let now = 1_000_000;
        let prev = code_at(&s, now - 1).unwrap();
        assert_eq!(verify(&s, &prev, now, 0), Some(now - 1));
        assert_eq!(verify(&s, &code_at(&s, now + 1).unwrap(), now, 0), Some(now + 1));
        assert_eq!(verify(&s, &code_at(&s, now - 2).unwrap(), now, 0), None);
        // ya usado
        assert_eq!(verify(&s, &prev, now, now - 1), None);
        assert_eq!(verify(&s, "12345", now, 0), None);
        assert_eq!(verify(&s, "abcdef", now, 0), None);
    }

    #[test]
    fn recuperacion_normalizada() {
        let c = new_recovery_code();
        assert_eq!(c.len(), 11);
        assert_eq!(hash_recovery(&c), hash_recovery(&c.to_uppercase().replace('-', " ")));
        assert!(otpauth_uri("LabNAS", "ana", "ABC").starts_with("otpauth://totp/LabNAS%3Aana?secret=ABC&issuer=LabNAS"));
        assert!(qr_svg("otpauth://totp/x").unwrap().contains("<svg"));
    }
}
