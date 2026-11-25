//! Descarga de `labnas-tray` desde el propio LabNAS (Configuracion > General >
//! Dispositivos vinculados): asi nadie tiene que buscar el archivo correcto en GitHub.
//!
//! La primera vez se baja del release de GitHub de esta misma version, se verifica con
//! su `.sha256` y queda guardado en `<datos>/downloads/`; despues se sirve desde ahi
//! (sirve aunque luego no haya internet). Se borran las copias de versiones anteriores.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use std::time::Duration;

use crate::error::ApiError;
use crate::state::AppState;

/// Plataformas publicadas por el release (ver `.github/workflows/release.yml`)
const PLATFORMS: &[(&str, &str)] = &[
    ("windows-x86_64", "zip"),
    ("linux-x86_64", "tar.gz"),
    ("linux-aarch64", "tar.gz"),
];

/// Una descarga a GitHub a la vez (dos personas pidiendo lo mismo no bajan dos veces)
static FETCH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn tag() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

/// Nombre exacto del asset para la plataforma (None si no existe)
pub fn asset_name(platform: &str, tag: &str) -> Option<String> {
    PLATFORMS
        .iter()
        .find(|(p, _)| *p == platform)
        .map(|(p, ext)| format!("labnas-tray-{tag}-{p}.{ext}"))
}

fn cache_dir() -> std::path::PathBuf {
    crate::db::data_dir().join("downloads")
}

/// Baja el asset de GitHub, lo verifica y lo deja en la cache
async fn fetch(state: &AppState, name: &str, dest: &std::path::Path) -> Result<(), ApiError> {
    let unavailable = |why: String| {
        (
            StatusCode::BAD_GATEWAY,
            format!(
                "No se pudo obtener {name} desde GitHub ({why}). Descárgalo de https://github.com/{}/releases/tag/{}",
                crate::updater::GITHUB_REPO,
                tag()
            ),
        )
    };
    let release = crate::updater::fetch_release(&state.http_client, Some(&tag()))
        .await
        .ok_or_else(|| unavailable("sin conexión o la versión no está publicada".into()))?;
    let url = crate::updater::asset_url(&release, name).ok_or_else(|| unavailable("el release no trae ese archivo".into()))?;
    let sha_url = crate::updater::asset_url(&release, &format!("{name}.sha256"))
        .ok_or_else(|| unavailable("falta el .sha256".into()))?;
    let sha = crate::updater::download(&state.http_client, &sha_url, Duration::from_secs(20)).await.map_err(unavailable)?;
    let expected = crate::updater::parse_sha256_file(&String::from_utf8_lossy(&sha))
        .ok_or_else(|| unavailable(".sha256 inválido".into()))?;
    let data = crate::updater::download(&state.http_client, &url, Duration::from_secs(180)).await.map_err(unavailable)?;
    if crate::updater::sha256_hex(&data) != expected {
        return Err(unavailable("el checksum no coincide".into()));
    }

    let dir = cache_dir();
    tokio::fs::create_dir_all(&dir).await.map_err(crate::error::internal)?;
    let tmp = dir.join(format!(".{name}.tmp"));
    tokio::fs::write(&tmp, &data).await.map_err(crate::error::internal)?;
    tokio::fs::rename(&tmp, dest).await.map_err(crate::error::internal)?;

    // Copias de versiones anteriores
    if let Ok(mut rd) = tokio::fs::read_dir(&dir).await {
        let current = tag();
        while let Ok(Some(e)) = rd.next_entry().await {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("labnas-tray-v") && !n.starts_with(&format!("labnas-tray-{current}-")) {
                let _ = tokio::fs::remove_file(e.path()).await;
            }
        }
    }
    Ok(())
}

/// GET /api/music/tray/{platform}
pub async fn download_tray(State(state): State<AppState>, Path(platform): Path<String>) -> Result<Response, ApiError> {
    let name = asset_name(&platform, &tag()).ok_or((StatusCode::NOT_FOUND, "Plataforma no disponible".to_string()))?;
    let path = cache_dir().join(&name);
    if !path.exists() {
        let _guard = FETCH_LOCK.lock().await;
        if !path.exists() {
            fetch(&state, &name, &path).await?;
        }
    }
    let data = tokio::fs::read(&path).await.map_err(crate::error::internal)?;
    let mime = if name.ends_with(".zip") { "application/zip" } else { "application/gzip" };
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
            (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
        ],
        data,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_de_asset() {
        assert_eq!(asset_name("windows-x86_64", "v3.5.0").as_deref(), Some("labnas-tray-v3.5.0-windows-x86_64.zip"));
        assert_eq!(asset_name("linux-aarch64", "v3.5.0").as_deref(), Some("labnas-tray-v3.5.0-linux-aarch64.tar.gz"));
        assert_eq!(asset_name("../etc", "v3.5.0"), None);
    }
}
