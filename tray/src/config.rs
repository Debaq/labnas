//! Configuracion en `<config>/labnas-tray/` (Linux: ~/.config, Windows: %APPDATA%):
//! - `config`: lineas `clave=valor` (url, token, user)
//! - `tls-pins`: lineas `host huella` (certificados fijados la primera vez)

use std::path::PathBuf;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Config {
    /// Direccion del LabNAS (ya resuelta a HTTPS si el servidor redirige)
    pub url: Option<String>,
    /// Token de dispositivo (`lnd_...`)
    pub token: Option<String>,
    /// Usuario a cuyo nombre esta el token
    pub user: Option<String>,
}

fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("labnas-tray"))
}

fn parse(content: &str) -> Config {
    let mut c = Config::default();
    for line in content.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim();
        if v.is_empty() {
            continue;
        }
        match k.trim() {
            "url" => c.url = Some(v.to_string()),
            "token" => c.token = Some(v.to_string()),
            "user" => c.user = Some(v.to_string()),
            _ => {}
        }
    }
    c
}

fn render(c: &Config) -> String {
    let mut s = String::from("# labnas-tray: se reescribe solo\n");
    for (k, v) in [("url", &c.url), ("token", &c.token), ("user", &c.user)] {
        if let Some(v) = v {
            s.push_str(&format!("{k}={v}\n"));
        }
    }
    s
}

pub fn load() -> Config {
    dir().and_then(|d| std::fs::read_to_string(d.join("config")).ok()).map(|s| parse(&s)).unwrap_or_default()
}

pub fn save(c: &Config) {
    let Some(d) = dir() else { return };
    let _ = std::fs::create_dir_all(&d);
    write_private(&d.join("config"), &render(c));
}

/// El archivo lleva el token: solo lo lee el usuario
fn write_private(path: &std::path::Path, content: &str) {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path);
        match f {
            Ok(mut f) => {
                let _ = f.write_all(content.as_bytes());
            }
            Err(e) => eprintln!("[labnas-tray] no se pudo guardar {}: {e}", path.display()),
        }
    }
    #[cfg(not(unix))]
    {
        if let Err(e) = std::fs::write(path, content) {
            eprintln!("[labnas-tray] no se pudo guardar {}: {e}", path.display());
        }
    }
}

// ─── Huellas de certificados (mismo formato que labnas-viewer) ───

#[derive(Debug, PartialEq)]
pub enum PinCheck {
    Trusted,
    New,
    /// Hay otra huella guardada para el host (la devuelve)
    Changed(String),
}

pub fn check_pin(pins: &str, host: &str, fp: &str) -> PinCheck {
    let saved = pins
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once(' '))
        .find(|(h, _)| *h == host)
        .map(|(_, f)| f.trim());
    match saved {
        Some(f) if f.eq_ignore_ascii_case(fp) => PinCheck::Trusted,
        Some(f) => PinCheck::Changed(f.to_string()),
        None => PinCheck::New,
    }
}

pub fn forget_pin(pins: &str, host: &str) -> String {
    pins.lines()
        .filter(|l| l.split_once(' ').map(|(h, _)| h) != Some(host))
        .map(|l| format!("{l}\n"))
        .collect()
}

pub fn read_pins() -> String {
    dir().and_then(|d| std::fs::read_to_string(d.join("tls-pins")).ok()).unwrap_or_default()
}

pub fn save_pin(host: &str, fp: &str) {
    let Some(d) = dir() else { return };
    let _ = std::fs::create_dir_all(&d);
    let mut pins = forget_pin(&read_pins(), host);
    pins.push_str(&format!("{host} {fp}\n"));
    write_private(&d.join("tls-pins"), &pins);
}

/// Borra la huella guardada (tras confirmar que el certificado cambio de verdad)
pub fn remove_pin(host: &str) {
    let Some(d) = dir() else { return };
    write_private(&d.join("tls-pins"), &forget_pin(&read_pins(), host));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_y_vuelta() {
        let c = Config { url: Some("https://x:3443".into()), token: Some("lnd_ab".into()), user: None };
        assert_eq!(parse(&render(&c)), c);
        assert_eq!(parse("basura\nurl=\n"), Config::default());
    }

    #[test]
    fn huellas() {
        let pins = "# comentario\nlabnas.local AA:BB\n10.0.0.2 CC:DD\n";
        assert_eq!(check_pin(pins, "labnas.local", "aa:bb"), PinCheck::Trusted);
        assert_eq!(check_pin(pins, "10.0.0.2", "AA:BB"), PinCheck::Changed("CC:DD".into()));
        assert_eq!(check_pin(pins, "otro", "AA:BB"), PinCheck::New);
        assert!(!forget_pin(pins, "10.0.0.2").contains("CC:DD"));
    }
}
