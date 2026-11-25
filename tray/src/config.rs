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
    /// Avisar con una notificacion cuando cambia la cancion
    pub notify: bool,
    /// Avisos de LabNAS: los personales y los de administracion vienen activados, los del
    /// laboratorio (todas las impresoras, sensores...) no
    pub quiet_personal: bool,
    pub lab: bool,
    pub quiet_admin: bool,
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
            "notify" => c.notify = v == "1",
            "avisos_personales" => c.quiet_personal = v == "0",
            "avisos_laboratorio" => c.lab = v == "1",
            "avisos_admin" => c.quiet_admin = v == "0",
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
    for (on, line) in [
        (c.notify, "notify=1"),
        (c.quiet_personal, "avisos_personales=0"),
        (c.lab, "avisos_laboratorio=1"),
        (c.quiet_admin, "avisos_admin=0"),
    ] {
        if on {
            s.push_str(line);
            s.push('\n');
        }
    }
    s
}

/// Ya hay configuracion guardada (si no, es la primera vez que se abre)
pub fn exists() -> bool {
    dir().is_some_and(|d| d.join("config").exists())
}

/// Marca del bloque de configuracion que LabNAS agrega al final del .exe al
/// descargarlo (ver backend `handlers/music/tray_download.rs`)
const TRAILER_MAGIC: &[u8; 8] = b"LNTCONF1";

/// `url=...` del bloque al final de un ejecutable, si lo tiene
pub fn trailer_conf(bytes_tail: &[u8]) -> Option<Config> {
    let n = bytes_tail.len();
    if n < 12 || &bytes_tail[n - 12..n - 4] != TRAILER_MAGIC {
        return None;
    }
    let len = u32::from_le_bytes(bytes_tail[n - 4..].try_into().ok()?) as usize;
    let start = (n - 12).checked_sub(len)?;
    Some(parse(std::str::from_utf8(&bytes_tail[start..n - 12]).ok()?))
}

/// Direccion que trae esta copia: al final del ejecutable (descarga de Windows) o en
/// `labnas-tray.conf` junto a el (tarball de Linux)
pub fn bundled_url() -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let exe = std::env::current_exe().ok()?;
    if let Ok(mut f) = std::fs::File::open(&exe) {
        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
        let take = size.min(4096);
        if take >= 12 && f.seek(SeekFrom::End(-(take as i64))).is_ok() {
            let mut tail = vec![0u8; take as usize];
            if f.read_exact(&mut tail).is_ok()
                && let Some(url) = trailer_conf(&tail).and_then(|c| c.url)
            {
                return Some(url);
            }
        }
    }
    let conf = exe.parent()?.join("labnas-tray.conf");
    parse(&std::fs::read_to_string(conf).ok()?).url
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
        let c = Config { url: Some("https://x:3443".into()), token: Some("lnd_ab".into()), user: None, ..Config::default() };
        assert_eq!(parse(&render(&c)), c);
        let c = Config { user: Some("ana".into()), notify: true, lab: true, quiet_admin: true, ..c };
        assert_eq!(parse(&render(&c)), c);
        assert_eq!(parse("basura\nurl=\n"), Config::default());
    }

    #[test]
    fn bloque_al_final_del_exe() {
        let conf = "# comentario\nurl=https://10.0.0.5:3443\n";
        let mut exe = b"MZ binario".to_vec();
        exe.extend_from_slice(conf.as_bytes());
        exe.extend_from_slice(TRAILER_MAGIC);
        exe.extend_from_slice(&(conf.len() as u32).to_le_bytes());
        assert_eq!(trailer_conf(&exe).and_then(|c| c.url).as_deref(), Some("https://10.0.0.5:3443"));
        assert!(trailer_conf(b"MZ sin bloque").is_none());
        // largo mayor que lo disponible
        let mut bad = b"x".to_vec();
        bad.extend_from_slice(TRAILER_MAGIC);
        bad.extend_from_slice(&999u32.to_le_bytes());
        assert!(trailer_conf(&bad).is_none());
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
