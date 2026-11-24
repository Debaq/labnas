//! "Iniciar con el sistema": Linux con ~/.config/autostart, Windows con la clave Run
//! del registro del usuario (sin permisos de administrador).

#[cfg(target_os = "linux")]
fn desktop_file() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("autostart/labnas-tray.desktop"))
}

#[cfg(target_os = "linux")]
pub fn enabled() -> bool {
    desktop_file().is_some_and(|p| p.exists())
}

#[cfg(target_os = "linux")]
pub fn set(on: bool) -> Result<(), String> {
    let path = desktop_file().ok_or("sin carpeta de configuracion")?;
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=LabNAS (bandeja)\nComment=Control de la musica de LabNAS\nExec=\"{}\"\nIcon=io.github.debaq.LabNAS\nX-GNOME-Autostart-enabled=true\nTerminal=false\n",
        exe.display()
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output()
}

#[cfg(windows)]
pub fn enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", "LabNAS-Tray"]).map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(windows)]
pub fn set(on: bool) -> Result<(), String> {
    let out = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\"", exe.display());
        reg(&["add", RUN_KEY, "/v", "LabNAS-Tray", "/t", "REG_SZ", "/d", value.as_str(), "/f"])
    } else {
        reg(&["delete", RUN_KEY, "/v", "LabNAS-Tray", "/f"])
    }
    .map_err(|e| e.to_string())?;
    if out.status.success() || !on { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn enabled() -> bool {
    false
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn set(_on: bool) -> Result<(), String> {
    Err("no disponible en este sistema".into())
}
