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
pub fn canonicalize_lenient(raw: &Path) -> Result<PathBuf, ApiError> {
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

// ─── Permisos por carpeta ───
//
// Cada raiz tiene lectores y escritores. Principales:
//   "*"             cualquier usuario aprobado
//   "perm:write"    usuarios con permiso de escritura de archivos (regla historica)
//   "role:<rol>"    admin | operador | observador
//   "user:<nombre>" un usuario puntual
// El admin siempre tiene acceso total. Sin configurar: lee cualquiera, escribe "perm:write".

pub const EVERYONE: &str = "*";
pub const WRITE_PERM: &str = "perm:write";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RootAccess {
    pub readers: Vec<String>,
    pub writers: Vec<String>,
}

impl Default for RootAccess {
    fn default() -> Self {
        Self { readers: vec![EVERYONE.to_string()], writers: vec![WRITE_PERM.to_string()] }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Read,
    Write,
}

/// Principal valido para una lista de acceso
pub fn valid_principal(p: &str) -> bool {
    match p {
        EVERYONE | WRITE_PERM => true,
        _ => {
            if let Some(role) = p.strip_prefix("role:") {
                matches!(role, "admin" | "operador" | "observador")
            } else if let Some(user) = p.strip_prefix("user:") {
                !user.is_empty() && user.len() <= 32 && user.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            } else {
                false
            }
        }
    }
}

fn principal_matches(p: &str, s: &crate::state::SessionInfo) -> bool {
    use crate::models::notifications::UserRole;
    match p {
        EVERYONE => true,
        WRITE_PERM => s.permissions.archivos_escritura,
        _ => {
            if let Some(role) = p.strip_prefix("role:") {
                let r = match s.role {
                    UserRole::Admin => "admin",
                    UserRole::Operador => "operador",
                    UserRole::Observador => "observador",
                    UserRole::Pendiente => "pendiente",
                };
                role == r
            } else {
                p.strip_prefix("user:") == Some(s.username.as_str())
            }
        }
    }
}

/// Raices configuradas con su acceso
#[derive(Debug, Clone)]
pub struct Storage {
    /// Canonicalizadas; ordenadas de la mas especifica a la mas general (para resolver)
    roots: Vec<(PathBuf, RootAccess)>,
    /// Orden en que el admin las configuro (para mostrarlas)
    ordered: Vec<PathBuf>,
}

impl Storage {
    pub fn from_conn(conn: &Connection) -> Self {
        let mut acl = std::collections::HashMap::new();
        if let Ok(mut stmt) = conn.prepare("SELECT root, readers, writers FROM root_access") {
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            });
            if let Ok(rows) = rows {
                for (root, readers, writers) in rows.flatten() {
                    acl.insert(
                        root,
                        RootAccess {
                            readers: serde_json::from_str(&readers).unwrap_or_default(),
                            writers: serde_json::from_str(&writers).unwrap_or_default(),
                        },
                    );
                }
            }
        }
        let ordered = canonical_roots(conn);
        let mut roots: Vec<(PathBuf, RootAccess)> = ordered
            .iter()
            .cloned()
            .map(|p| {
                let access = acl.get(p.to_string_lossy().as_ref()).cloned().unwrap_or_default();
                (p, access)
            })
            .collect();
        roots.sort_by_key(|(p, _)| std::cmp::Reverse(p.as_os_str().len()));
        Self { roots, ordered }
    }

    pub async fn load(pool: &DbPool) -> Result<Self, ApiError> {
        crate::db::db_op(pool, |conn| Ok(Self::from_conn(conn))).await
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.roots.iter().map(|(p, _)| p.clone()).collect()
    }

    pub fn is_root(&self, path: &Path) -> bool {
        self.roots.iter().any(|(p, _)| p == path)
    }

    /// Acceso de la raiz mas especifica que contiene `path`
    fn access_for(&self, path: &Path) -> Option<&RootAccess> {
        self.roots.iter().find(|(r, _)| path.starts_with(r)).map(|(_, a)| a)
    }

    pub fn can(&self, s: &crate::state::SessionInfo, path: &Path, op: Op) -> bool {
        if s.role == crate::models::notifications::UserRole::Admin {
            return self.access_for(path).is_some();
        }
        let Some(a) = self.access_for(path) else { return false };
        let writer = a.writers.iter().any(|p| principal_matches(p, s));
        match op {
            Op::Write => writer,
            // Quien puede escribir tambien puede leer
            Op::Read => writer || a.readers.iter().any(|p| principal_matches(p, s)),
        }
    }

    fn check(&self, s: &crate::state::SessionInfo, path: PathBuf, op: Op) -> Result<PathBuf, ApiError> {
        if self.can(s, &path, op) {
            Ok(path)
        } else {
            Err(forbidden(match op {
                Op::Read => "Sin permiso de lectura en esta carpeta",
                Op::Write => "Sin permiso de escritura en esta carpeta",
            }))
        }
    }

    /// Resuelve dentro de las raices y verifica el permiso del usuario
    pub fn resolve_for(&self, s: &crate::state::SessionInfo, raw: &str, op: Op) -> Result<PathBuf, ApiError> {
        let p = resolve(&self.paths(), raw)?;
        self.check(s, p, op)
    }

    pub fn resolve_existing_for(&self, s: &crate::state::SessionInfo, raw: &str, op: Op) -> Result<PathBuf, ApiError> {
        let p = resolve_existing(&self.paths(), raw)?;
        self.check(s, p, op)
    }

    /// Raices que el usuario puede leer (en el orden configurado)
    pub fn visible_roots(&self, s: &crate::state::SessionInfo) -> Vec<PathBuf> {
        self.ordered.iter().filter(|p| self.can(s, p, Op::Read)).cloned().collect()
    }

    pub fn access(&self) -> Vec<(PathBuf, RootAccess)> {
        self.roots.clone()
    }
}

/// Guarda el acceso de cada raiz (reemplaza lo anterior)
pub fn save_access(conn: &Connection, entries: &[(String, RootAccess)]) -> Result<(), String> {
    conn.execute("DELETE FROM root_access", []).map_err(|e| e.to_string())?;
    for (root, a) in entries {
        conn.execute(
            "INSERT INTO root_access (root, readers, writers) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                root,
                serde_json::to_string(&a.readers).map_err(|e| e.to_string())?,
                serde_json::to_string(&a.writers).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
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

    fn sess(user: &str, role: crate::models::notifications::UserRole, write: bool) -> crate::state::SessionInfo {
        crate::state::SessionInfo {
            username: user.into(),
            role,
            permissions: crate::models::notifications::UserPermissions { archivos_escritura: write, ..Default::default() },
            created_at: 0,
        }
    }

    #[test]
    fn permisos_por_carpeta() {
        use crate::models::notifications::UserRole::*;
        let a = tmp_root();
        let b = tmp_root();
        let st = Storage {
            roots: vec![
                (a.clone(), RootAccess::default()),
                (b.clone(), RootAccess { readers: vec!["role:operador".into()], writers: vec!["user:bob".into()] }),
            ],
            ordered: vec![a.clone(), b.clone()],
        };
        let obs = sess("ana", Observador, false);
        let obs_w = sess("eva", Observador, true);
        let op = sess("oscar", Operador, false);
        let bob = sess("bob", Observador, false);
        let admin = sess("root", Admin, false);

        // por defecto: todos leen, escribe quien tiene el permiso
        assert!(st.can(&obs, &a.join("x"), Op::Read));
        assert!(!st.can(&obs, &a.join("x"), Op::Write));
        assert!(st.can(&obs_w, &a.join("x"), Op::Write));
        // carpeta restringida
        assert!(!st.can(&obs, &b.join("x"), Op::Read));
        assert!(st.can(&op, &b.join("x"), Op::Read));
        assert!(!st.can(&op, &b.join("x"), Op::Write));
        assert!(st.can(&bob, &b.join("x"), Op::Write));
        assert!(st.can(&bob, &b.join("x"), Op::Read), "escribir implica leer");
        assert!(st.can(&admin, &b.join("x"), Op::Write));
        assert_eq!(st.visible_roots(&obs), vec![a.clone()]);
        // fuera de toda raiz: nadie, ni el admin
        assert!(!st.can(&admin, Path::new("/etc"), Op::Read));
    }

    #[test]
    fn raiz_mas_especifica_gana() {
        let a = tmp_root();
        let inner = a.join("sub");
        let st = Storage {
            roots: vec![
                (inner.clone(), RootAccess { readers: vec!["user:bob".into()], writers: vec![] }),
                (a.clone(), RootAccess::default()),
            ],
            ordered: vec![a.clone(), inner.clone()],
        };
        let ana = sess("ana", crate::models::notifications::UserRole::Observador, false);
        assert!(st.can(&ana, &a.join("otro"), Op::Read));
        assert!(!st.can(&ana, &inner.join("x"), Op::Read));
    }

    #[test]
    fn principales_validos() {
        for p in ["*", "perm:write", "role:operador", "user:bob.lab"] {
            assert!(valid_principal(p), "{}", p);
        }
        for p in ["", "role:jefe", "user:", "user:a b", "grupo:x"] {
            assert!(!valid_principal(p), "{}", p);
        }
    }

    #[test]
    fn sanitiza_nombres() {
        assert_eq!(sanitize_filename("../../etc/cron.d/x").as_deref(), Some("x"));
        assert_eq!(sanitize_filename("foto.png").as_deref(), Some("foto.png"));
        assert_eq!(sanitize_filename(".."), None);
        assert_eq!(sanitize_filename(""), None);
    }
}
