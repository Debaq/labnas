//! Respaldos programados con snapshots incrementales (rsync --link-dest).
//!
//! Cada ejecucion crea `<destino>/<AAAA-MM-DD_HHMMSS>/` con una copia completa del
//! origen; los archivos sin cambios son hardlinks al snapshot anterior (`latest`),
//! asi que solo ocupa espacio lo que cambio. Se conservan los ultimos `keep`.
//!
//! Aparte, la DB de LabNAS se respalda sola cada dia en `~/.labnas/backups/`.

use axum::{
    extract::{Path as AxPath, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Local, NaiveTime, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex as StdMutex};

use crate::db::DbPool;
use crate::state::{AppState, SessionInfo};

type ApiError = (StatusCode, String);

/// Copias diarias automaticas de labnas.db que se conservan
const DB_BACKUPS_KEEP: usize = 7;
const LATEST: &str = "latest";

/// Tareas ejecutandose ahora (evita dos ejecuciones simultaneas de la misma)
static RUNNING: LazyLock<StdMutex<HashSet<String>>> = LazyLock::new(|| StdMutex::new(HashSet::new()));

fn is_running(id: &str) -> bool {
    RUNNING.lock().map(|r| r.contains(id)).unwrap_or(false)
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupJob {
    pub id: String,
    pub name: String,
    pub source: String,
    pub destination: String,
    pub hour: u32,
    pub minute: u32,
    pub keep: u32,
    pub include_db: bool,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub last_status: Option<String>,
    pub last_message: Option<String>,
    pub created_at: String,
    pub running: bool,
}

#[derive(Debug, Deserialize)]
pub struct BackupJobRequest {
    pub name: String,
    pub source: String,
    pub destination: String,
    pub hour: u32,
    pub minute: u32,
    pub keep: u32,
    #[serde(default = "yes")]
    pub include_db: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Serialize)]
pub struct BackupsResponse {
    pub jobs: Vec<BackupJob>,
    pub rsync_available: bool,
    /// Copias automaticas de labnas.db (nombres, mas recientes primero)
    pub db_backups: Vec<String>,
    pub db_backups_dir: String,
}

const SELECT_JOB: &str = "SELECT id, name, source, destination, hour, minute, keep, include_db, enabled, \
     last_run, last_status, last_message, created_at FROM backup_jobs";

fn row_to_job(row: &rusqlite::Row) -> rusqlite::Result<BackupJob> {
    let id: String = row.get(0)?;
    Ok(BackupJob {
        running: is_running(&id),
        id,
        name: row.get(1)?,
        source: row.get(2)?,
        destination: row.get(3)?,
        hour: row.get(4)?,
        minute: row.get(5)?,
        keep: row.get(6)?,
        include_db: row.get(7)?,
        enabled: row.get(8)?,
        last_run: row.get(9)?,
        last_status: row.get(10)?,
        last_message: row.get(11)?,
        created_at: row.get(12)?,
    })
}

fn load_jobs(conn: &Connection) -> Result<Vec<BackupJob>, String> {
    let mut stmt = conn
        .prepare(&format!("{} ORDER BY name", SELECT_JOB))
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row_to_job).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn load_job(conn: &Connection, id: &str) -> Result<Option<BackupJob>, String> {
    conn.query_row(&format!("{} WHERE id = ?1", SELECT_JOB), [id], row_to_job)
        .optional()
        .map_err(|e| e.to_string())
}

fn rsync_available() -> bool {
    std::process::Command::new("rsync")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn db_backups_dir() -> PathBuf {
    crate::db::data_dir().join("backups")
}

// ─── Validacion ───

/// Origen dentro de las raices; destino absoluto, fuera del origen y de ~/.labnas.
fn validate(req: &BackupJobRequest, roots: &[PathBuf]) -> Result<(String, String, String), ApiError> {
    let bad = |m: &str| (StatusCode::BAD_REQUEST, m.to_string());
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 64 {
        return Err(bad("El nombre debe tener entre 1 y 64 caracteres"));
    }
    if req.hour > 23 || req.minute > 59 {
        return Err(bad("Hora invalida"));
    }
    if req.keep == 0 || req.keep > 365 {
        return Err(bad("Copias a conservar: entre 1 y 365"));
    }

    let source = crate::storage::resolve_existing(roots, &req.source)?;
    if !source.is_dir() {
        return Err(bad("El origen debe ser una carpeta"));
    }

    let raw_dest = Path::new(req.destination.trim());
    if !raw_dest.is_absolute() || raw_dest.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(bad("El destino debe ser una ruta absoluta"));
    }
    let dest = crate::storage::canonicalize_lenient(raw_dest)?;
    if dest.starts_with(&source) {
        return Err(bad("El destino no puede estar dentro del origen"));
    }
    if source.starts_with(&dest) {
        return Err(bad("El origen no puede estar dentro del destino"));
    }
    let data_dir = std::fs::canonicalize(crate::db::data_dir()).unwrap_or_else(|_| crate::db::data_dir());
    if dest.starts_with(&data_dir) {
        return Err(bad("El destino no puede estar en el directorio de datos de LabNAS"));
    }

    Ok((name, source.to_string_lossy().to_string(), dest.to_string_lossy().to_string()))
}

// ─── Endpoints (admin: rutas no declaradas en el middleware) ───

pub async fn list_backups(State(state): State<AppState>) -> Result<Json<BackupsResponse>, ApiError> {
    let jobs = crate::db::db_op(&state.db, load_jobs).await?;
    let dir = db_backups_dir();
    let mut db_backups: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.starts_with("labnas-") && n.ends_with(".db"))
                .collect()
        })
        .unwrap_or_default();
    db_backups.sort_by(|a, b| b.cmp(a));

    Ok(Json(BackupsResponse {
        jobs,
        rsync_available: rsync_available(),
        db_backups,
        db_backups_dir: dir.to_string_lossy().to_string(),
    }))
}

pub async fn create_backup(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<BackupJobRequest>,
) -> Result<Json<BackupJob>, ApiError> {
    let roots = crate::storage::load_roots(&state.db).await?;
    let (name, source, dest) = validate(&req, &roots)?;
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let now = Local::now().to_rfc3339();

    let (id2, name2, src2, dest2) = (id.clone(), name.clone(), source.clone(), dest.clone());
    let (hour, minute, keep, include_db, enabled) = (req.hour, req.minute, req.keep, req.include_db, req.enabled);
    let job = crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO backup_jobs (id, name, source, destination, hour, minute, keep, include_db, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id2, name2, src2, dest2, hour, minute, keep, include_db, enabled, now],
        )
        .map_err(|e| e.to_string())?;
        load_job(conn, &id2)?.ok_or_else(|| "No se pudo crear".to_string())
    })
    .await?;

    state
        .log_activity("Respaldo creado", &format!("{}: {} -> {}", name, source, dest), &session.username)
        .await;
    Ok(Json(job))
}

pub async fn update_backup(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    AxPath(id): AxPath<String>,
    Json(req): Json<BackupJobRequest>,
) -> Result<Json<BackupJob>, ApiError> {
    let roots = crate::storage::load_roots(&state.db).await?;
    let (name, source, dest) = validate(&req, &roots)?;

    let (id2, name2, src2, dest2) = (id.clone(), name.clone(), source, dest);
    let (hour, minute, keep, include_db, enabled) = (req.hour, req.minute, req.keep, req.include_db, req.enabled);
    let job = crate::db::db_op_status(&state.db, move |conn| {
        let n = conn
            .execute(
                "UPDATE backup_jobs SET name=?1, source=?2, destination=?3, hour=?4, minute=?5, keep=?6,
                 include_db=?7, enabled=?8 WHERE id=?9",
                params![name2, src2, dest2, hour, minute, keep, include_db, enabled, id2],
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if n == 0 {
            return Err((StatusCode::NOT_FOUND, "Respaldo no encontrado".to_string()));
        }
        load_job(conn, &id2)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
            .ok_or((StatusCode::NOT_FOUND, "Respaldo no encontrado".to_string()))
    })
    .await?;

    state.log_activity("Respaldo editado", &name, &session.username).await;
    Ok(Json(job))
}

/// Borra la tarea (no los snapshots ya creados)
pub async fn delete_backup(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    AxPath(id): AxPath<String>,
) -> Result<StatusCode, ApiError> {
    let id2 = id.clone();
    let name = crate::db::db_op_status(&state.db, move |conn| {
        let name: Option<String> = conn
            .query_row("SELECT name FROM backup_jobs WHERE id = ?1", [&id2], |r| r.get(0))
            .optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let name = name.ok_or((StatusCode::NOT_FOUND, "Respaldo no encontrado".to_string()))?;
        conn.execute("DELETE FROM backup_jobs WHERE id = ?1", [&id2])
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(name)
    })
    .await?;
    state.log_activity("Respaldo eliminado", &name, &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn run_backup_now(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    AxPath(id): AxPath<String>,
) -> Result<(StatusCode, String), ApiError> {
    let id2 = id.clone();
    let job = crate::db::db_op(&state.db, move |conn| load_job(conn, &id2))
        .await?
        .ok_or((StatusCode::NOT_FOUND, "Respaldo no encontrado".to_string()))?;
    if is_running(&job.id) {
        return Err((StatusCode::CONFLICT, "Ese respaldo ya se esta ejecutando".to_string()));
    }
    state
        .log_activity("Respaldo manual", &job.name, &session.username)
        .await;
    tokio::spawn(run_job(state.clone(), job));
    Ok((StatusCode::ACCEPTED, "Respaldo iniciado".to_string()))
}

/// Snapshots existentes de una tarea (mas recientes primero)
pub async fn list_snapshots(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let job = crate::db::db_op(&state.db, move |conn| load_job(conn, &id))
        .await?
        .ok_or((StatusCode::NOT_FOUND, "Respaldo no encontrado".to_string()))?;
    let mut snaps = list_snapshot_dirs(Path::new(&job.destination));
    snaps.reverse();
    Ok(Json(snaps))
}

// ─── Ejecucion ───

async fn run_job(state: AppState, job: BackupJob) {
    {
        let Ok(mut running) = RUNNING.lock() else { return };
        if !running.insert(job.id.clone()) {
            return;
        }
    }

    let id = job.id.clone();
    let _ = crate::db::db_op(&state.db, move |conn| {
        conn.execute("UPDATE backup_jobs SET last_status = 'running' WHERE id = ?1", [&id])
            .map_err(|e| e.to_string())
    })
    .await;

    let pool = state.db.clone();
    let (source, dest, keep, include_db) = (
        PathBuf::from(&job.source),
        PathBuf::from(&job.destination),
        job.keep as usize,
        job.include_db,
    );
    let result = tokio::task::spawn_blocking(move || snapshot(&source, &dest, keep, include_db.then_some(&pool)))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));

    let (status, message) = match &result {
        Ok(SnapshotOutcome { name, warning: None }) => ("ok", format!("Snapshot {}", name)),
        Ok(SnapshotOutcome { name, warning: Some(w) }) => ("warning", format!("Snapshot {} con advertencias: {}", name, w)),
        Err(e) => ("error", e.clone()),
    };

    let (id, st, msg) = (job.id.clone(), status.to_string(), message.clone());
    let _ = crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE backup_jobs SET last_run = ?1, last_status = ?2, last_message = ?3 WHERE id = ?4",
            params![Local::now().to_rfc3339(), st, msg, id],
        )
        .map_err(|e| e.to_string())
    })
    .await;

    state
        .log_activity(&format!("Respaldo {}", status), &format!("{}: {}", job.name, message), "sistema")
        .await;

    if status != "ok" {
        let icon = if status == "error" { "Fallo" } else { "Advertencia en" };
        crate::handlers::notifications::notify_admins(
            &state,
            &format!("*{} respaldo* `{}`\n\n{}", icon, job.name, message),
        )
        .await;
    }

    if let Ok(mut running) = RUNNING.lock() {
        running.remove(&job.id);
    }
}

#[derive(Debug)]
struct SnapshotOutcome {
    name: String,
    warning: Option<String>,
}

fn is_snapshot_name(name: &str) -> bool {
    // AAAA-MM-DD_HHMMSS
    let b = name.as_bytes();
    b.len() == 17
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'_',
            _ => c.is_ascii_digit(),
        })
}

/// Snapshots completos en `dest`, en orden cronologico
fn list_snapshot_dirs(dest: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dest)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| is_snapshot_name(n))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    all[all.len().saturating_sub(lines)..].join(" | ")
}

fn snapshot(source: &Path, dest: &Path, keep: usize, db: Option<&DbPool>) -> Result<SnapshotOutcome, String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("No se pudo crear el destino {}: {}", dest.display(), e))?;

    // Restos de una ejecucion interrumpida
    if let Ok(rd) = std::fs::read_dir(dest) {
        for e in rd.filter_map(|e| e.ok()) {
            if e.file_name().to_string_lossy().ends_with(".partial") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }

    let mut name = Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    if dest.join(&name).exists() {
        // Dos ejecuciones en el mismo segundo
        std::thread::sleep(std::time::Duration::from_secs(1));
        name = Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    }
    let partial = dest.join(format!("{}.partial", name));
    let latest = dest.join(LATEST);

    let mut cmd = std::process::Command::new("rsync");
    cmd.args(["-a", "--delete", "--numeric-ids"]);
    // La papelera no se respalda
    cmd.arg(format!("--exclude={}/", crate::handlers::trash::TRASH_DIR));
    if let Ok(prev) = std::fs::canonicalize(&latest) {
        if prev.is_dir() {
            cmd.arg(format!("--link-dest={}", prev.display()));
        }
    }
    cmd.arg(format!("{}/", source.display())).arg(&partial);

    let out = cmd.output().map_err(|e| format!("No se pudo ejecutar rsync (¿instalado?): {}", e))?;
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let warning = match out.status.code() {
        Some(0) => None,
        // 23: algunos archivos no se pudieron copiar (permisos); 24: desaparecieron durante la copia
        Some(23) | Some(24) => Some(tail(&stderr, 3)),
        code => {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(format!("rsync fallo (codigo {:?}): {}", code, tail(&stderr, 3)));
        }
    };

    if let Some(pool) = db {
        let db_dir = partial.join("_labnas");
        std::fs::create_dir_all(&db_dir).map_err(|e| e.to_string())?;
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute("VACUUM INTO ?1", [db_dir.join("labnas.db").to_string_lossy().to_string()])
            .map_err(|e| format!("Error respaldando la base de datos: {}", e))?;
    }

    let final_dir = dest.join(&name);
    std::fs::rename(&partial, &final_dir).map_err(|e| e.to_string())?;

    // latest -> snapshot nuevo (symlink relativo, reemplazo atomico)
    let tmp_link = dest.join(".latest.tmp");
    let _ = std::fs::remove_file(&tmp_link);
    std::os::unix::fs::symlink(&name, &tmp_link).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_link, &latest).map_err(|e| e.to_string())?;

    // Retencion
    let snaps = list_snapshot_dirs(dest);
    if snaps.len() > keep {
        for old in &snaps[..snaps.len() - keep] {
            let _ = std::fs::remove_dir_all(dest.join(old));
        }
    }

    Ok(SnapshotOutcome { name, warning })
}

// ─── Programacion ───

fn parse_time(s: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(s).ok().map(|t| t.with_timezone(&Local))
}

/// ¿Toca ejecutar? Si la hora de hoy ya paso y no hubo ejecucion desde entonces.
/// Si el equipo estuvo apagado a esa hora, se ejecuta al volver.
fn is_due(job: &BackupJob, now: DateTime<Local>) -> bool {
    if !job.enabled {
        return false;
    }
    let Some(time) = NaiveTime::from_hms_opt(job.hour, job.minute, 0) else { return false };
    let Some(scheduled) = Local.from_local_datetime(&now.date_naive().and_time(time)).earliest() else {
        return false;
    };
    if now < scheduled {
        return false;
    }
    // Una tarea recien creada no corre hasta su primera hora programada
    let baseline = job
        .last_run
        .as_deref()
        .and_then(parse_time)
        .into_iter()
        .chain(parse_time(&job.created_at))
        .max();
    baseline.is_none_or(|b| b < scheduled)
}

/// Copia diaria de labnas.db en ~/.labnas/backups (conserva DB_BACKUPS_KEEP)
fn db_auto_backup(pool: &DbPool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = db_backups_dir();
    let file = dir.join(format!("labnas-{}.db", Local::now().format("%Y-%m-%d")));
    if file.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute("VACUUM INTO ?1", [file.to_string_lossy().to_string()])
        .map_err(|e| e.to_string())?;
    let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));

    let mut all: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("labnas-") && n.to_string_lossy().ends_with(".db"))
                .unwrap_or(false)
        })
        .collect();
    all.sort();
    if all.len() > DB_BACKUPS_KEEP {
        for old in &all[..all.len() - DB_BACKUPS_KEEP] {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(())
}

pub async fn backup_scheduler_loop(state: AppState) {
    loop {
        let pool = state.db.clone();
        match tokio::task::spawn_blocking(move || db_auto_backup(&pool)).await {
            Ok(Err(e)) => eprintln!("[Respaldos] Error en copia automatica de la DB: {}", e),
            Err(e) => eprintln!("[Respaldos] Error en copia automatica de la DB: {}", e),
            _ => {}
        }

        if let Ok(jobs) = crate::db::db_op(&state.db, load_jobs).await {
            let now = Local::now();
            for job in jobs {
                if !job.running && is_due(&job, now) {
                    println!("[Respaldos] Ejecutando '{}'", job.name);
                    tokio::spawn(run_job(state.clone(), job));
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(hour: u32, minute: u32, created: &str, last: Option<&str>) -> BackupJob {
        BackupJob {
            id: "x".into(),
            name: "x".into(),
            source: "/a".into(),
            destination: "/b".into(),
            hour,
            minute,
            keep: 3,
            include_db: false,
            enabled: true,
            last_run: last.map(|s| s.to_string()),
            last_status: None,
            last_message: None,
            created_at: created.to_string(),
            running: false,
        }
    }

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 25, h, m, 0).unwrap()
    }

    #[test]
    fn programacion() {
        let ayer = Local.with_ymd_and_hms(2026, 9, 24, 12, 0, 0).unwrap().to_rfc3339();
        // antes de la hora: no
        assert!(!is_due(&job(3, 0, &ayer, None), at(2, 59)));
        // pasada la hora y nunca ejecutada: si
        assert!(is_due(&job(3, 0, &ayer, None), at(3, 0)));
        // ya ejecutada hoy despues de la hora: no
        let hoy = at(3, 1).to_rfc3339();
        assert!(!is_due(&job(3, 0, &ayer, Some(&hoy)), at(10, 0)));
        // equipo apagado a las 3:00, ultima ejecucion ayer: al volver, si
        let ayer_3 = Local.with_ymd_and_hms(2026, 9, 24, 3, 0, 30).unwrap().to_rfc3339();
        assert!(is_due(&job(3, 0, &ayer, Some(&ayer_3)), at(9, 0)));
        // creada hoy despues de la hora: espera a mañana
        assert!(!is_due(&job(3, 0, &at(8, 0).to_rfc3339(), None), at(9, 0)));
    }

    #[test]
    fn nombres_de_snapshot() {
        assert!(is_snapshot_name("2026-09-25_031500"));
        assert!(!is_snapshot_name("latest"));
        assert!(!is_snapshot_name("2026-09-25_031500.partial"));
    }

    #[test]
    fn snapshots_incrementales_y_retencion() {
        use std::os::unix::fs::MetadataExt;
        if !rsync_available() {
            eprintln!("rsync no disponible; test omitido");
            return;
        }
        let root = std::env::temp_dir().join(format!("labnas-bk-{}", uuid::Uuid::new_v4()));
        let src = root.join("src");
        let dest = root.join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), "uno").unwrap();

        let s1 = snapshot(&src, &dest, 2, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(src.join("b.txt"), "dos").unwrap();
        let s2 = snapshot(&src, &dest, 2, None).unwrap();

        // archivo sin cambios = mismo inode (hardlink)
        let i1 = std::fs::metadata(dest.join(&s1.name).join("a.txt")).unwrap().ino();
        let i2 = std::fs::metadata(dest.join(&s2.name).join("a.txt")).unwrap().ino();
        assert_eq!(i1, i2);
        assert!(dest.join(&s2.name).join("b.txt").exists());
        assert_eq!(std::fs::read_link(dest.join(LATEST)).unwrap(), PathBuf::from(&s2.name));

        std::thread::sleep(std::time::Duration::from_millis(1100));
        let s3 = snapshot(&src, &dest, 2, None).unwrap();
        let snaps = list_snapshot_dirs(&dest);
        assert_eq!(snaps, vec![s2.name, s3.name]);
        let _ = std::fs::remove_dir_all(root);
    }
}
