//! Raices de almacenamiento: las unicas rutas del sistema accesibles desde la API
//! (explorador, compartir, descargar URL, imprimir archivo...).
//!
//! Toda ruta recibida del cliente se canonicaliza (resuelve `..` y symlinks) y debe
//! quedar dentro de alguna raiz. El directorio de datos de LabNAS (~/.labnas, con la
//! DB y sus secretos) queda bloqueado aunque este dentro de una raiz.

use axum::http::StatusCode;
use rusqlite::Connection;
use std::path::{Component, Path, PathBuf};

use crate::db::DbPool;

const SETTING_KEY: &str = "storage_roots";

type ApiError = (StatusCode, String);

fn forbidden(msg: &str) -> ApiError {
    (StatusCode::FORBIDDEN, msg.to_string())
}

/// Raices por defecto: home del usuario del NAS + puntos de montaje de medios externos.
pub fn default_roots() -> Vec<String> {
    let mut roots = vec![crate::config::resolve_home()];
    for p in ["/media", "/mnt", "/run/media"] {
        if Path::new(p).is_dir() {
            roots.push(p.to_string());
        }
    }
    roots
}

/// Raices configuradas (tal como se guardaron), o las por defecto.
pub fn configured_roots(conn: &Connection) -> Vec<String> {
    crate::db::get_setting(conn, SETTING_KEY)
        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .filter(|r| !r.is_empty())
        .unwrap_or_else(default_roots)
}

pub fn save_roots(conn: &Connection, roots: &[String]) -> Result<(), String> {
    let json = serde_json::to_string(roots).map_err(|e| e.to_string())?;
    crate::db::set_setting(conn, SETTING_KEY, &json)
}

/// Raices canonicalizadas que existen en disco.
pub fn canonical_roots(conn: &Connection) -> Vec<PathBuf> {
    configured_roots(conn)
        .iter()
        .filter_map(|r| std::fs::canonicalize(r).ok())
        .filter(|p| p.is_dir())
        .collect()
}

pub async fn load_roots(pool: &DbPool) -> Result<Vec<PathBuf>, ApiError> {
    crate::db::db_op(pool, |conn| Ok(canonical_roots(conn))).await
}

/// Valida una lista de raices enviada por un admin.
pub fn validate_roots(roots: &[String]) -> Result<Vec<String>, ApiError> {
    let mut out = Vec::new();
    for r in roots {
        let r = r.trim();
        if r.is_empty() {
            continue;
        }
        let p = Path::new(r);
        if !p.is_absolute() {
            return Err((StatusCode::BAD_REQUEST, format!("'{}' no es una ruta absoluta", r)));
        }
        let canon = std::fs::canonicalize(p)
            .map_err(|_| (StatusCode::BAD_REQUEST, format!("'{}' no existe", r)))?;
        if !canon.is_dir() {
            return Err((StatusCode::BAD_REQUEST, format!("'{}' no es un directorio", r)));
        }
        let canon = canon.to_string_lossy().to_string();
        if !out.contains(&canon) {
            out.push(canon);
        }
    }
    if out.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Debe haber al menos una raiz".to_string()));
    }
    Ok(out)
}

/// Canonicaliza `raw`. Si no existe, canonicaliza el ancestro existente mas cercano
/// y agrega el resto (sin permitir `..`).
fn canonicalize_lenient(raw: &Path) -> Result<PathBuf, ApiError> {
    if let Ok(p) = std::fs::canonicalize(raw) {
        return Ok(p);
    }
    let mut existing = raw.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        match existing.file_name() {
            Some(name) => {
                rest.push(name.to_os_string());
                existing.pop();
            }
            None => return Err(forbidden("Ruta invalida")),
        }
        if let Ok(base) = std::fs::canonicalize(&existing) {
            let mut out = base;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return Ok(out);
        }
    }
}

/// Resuelve una ruta del cliente dentro de las raices permitidas.
pub fn resolve(roots: &[PathBuf], raw: &str) -> Result<PathBuf, ApiError> {
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err((StatusCode::BAD_REQUEST, "La ruta debe ser absoluta".to_string()));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        // canonicalize lo resolveria, pero en rutas inexistentes no; se rechaza directo
        return Err(forbidden("Ruta invalida"));
    }

    let canon = canonicalize_lenient(path)?;

    let data_dir = std::fs::canonicalize(crate::db::data_dir()).unwrap_or_else(|_| crate::db::data_dir());
    if canon.starts_with(&data_dir) {
        return Err(forbidden("Ruta protegida"));
    }

    if roots.iter().any(|r| canon.starts_with(r)) {
        Ok(canon)
    } else {
        Err(forbidden("Ruta fuera de las carpetas permitidas"))
    }
}

/// Resuelve y exige que exista.
pub fn resolve_existing(roots: &[PathBuf], raw: &str) -> Result<PathBuf, ApiError> {
    let p = resolve(roots, raw)?;
    if !p.exists() {
        return Err((StatusCode::NOT_FOUND, "No encontrado".to_string()));
    }
    Ok(p)
}

pub fn is_root(roots: &[PathBuf], path: &Path) -> bool {
    roots.iter().any(|r| r == path)
}

/// Nombre de archivo seguro (sin directorios, `..` ni NUL).
pub fn sanitize_filename(name: &str) -> Option<String> {
    let base = Path::new(name).file_name()?.to_string_lossy().to_string();
    let base = base.replace('\0', "");
    if base.is_empty() || base == "." || base == ".." {
        return None;
    }
    Some(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("labnas-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::canonicalize(dir).unwrap()
    }

    #[test]
    fn permite_dentro_de_raiz() {
        let root = tmp_root();
        let roots = vec![root.clone()];
        let p = resolve(&roots, root.join("sub").to_str().unwrap()).unwrap();
        assert_eq!(p, root.join("sub"));
        // inexistente pero dentro
        let p = resolve(&roots, root.join("sub/nuevo/x.txt").to_str().unwrap()).unwrap();
        assert_eq!(p, root.join("sub/nuevo/x.txt"));
    }

    #[test]
    fn rechaza_fuera_de_raiz() {
        let root = tmp_root();
        let roots = vec![root.clone()];
        assert!(resolve(&roots, "/etc/passwd").is_err());
        assert!(resolve(&roots, "relativa").is_err());
        let escape = format!("{}/../../etc/passwd", root.display());
        assert!(resolve(&roots, &escape).is_err());
    }

    #[test]
    fn rechaza_symlink_que_escapa() {
        let root = tmp_root();
        let link = root.join("escape");
        std::os::unix::fs::symlink("/etc", &link).unwrap();
        let roots = vec![root.clone()];
        assert!(resolve(&roots, link.join("passwd").to_str().unwrap()).is_err());
    }

    #[test]
    fn sanitiza_nombres() {
        assert_eq!(sanitize_filename("../../etc/cron.d/x").as_deref(), Some("x"));
        assert_eq!(sanitize_filename("foto.png").as_deref(), Some("foto.png"));
        assert_eq!(sanitize_filename(".."), None);
        assert_eq!(sanitize_filename(""), None);
    }
}
