//! Actualizacion segura del binario + frontend desde GitHub Releases.
//!
//! Flujo de `install`:
//!   1. Descarga `labnas-<tag>-linux-<arch>.tar.gz` y su `.sha256` (sin checksum no se instala)
//!   2. Extrae en `<instalacion>/.update-tmp` (mismo disco => rename atomico)
//!   3. Verifica que el binario nuevo arranque en esta maquina (`--version`)
//!   4. Mueve lo actual a `<instalacion>/.rollback/` y lo nuevo a su lugar
//!   5. Deja `.rollback/.pending`; si el binario nuevo cae 2 veces al arrancar,
//!      el siguiente arranque restaura la version anterior (`startup_check`)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const GITHUB_REPO: &str = "Debaq/labnas";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

const ROLLBACK_DIR: &str = ".rollback";
const TMP_DIR: &str = ".update-tmp";
const PENDING_FILE: &str = ".pending";
/// Arranques fallidos tolerados antes de restaurar la version anterior
const MAX_START_ATTEMPTS: u32 = 2;
/// Tiempo que la version nueva debe mantenerse viva para darse por buena
pub const CONFIRM_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug, Serialize, Deserialize)]
struct Pending {
    from: String,
    to: String,
    attempts: u32,
}

/// Assets del servidor en un release
#[derive(Debug, Clone)]
pub struct ReleaseAssets {
    pub tag: String,
    pub tarball_url: String,
    pub sha256_url: Option<String>,
}

// ─── Consulta de releases ───

/// Arquitectura tal como aparece en el nombre del tarball del release
pub fn release_arch() -> &'static str {
    match std::env::consts::ARCH {
        "arm" => "armv7",
        other => other,
    }
}

pub fn server_asset_name(tag: &str) -> String {
    format!("labnas-{}-linux-{}.tar.gz", tag, release_arch())
}

fn asset_url(release: &serde_json::Value, name: &str) -> Option<String> {
    release["assets"]
        .as_array()?
        .iter()
        .find(|a| a["name"].as_str() == Some(name))
        .and_then(|a| a["browser_download_url"].as_str().map(|s| s.to_string()))
}

/// Tarball exacto del servidor para esta arquitectura (+ su checksum si existe).
pub fn assets_for(release: &serde_json::Value) -> Option<ReleaseAssets> {
    let tag = release["tag_name"].as_str()?.to_string();
    let name = server_asset_name(&tag);
    Some(ReleaseAssets {
        tarball_url: asset_url(release, &name)?,
        sha256_url: asset_url(release, &format!("{}.sha256", name)),
        tag,
    })
}

/// JSON de un release: el ultimo (`tag = None`) o uno especifico.
pub async fn fetch_release(client: &reqwest::Client, tag: Option<&str>) -> Option<serde_json::Value> {
    let url = match tag {
        Some(t) => format!("https://api.github.com/repos/{}/releases/tags/{}", GITHUB_REPO, t),
        None => format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO),
    };
    let resp = client
        .get(&url)
        .header("User-Agent", "LabNAS")
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json().await.ok()
}

// ─── Rutas de la instalacion ───

static EXE_PATH: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

/// Ruta del ejecutable instalado, fijada al arrancar.
/// No usar `current_exe()` despues de un intercambio: en Linux sigue al *inode* en
/// ejecucion, que tras actualizar vive en `.rollback/`.
pub fn exe_path() -> Result<PathBuf, String> {
    EXE_PATH
        .get_or_init(|| std::env::current_exe().ok().and_then(|p| std::fs::canonicalize(p).ok()))
        .clone()
        .ok_or_else(|| "No se pudo determinar el ejecutable".to_string())
}

/// Llamar al inicio de main(), antes de cualquier intercambio de archivos.
pub fn init() {
    let _ = exe_path();
}

pub fn install_dir() -> Result<PathBuf, String> {
    exe_path()?
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "No se pudo determinar el directorio de instalacion".to_string())
}

fn exe_name() -> String {
    exe_path()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "labnas-backend".to_string())
}

// ─── Instalacion ───

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

/// Primer token de un archivo `sha256sum` ("<hash>  <archivo>")
fn parse_sha256_file(text: &str) -> Option<String> {
    let hash = text.split_whitespace().next()?.to_ascii_lowercase();
    (hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then_some(hash)
}

async fn download(client: &reqwest::Client, url: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "LabNAS")
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("Error descargando: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub respondio {}", resp.status()));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Error leyendo descarga: {}", e))
}

/// Descarga, verifica e instala un release. Tras `Ok`, hay que reiniciar el proceso.
pub async fn install(client: &reqwest::Client, assets: &ReleaseAssets) -> Result<(), String> {
    let sha_url = assets
        .sha256_url
        .as_ref()
        .ok_or("El release no incluye checksum SHA-256; no se instala por seguridad")?;

    let expected = parse_sha256_file(&String::from_utf8_lossy(
        &download(client, sha_url, Duration::from_secs(30)).await?,
    ))
    .ok_or("Checksum SHA-256 con formato invalido")?;

    let tarball = download(client, &assets.tarball_url, Duration::from_secs(300)).await?;
    let actual = sha256_hex(&tarball);
    if actual != expected {
        return Err(format!(
            "Checksum no coincide (esperado {}, obtenido {}); descarga corrupta o alterada",
            expected, actual
        ));
    }

    let dir = install_dir()?;
    let version = assets.tag.trim_start_matches('v').to_string();
    tokio::task::spawn_blocking(move || install_tarball(&dir, &tarball, &version))
        .await
        .map_err(|e| e.to_string())?
}

fn install_tarball(dir: &Path, tarball: &[u8], version: &str) -> Result<(), String> {
    let tmp = dir.join(TMP_DIR);
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("No se pudo crear {}: {}", tmp.display(), e))?;

    let result = (|| {
        let tar_path = tmp.join("labnas.tar.gz");
        std::fs::write(&tar_path, tarball).map_err(|e| e.to_string())?;
        let out = std::process::Command::new("tar")
            .arg("xzf")
            .arg(&tar_path)
            .arg("-C")
            .arg(&tmp)
            .output()
            .map_err(|e| format!("Error ejecutando tar: {}", e))?;
        if !out.status.success() {
            return Err(format!("Error extrayendo: {}", String::from_utf8_lossy(&out.stderr)));
        }

        let new_dir = tmp.join("labnas");
        verify_new_binary(&new_dir.join("labnas-backend"), version)?;

        // El release trae el binario como labnas-backend; respetar el nombre instalado
        let name = exe_name();
        if name != "labnas-backend" {
            std::fs::rename(new_dir.join("labnas-backend"), new_dir.join(&name))
                .map_err(|e| e.to_string())?;
        }

        swap_in(dir, &new_dir)?;
        write_pending(dir, &Pending {
            from: CURRENT_VERSION.to_string(),
            to: version.to_string(),
            attempts: 0,
        })
    })();

    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// El binario nuevo debe ejecutarse en esta maquina y reportar la version esperada.
fn verify_new_binary(bin: &Path, version: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    if !bin.is_file() {
        return Err("El tarball no contiene labnas-backend".to_string());
    }
    let _ = std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o755));
    let out = std::process::Command::new(bin)
        .arg("--version")
        .output()
        .map_err(|e| format!("El binario nuevo no se puede ejecutar en esta maquina: {}", e))?;
    let reported = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || reported != version {
        return Err(format!(
            "El binario nuevo no respondio correctamente (--version: '{}', esperado '{}')",
            reported, version
        ));
    }
    Ok(())
}

/// Mueve lo actual a `.rollback/` y lo de `new_dir` a la instalacion.
/// Si algo falla a mitad, deja la instalacion como estaba.
fn swap_in(dir: &Path, new_dir: &Path) -> Result<(), String> {
    let backup = dir.join(ROLLBACK_DIR);
    let _ = std::fs::remove_dir_all(&backup);
    std::fs::create_dir_all(&backup).map_err(|e| e.to_string())?;

    let entries: Vec<std::ffi::OsString> = std::fs::read_dir(new_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .collect();

    let mut moved_out = Vec::new();
    let mut moved_in = Vec::new();
    let res: Result<(), String> = (|| {
        for name in &entries {
            let current = dir.join(name);
            if current.exists() {
                std::fs::rename(&current, backup.join(name)).map_err(|e| e.to_string())?;
                moved_out.push(name.clone());
            }
        }
        for name in &entries {
            std::fs::rename(new_dir.join(name), dir.join(name)).map_err(|e| e.to_string())?;
            moved_in.push(name.clone());
        }
        Ok(())
    })();

    if let Err(e) = res {
        for name in &moved_in {
            let _ = std::fs::remove_dir_all(dir.join(name)).or_else(|_| std::fs::remove_file(dir.join(name)));
        }
        for name in &moved_out {
            let _ = std::fs::rename(backup.join(name), dir.join(name));
        }
        return Err(format!("Error reemplazando archivos (instalacion restaurada): {}", e));
    }
    Ok(())
}

// ─── Rollback ───

fn pending_path(dir: &Path) -> PathBuf {
    dir.join(ROLLBACK_DIR).join(PENDING_FILE)
}

fn read_pending(dir: &Path) -> Option<Pending> {
    serde_json::from_str(&std::fs::read_to_string(pending_path(dir)).ok()?).ok()
}

fn write_pending(dir: &Path, p: &Pending) -> Result<(), String> {
    let json = serde_json::to_string(p).map_err(|e| e.to_string())?;
    std::fs::write(pending_path(dir), json).map_err(|e| e.to_string())
}

/// Version guardada en `.rollback/` (la que se restauraria), si hay una.
pub fn rollback_version() -> Option<String> {
    let backup = install_dir().ok()?.join(ROLLBACK_DIR);
    backup.join(exe_name()).is_file().then(|| {
        std::fs::read_to_string(backup.join("VERSION"))
            .map(|v| v.trim().to_string())
            .unwrap_or_else(|_| "desconocida".to_string())
    })
}

/// Intercambia la instalacion con `.rollback/` (repetirlo vuelve a la version nueva).
pub fn swap_rollback() -> Result<String, String> {
    let dir = install_dir()?;
    let backup = dir.join(ROLLBACK_DIR);
    if !backup.join(exe_name()).is_file() {
        return Err("No hay una version anterior guardada".to_string());
    }
    let restored = rollback_version().unwrap_or_default();

    let entries: Vec<std::ffi::OsString> = std::fs::read_dir(&backup)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .filter(|n| !n.to_string_lossy().starts_with('.'))
        .collect();

    for name in &entries {
        let current = dir.join(name);
        let parked = dir.join(format!(".swap-{}", name.to_string_lossy()));
        let had_current = current.exists();
        if had_current {
            std::fs::rename(&current, &parked).map_err(|e| e.to_string())?;
        }
        std::fs::rename(backup.join(name), &current).map_err(|e| e.to_string())?;
        if had_current {
            std::fs::rename(&parked, backup.join(name)).map_err(|e| e.to_string())?;
        }
    }
    let _ = std::fs::remove_file(pending_path(&dir));
    Ok(restored)
}

/// Llamar al inicio de main(). Cuenta arranques de una version recien instalada y,
/// si fallo demasiadas veces, restaura la anterior. Devuelve true si hay que re-ejecutar.
pub fn startup_check() -> bool {
    let Ok(dir) = install_dir() else { return false };
    let Some(mut pending) = read_pending(&dir) else { return false };

    if pending.attempts >= MAX_START_ATTEMPTS {
        eprintln!(
            "[Updater] v{} fallo {} veces al arrancar; restaurando v{}",
            pending.to, pending.attempts, pending.from
        );
        match swap_rollback() {
            Ok(v) => {
                eprintln!("[Updater] Restaurada v{}", v);
                return true;
            }
            Err(e) => eprintln!("[Updater] No se pudo restaurar: {}", e),
        }
        let _ = std::fs::remove_file(pending_path(&dir));
        return false;
    }

    pending.attempts += 1;
    let _ = write_pending(&dir, &pending);
    false
}

/// La version nueva sobrevivio `CONFIRM_AFTER`: se da por buena.
pub fn confirm_update() {
    let Ok(dir) = install_dir() else { return };
    if let Some(p) = read_pending(&dir) {
        let _ = std::fs::remove_file(pending_path(&dir));
        println!("[Updater] Actualizacion a v{} confirmada", p.to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("labnas-upd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn checksum() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let f = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD  labnas.tar.gz\n";
        assert_eq!(parse_sha256_file(f).as_deref(), Some(sha256_hex(b"abc").as_str()));
        assert_eq!(parse_sha256_file("no-es-un-hash"), None);
    }

    #[test]
    fn nombre_de_asset_exacto() {
        let release = serde_json::json!({
            "tag_name": "v2.9.0",
            "assets": [
                { "name": format!("labnas-viewer-v2.9.0-linux-{}.tar.gz", release_arch()), "browser_download_url": "viewer" },
                { "name": format!("labnas-v2.9.0-linux-{}.tar.gz", release_arch()), "browser_download_url": "server" },
                { "name": format!("labnas-v2.9.0-linux-{}.tar.gz.sha256", release_arch()), "browser_download_url": "sha" },
            ]
        });
        let a = assets_for(&release).unwrap();
        assert_eq!(a.tarball_url, "server");
        assert_eq!(a.sha256_url.as_deref(), Some("sha"));
    }

    /// Tarball como el del release, con un "binario" que responde `--version`
    fn fake_release_tarball(root: &Path, version: &str) -> Vec<u8> {
        use std::os::unix::fs::PermissionsExt;
        let src = root.join("src/labnas");
        std::fs::create_dir_all(src.join("dist")).unwrap();
        let bin = src.join("labnas-backend");
        std::fs::write(&bin, format!("#!/bin/sh\necho {}\n", version)).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(src.join("VERSION"), version).unwrap();
        std::fs::write(src.join("dist/index.html"), version).unwrap();
        let tar = root.join("rel.tar.gz");
        let ok = std::process::Command::new("tar")
            .arg("czf").arg(&tar).arg("-C").arg(root.join("src")).arg("labnas")
            .status().unwrap().success();
        assert!(ok);
        std::fs::read(tar).unwrap()
    }

    #[test]
    fn instala_tarball_y_deja_pendiente() {
        let root = tmpdir();
        let inst = root.join("inst");
        std::fs::create_dir_all(inst.join("dist")).unwrap();
        std::fs::write(inst.join("VERSION"), "1.0.0").unwrap();
        std::fs::write(inst.join("dist/index.html"), "1.0.0").unwrap();

        let tarball = fake_release_tarball(&root, "9.9.9");
        install_tarball(&inst, &tarball, "9.9.9").unwrap();

        assert_eq!(std::fs::read_to_string(inst.join("VERSION")).unwrap(), "9.9.9");
        assert_eq!(std::fs::read_to_string(inst.join("dist/index.html")).unwrap(), "9.9.9");
        assert_eq!(std::fs::read_to_string(inst.join(".rollback/VERSION")).unwrap(), "1.0.0");
        let p = read_pending(&inst).unwrap();
        assert_eq!((p.to.as_str(), p.attempts), ("9.9.9", 0));
        assert!(!inst.join(TMP_DIR).exists());
    }

    #[test]
    fn rechaza_binario_con_version_distinta() {
        let root = tmpdir();
        let inst = root.join("inst");
        std::fs::create_dir_all(&inst).unwrap();
        std::fs::write(inst.join("VERSION"), "1.0.0").unwrap();

        let tarball = fake_release_tarball(&root, "9.9.9");
        let err = install_tarball(&inst, &tarball, "9.9.8").unwrap_err();
        assert!(err.contains("no respondio correctamente"), "{}", err);
        // instalacion intacta
        assert_eq!(std::fs::read_to_string(inst.join("VERSION")).unwrap(), "1.0.0");
        assert!(read_pending(&inst).is_none());
    }

    #[test]
    fn swap_in_y_restauracion() {
        let dir = tmpdir();
        std::fs::write(dir.join("labnas-backend"), "viejo").unwrap();
        std::fs::create_dir_all(dir.join("dist")).unwrap();
        std::fs::write(dir.join("dist/index.html"), "viejo").unwrap();

        let new_dir = dir.join("nuevo");
        std::fs::create_dir_all(new_dir.join("dist")).unwrap();
        std::fs::write(new_dir.join("labnas-backend"), "nuevo").unwrap();
        std::fs::write(new_dir.join("dist/index.html"), "nuevo").unwrap();

        swap_in(&dir, &new_dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("labnas-backend")).unwrap(), "nuevo");
        assert_eq!(std::fs::read_to_string(dir.join("dist/index.html")).unwrap(), "nuevo");
        assert_eq!(std::fs::read_to_string(dir.join(".rollback/labnas-backend")).unwrap(), "viejo");
        assert_eq!(std::fs::read_to_string(dir.join(".rollback/dist/index.html")).unwrap(), "viejo");
    }
}
