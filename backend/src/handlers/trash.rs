//! Papelera: borrar desde el explorador mueve a `.labnas-trash/` en vez de eliminar.
//!
//! La papelera de cada archivo vive en el ancestro mas alto que esta en la misma raiz
//! y en el mismo disco (st_dev), asi mover es un `rename` instantaneo incluso en discos
//! externos montados dentro de una raiz. Los metadatos van en la tabla `trash_items`.

use axum::{
    extract::{Path as AxPath, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::state::{AppState, SessionInfo};

type ApiError = (StatusCode, String);

pub const TRASH_DIR: &str = ".labnas-trash";
const DEFAULT_RETENTION_DAYS: u32 = 30;
/// Tope de entradas al calcular el tamaño de una carpeta
const SIZE_WALK_LIMIT: usize = 100_000;

fn internal(e: impl std::fmt::Display) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct TrashItem {
    pub id: String,
    pub name: String,
    pub original_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub deleted_by: String,
    pub deleted_at: String,
}

/// Directorio de papelera para `path` (ya resuelto dentro de `roots`).
fn trash_base(roots: &[PathBuf], path: &Path) -> Option<PathBuf> {
    let root = roots.iter().filter(|r| path.starts_with(r)).max_by_key(|r| r.as_os_str().len())?;
    let dev = std::fs::symlink_metadata(path).ok()?.dev();

    // Subir desde el padre mientras siga en el mismo disco y dentro de la raiz
    let mut base = path.parent()?.to_path_buf();
    while base != *root {
        let Some(parent) = base.parent() else { break };
        match std::fs::metadata(parent) {
            Ok(m) if m.dev() == dev && parent.starts_with(root) => base = parent.to_path_buf(),
            _ => break,
        }
    }
    Some(base.join(TRASH_DIR))
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    let mut seen = 0usize;
    while let Some(p) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&p) else { continue };
        for e in rd.filter_map(|e| e.ok()) {
            seen += 1;
            if seen > SIZE_WALK_LIMIT {
                return total;
            }
            match e.metadata() {
                Ok(m) if m.is_dir() => stack.push(e.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

/// Mueve `path` a la papelera y registra el elemento. Devuelve su id.
pub async fn move_to_trash(state: &AppState, roots: &[PathBuf], path: &Path, user: &str) -> Result<String, ApiError> {
    let base = trash_base(roots, path).ok_or((StatusCode::BAD_REQUEST, "Ruta invalida".to_string()))?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let target = base.join(&id);

    let (src, dst) = (path.to_path_buf(), target.clone());
    let (is_dir, size) = tokio::task::spawn_blocking(move || -> Result<(bool, u64), String> {
        let meta = std::fs::symlink_metadata(&src).map_err(|e| e.to_string())?;
        let is_dir = meta.is_dir();
        let size = if is_dir { dir_size(&src) } else { meta.len() };
        std::fs::create_dir_all(dst.parent().unwrap_or(Path::new("/"))).map_err(|e| e.to_string())?;
        std::fs::rename(&src, &dst).map_err(|e| e.to_string())?;
        Ok((is_dir, size))
    })
    .await
    .map_err(internal)?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo mover a la papelera: {}", e)))?;

    let item = TrashItem {
        id: id.clone(),
        name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        original_path: path.to_string_lossy().to_string(),
        is_dir,
        size,
        deleted_by: user.to_string(),
        deleted_at: chrono::Utc::now().to_rfc3339(),
    };
    let trash_path = target.to_string_lossy().to_string();
    crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO trash_items (id, name, original_path, trash_path, is_dir, size, deleted_by, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![item.id, item.name, item.original_path, trash_path, item.is_dir, item.size as i64, item.deleted_by, item.deleted_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;
    Ok(id)
}

fn load_item(conn: &rusqlite::Connection, id: &str) -> Result<Option<(TrashItem, String)>, String> {
    conn.query_row(
        "SELECT id, name, original_path, is_dir, size, deleted_by, deleted_at, trash_path FROM trash_items WHERE id = ?1",
        [id],
        |r| {
            Ok((
                TrashItem {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    original_path: r.get(2)?,
                    is_dir: r.get(3)?,
                    size: r.get::<_, i64>(4)? as u64,
                    deleted_by: r.get(5)?,
                    deleted_at: r.get(6)?,
                },
                r.get(7)?,
            ))
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn remove_path(p: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(p),
        Ok(_) => std::fs::remove_file(p),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// GET /api/trash — mas recientes primero
/// Puede gestionar el elemento: escritura en la carpeta de origen (el admin siempre)
fn can_manage(st: &crate::storage::Storage, s: &SessionInfo, item: &TrashItem) -> bool {
    s.role == crate::models::notifications::UserRole::Admin
        || st.can(s, Path::new(&item.original_path), crate::storage::Op::Write)
}

pub async fn list_trash(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<Vec<TrashItem>>, ApiError> {
    let st = crate::storage::Storage::load(&state.db).await?;
    let items = crate::db::db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare("SELECT id FROM trash_items ORDER BY deleted_at DESC")
            .map_err(|e| e.to_string())?;
        let ids: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        let mut out = Vec::new();
        for id in ids {
            if let Some((item, _)) = load_item(conn, &id)? {
                out.push(item);
            }
        }
        Ok(out)
    })
    .await?;
    // Solo lo que el usuario puede restaurar o borrar
    let items = items.into_iter().filter(|i| can_manage(&st, &session, i)).collect();
    Ok(Json(items))
}

/// POST /api/trash/{id}/restore — vuelve a su ruta original (con sufijo si ya existe)
pub async fn restore_item(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    AxPath(id): AxPath<String>,
) -> Result<Json<String>, ApiError> {
    let id2 = id.clone();
    let (item, trash_path) = crate::db::db_op(&state.db, move |conn| load_item(conn, &id2))
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No esta en la papelera".to_string()))?;

    // El destino debe seguir dentro de las raices actuales y el usuario poder escribir ahi
    let st = crate::storage::Storage::load(&state.db).await?;
    let original = st.resolve_for(&session, &item.original_path, crate::storage::Op::Write)?;

    let (src, orig) = (PathBuf::from(&trash_path), original.clone());
    let restored = tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
        if std::fs::symlink_metadata(&src).is_err() {
            return Err("El elemento ya no existe en la papelera".to_string());
        }
        if let Some(parent) = orig.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut dest = orig.clone();
        let mut n = 1;
        while std::fs::symlink_metadata(&dest).is_ok() {
            let stem = orig.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let ext = orig.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            dest = orig.with_file_name(format!("{} (restaurado {}){}", stem, n, ext));
            n += 1;
        }
        std::fs::rename(&src, &dest).map_err(|e| e.to_string())?;
        Ok(dest)
    })
    .await
    .map_err(internal)?
    .map_err(|e| (StatusCode::CONFLICT, e))?;

    crate::db::db_op(&state.db, move |conn| {
        conn.execute("DELETE FROM trash_items WHERE id = ?1", [&id]).map_err(|e| e.to_string())
    })
    .await?;

    let shown = restored.to_string_lossy().to_string();
    state.log_activity("Restaurado", &shown, &session.username).await;
    Ok(Json(shown))
}

/// DELETE /api/trash/{id} — borrar definitivamente
pub async fn delete_item(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    AxPath(id): AxPath<String>,
) -> Result<StatusCode, ApiError> {
    let id2 = id.clone();
    let (item, trash_path) = crate::db::db_op(&state.db, move |conn| load_item(conn, &id2))
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No esta en la papelera".to_string()))?;

    let st = crate::storage::Storage::load(&state.db).await?;
    if !can_manage(&st, &session, &item) {
        return Err((StatusCode::FORBIDDEN, "Sin permiso de escritura en la carpeta de origen".to_string()));
    }

    let p = PathBuf::from(trash_path);
    tokio::task::spawn_blocking(move || remove_path(&p)).await.map_err(internal)?.map_err(internal)?;
    crate::db::db_op(&state.db, move |conn| {
        conn.execute("DELETE FROM trash_items WHERE id = ?1", [&id]).map_err(|e| e.to_string())
    })
    .await?;

    state
        .log_activity("Eliminado definitivo", &item.original_path, &session.username)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Borra definitivamente los elementos que cumplen `where_sql`. Devuelve cuantos.
async fn purge(state: &AppState, where_sql: &'static str, arg: Option<String>) -> Result<usize, ApiError> {
    let rows: Vec<(String, String)> = crate::db::db_op(&state.db, move |conn| {
        let sql = format!("SELECT id, trash_path FROM trash_items {}", where_sql);
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let map = |r: &rusqlite::Row| Ok((r.get(0)?, r.get(1)?));
        let rows = match &arg {
            Some(a) => stmt.query_map([a], map),
            None => stmt.query_map([], map),
        }
        .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .await?;

    let mut removed = Vec::new();
    for (id, path) in rows {
        let p = PathBuf::from(path);
        let ok = tokio::task::spawn_blocking(move || remove_path(&p).is_ok()).await.unwrap_or(false);
        if ok {
            removed.push(id);
        }
    }
    let n = removed.len();
    crate::db::db_op(&state.db, move |conn| {
        for id in &removed {
            conn.execute("DELETE FROM trash_items WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await?;
    Ok(n)
}

/// DELETE /api/trash — vaciar (admin)
pub async fn empty_trash(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
) -> Result<Json<usize>, ApiError> {
    let n = purge(&state, "", None).await?;
    state
        .log_activity("Papelera vaciada", &format!("{} elementos", n), &session.username)
        .await;
    Ok(Json(n))
}

/// Borra lo que lleva mas de `trash_retention_days` en la papelera, una vez al dia.
pub async fn trash_cleanup_loop(state: AppState) {
    loop {
        let days = crate::db::db_op(&state.db, |conn| {
            Ok(crate::db::get_setting_u32(conn, "trash_retention_days", DEFAULT_RETENTION_DAYS))
        })
        .await
        .unwrap_or(DEFAULT_RETENTION_DAYS);
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        match purge(&state, "WHERE deleted_at < ?1", Some(cutoff)).await {
            Ok(n) if n > 0 => {
                println!("[Papelera] Limpieza: {} elementos eliminados (>{} dias)", n, days);
                state
                    .log_activity("Papelera limpieza", &format!("{} elementos (>{} dias)", n, days), "sistema")
                    .await;
            }
            Err((_, e)) => eprintln!("[Papelera] Error en limpieza: {}", e),
            _ => {}
        }
        tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn papelera_en_la_raiz_del_mismo_disco() {
        let root = std::env::temp_dir().join(format!("labnas-trash-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/f.txt"), "x").unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let roots = vec![root.clone()];
        assert_eq!(trash_base(&roots, &root.join("a/b/f.txt")), Some(root.join(TRASH_DIR)));
        assert_eq!(trash_base(&roots, &root.join("a")), Some(root.join(TRASH_DIR)));
        assert_eq!(trash_base(&[PathBuf::from("/otra")], &root.join("a")), None);
    }
}
