//! Registro en `<datos locales>/labnas-tray/labnas-tray.log` (en Windows no hay consola).
//! Se guarda la sesion actual y la anterior (`.old`).

use std::io::Write;
use std::sync::Mutex;

static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

pub fn path() -> Option<std::path::PathBuf> {
    dirs::data_local_dir().map(|d| d.join("labnas-tray").join("labnas-tray.log"))
}

pub fn init() {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::rename(&p, p.with_extension("log.old"));
    if let Ok(f) = std::fs::File::create(&p)
        && let Ok(mut g) = FILE.lock()
    {
        *g = Some(f);
    }
}

pub fn write(msg: &str) {
    eprintln!("[labnas-tray] {msg}");
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    if let Ok(mut g) = FILE.lock()
        && let Some(f) = g.as_mut()
    {
        let _ = writeln!(f, "{h:02}:{m:02}:{s:02}Z {msg}");
    }
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { $crate::logfile::write(&format!($($t)*)) };
}
