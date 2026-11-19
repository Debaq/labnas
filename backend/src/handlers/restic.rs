//! Respaldos con restic: cifrados, deduplicados y en cualquier destino que
//! restic soporte (carpeta local o disco externo, SFTP a otro equipo, S3,
//! Backblaze B2, servidor REST...). Se usa el binario `restic` del sistema.
//!
//! La contraseña del repositorio y las credenciales del destino se guardan
//! cifradas en la base y se pasan a restic por variables de entorno (nunca
//! como argumentos visibles en `ps`).

use std::path::Path;
use std::process::{Command, Output};

/// Variables de entorno de credenciales que se aceptan (lista cerrada: nada de LD_PRELOAD y similares)
pub const ALLOWED_ENV: &[&str] = &[
    "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_DEFAULT_REGION", "AWS_SESSION_TOKEN",
    "B2_ACCOUNT_ID", "B2_ACCOUNT_KEY",
    "AZURE_ACCOUNT_NAME", "AZURE_ACCOUNT_KEY", "AZURE_ACCOUNT_SAS",
    "GOOGLE_PROJECT_ID", "GOOGLE_APPLICATION_CREDENTIALS",
    "RESTIC_REST_USERNAME", "RESTIC_REST_PASSWORD",
    "ST_AUTH", "ST_USER", "ST_KEY", "OS_AUTH_URL", "OS_USERNAME", "OS_PASSWORD", "OS_PROJECT_NAME",
];

const REMOTE_PREFIXES: &[&str] = &["sftp:", "s3:", "b2:", "rest:", "azure:", "gs:", "swift:", "rclone:"];

/// Carpeta (dentro del origen) donde se restauran snapshots; no se respalda
pub const RESTORE_DIR: &str = ".labnas-restore";

pub fn available() -> bool {
    Command::new("restic").arg("version").output().is_ok_and(|o| o.status.success())
}

/// ¿Repositorio remoto? (si no, es una ruta local que se valida como el destino de rsync)
pub fn is_remote(repo: &str) -> bool {
    REMOTE_PREFIXES.iter().any(|p| repo.starts_with(p))
}

pub fn validate_repo(repo: &str) -> Result<(), String> {
    if repo.is_empty() || repo.len() > 500 || repo.chars().any(|c| c.is_control()) {
        return Err("Repositorio invalido".into());
    }
    if repo.starts_with('-') {
        return Err("Repositorio invalido".into());
    }
    if !is_remote(repo) && !repo.starts_with('/') {
        return Err("El repositorio debe ser una ruta absoluta o un destino de restic (sftp:, s3:, b2:, rest:...)".into());
    }
    Ok(())
}

/// "CLAVE=valor" por linea, solo claves permitidas
pub fn parse_env(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let (k, v) = line.split_once('=').ok_or_else(|| format!("Linea sin '=': {}", k_only(line)))?;
        let k = k.trim();
        if !ALLOWED_ENV.contains(&k) {
            return Err(format!("Variable no permitida: {} (permitidas: {})", k, ALLOWED_ENV.join(", ")));
        }
        out.push((k.to_string(), v.trim().to_string()));
    }
    Ok(out)
}

fn k_only(line: &str) -> String {
    crate::textutil::ellipsize(line.split('=').next().unwrap_or(line), 30)
}

pub struct Repo<'a> {
    pub repo: &'a str,
    pub password: &'a str,
    pub env: &'a [(String, String)],
}

impl Repo<'_> {
    fn run(&self, args: &[&str]) -> Result<Output, String> {
        let mut c = Command::new("restic");
        c.arg("--repo").arg(self.repo).env("RESTIC_PASSWORD", self.password);
        // Sin barra de progreso: la salida va a los logs
        c.env("RESTIC_PROGRESS_FPS", "0");
        for (k, v) in self.env {
            c.env(k, v);
        }
        c.args(args).output().map_err(|e| format!("No se pudo ejecutar restic (¿instalado?): {}", e))
    }

    /// Crea el repositorio si no existe
    pub fn ensure_init(&self) -> Result<(), String> {
        let probe = self.run(&["cat", "config"])?;
        if probe.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&probe.stderr).to_lowercase();
        // restic >= 0.17 sale con 10 si el repositorio no existe
        let missing = probe.status.code() == Some(10)
            || err.contains("does not exist")
            || err.contains("is there a repository")
            || err.contains("unable to open config file");
        if !missing {
            if err.contains("wrong password") || probe.status.code() == Some(12) {
                return Err("Contraseña del repositorio incorrecta".into());
            }
            return Err(format!("No se pudo abrir el repositorio: {}", tail(&err, 2)));
        }
        let init = self.run(&["init"])?;
        if !init.status.success() {
            return Err(format!("No se pudo crear el repositorio: {}", tail(&String::from_utf8_lossy(&init.stderr), 2)));
        }
        Ok(())
    }

    /// Respalda `paths`; devuelve (id corto del snapshot, advertencia)
    pub fn backup(&self, paths: &[&Path], excludes: &[&str]) -> Result<(String, Option<String>), String> {
        let mut args: Vec<String> = vec!["backup".into(), "--json".into(), "--tag".into(), "labnas".into()];
        for e in excludes {
            args.push(format!("--exclude={}", e));
        }
        args.push("--".into());
        for p in paths {
            args.push(p.to_string_lossy().to_string());
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run(&refs)?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let summary = stdout
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .find(|v| v["message_type"] == "summary");
        let warning = match out.status.code() {
            Some(0) => None,
            // 3: no se pudieron leer algunos archivos (permisos), el snapshot igual se creo
            Some(3) => Some(tail(&String::from_utf8_lossy(&out.stderr), 3)),
            code => {
                return Err(format!("restic fallo (codigo {:?}): {}", code, tail(&String::from_utf8_lossy(&out.stderr), 3)));
            }
        };
        let summary = summary.ok_or("restic no informo el snapshot creado")?;
        let id = summary["snapshot_id"].as_str().unwrap_or("?").chars().take(8).collect::<String>();
        let added = summary["data_added"].as_u64().unwrap_or(0);
        let mb = added as f64 / 1_048_576.0;
        Ok((format!("{} (+{:.1} MB)", id, mb), warning))
    }

    /// Conserva un snapshot por dia de los ultimos `keep` dias y libera el resto
    pub fn forget(&self, keep: u32) -> Result<(), String> {
        let k = keep.to_string();
        let out = self.run(&["forget", "--tag", "labnas", "--keep-daily", &k, "--keep-last", "1", "--prune"])?;
        if !out.status.success() {
            return Err(format!("No se pudieron limpiar snapshots viejos: {}", tail(&String::from_utf8_lossy(&out.stderr), 2)));
        }
        Ok(())
    }

    /// "AAAA-MM-DD HH:MM · id" de los snapshots, mas recientes primero
    pub fn snapshots(&self) -> Result<Vec<String>, String> {
        let out = self.run(&["snapshots", "--json", "--tag", "labnas"])?;
        if !out.status.success() {
            return Err(tail(&String::from_utf8_lossy(&out.stderr), 2));
        }
        let list: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
        let mut snaps: Vec<(String, String)> = list
            .iter()
            .filter_map(|s| {
                let time = chrono::DateTime::parse_from_rfc3339(s["time"].as_str()?).ok()?;
                let local = time.with_timezone(&chrono::Local);
                Some((local.to_rfc3339(), format!("{} · {}", local.format("%Y-%m-%d %H:%M"), s["short_id"].as_str()?)))
            })
            .collect();
        snaps.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(snaps.into_iter().map(|(_, s)| s).collect())
    }

    /// Restaura un snapshot completo en `target` (carpeta nueva)
    pub fn restore(&self, snapshot: &str, target: &Path) -> Result<(), String> {
        let t = target.to_string_lossy().to_string();
        let out = self.run(&["restore", snapshot, "--target", &t])?;
        if !out.status.success() {
            return Err(format!("No se pudo restaurar: {}", tail(&String::from_utf8_lossy(&out.stderr), 3)));
        }
        Ok(())
    }

    /// Verifica la estructura y una muestra de los datos
    pub fn check(&self) -> Result<(), String> {
        let out = self.run(&["check", "--read-data-subset=5%"])?;
        if !out.status.success() {
            return Err(format!("El repositorio tiene errores: {}", tail(&String::from_utf8_lossy(&out.stdout), 3)));
        }
        Ok(())
    }
}

/// Id corto de un snapshot (8 hex) a partir de "fecha · id"
pub fn snapshot_id(label: &str) -> Option<String> {
    let id = label.rsplit(' ').next()?.trim();
    (id.len() >= 8 && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit())).then(|| id.to_string())
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    crate::textutil::ellipsize(&all[all.len().saturating_sub(lines)..].join(" | "), 400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_y_env() {
        assert!(validate_repo("/mnt/externo/labnas").is_ok());
        assert!(validate_repo("sftp:respaldo@nas2:/srv/labnas").is_ok());
        assert!(validate_repo("b2:mi-bucket:labnas").is_ok());
        assert!(validate_repo("relativa/x").is_err());
        assert!(validate_repo("--password-command=evil").is_err());
        assert!(validate_repo("s3:x\nrm").is_err());

        let env = parse_env("# credenciales\nB2_ACCOUNT_ID=abc\n B2_ACCOUNT_KEY = k=1 \n").unwrap();
        assert_eq!(env, vec![("B2_ACCOUNT_ID".into(), "abc".into()), ("B2_ACCOUNT_KEY".into(), "k=1".into())]);
        assert!(parse_env("LD_PRELOAD=/tmp/x.so").is_err());
        assert!(parse_env("sin igual").is_err());

        assert_eq!(snapshot_id("2026-11-18 03:00 · a1b2c3d4").as_deref(), Some("a1b2c3d4"));
        assert_eq!(snapshot_id("a1b2c3d4").as_deref(), Some("a1b2c3d4"));
        assert_eq!(snapshot_id("--target=/").as_deref(), None);
    }
}
