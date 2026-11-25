//! Descarga de `labnas-tray` desde el propio LabNAS (Configuracion > General >
//! Dispositivos vinculados): asi nadie tiene que buscar el archivo correcto en GitHub.
//!
//! La primera vez se baja del release de GitHub de esta misma version, se verifica con
//! su `.sha256` y queda guardado en `<datos>/downloads/`; despues se sirve desde ahi
//! (sirve aunque luego no haya internet). Se borran las copias de versiones anteriores.
//!
//! Cada descarga lleva la direccion desde la que se bajo (`?server=`, que debe coincidir
//! con el `Host` del pedido), asi la bandeja encuentra el LabNAS aunque mDNS este
//! apagado o la red lo bloquee:
//! - Windows (.exe): al final del ejecutable, `url=...\n` + `LNTCONF1` + largo (u32 LE).
//!   Windows ignora lo que va despues de la imagen PE; sirve aunque se ejecute sin copiarlo.
//! - Linux (.tar.gz): `labnas-tray/labnas-tray.conf` (install.sh lo copia a la config).

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::io::{Read, Write};
use std::time::Duration;

use crate::error::ApiError;
use crate::state::AppState;

/// Plataformas publicadas por el release (ver `.github/workflows/release.yml`)
const PLATFORMS: &[(&str, &str)] = &[
    ("windows-x86_64", "exe"),
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

/// Nombre del archivo de configuracion que lee la bandeja junto a su ejecutable
const CONF_NAME: &str = "labnas-tray.conf";

/// `http(s)://host[:puerto]` si coincide con el `Host` del pedido (asi solo se
/// incrustan direcciones reales de este LabNAS)
fn server_url(raw: &str, host_header: &str) -> Option<String> {
    let raw = raw.trim().trim_end_matches('/');
    let rest = raw.strip_prefix("https://").or_else(|| raw.strip_prefix("http://"))?;
    let ok = !rest.is_empty()
        && rest.eq_ignore_ascii_case(host_header)
        && rest.chars().all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c));
    ok.then(|| raw.to_string())
}

fn conf_text(url: &str) -> String {
    format!("# Direccion del LabNAS desde el que se descargo esta copia\nurl={url}\n")
}

/// Marca del bloque de configuracion al final del .exe (debe coincidir con labnas-tray)
const TRAILER_MAGIC: &[u8; 8] = b"LNTCONF1";

/// Agrega la direccion al .exe (Windows) o `labnas-tray.conf` al .tar.gz (Linux)
fn with_conf(name: &str, data: &[u8], url: &str) -> Result<Vec<u8>, String> {
    let conf = conf_text(url);
    if name.ends_with(".exe") {
        let mut out = Vec::with_capacity(data.len() + conf.len() + 12);
        out.extend_from_slice(data);
        out.extend_from_slice(conf.as_bytes());
        out.extend_from_slice(TRAILER_MAGIC);
        out.extend_from_slice(&(conf.len() as u32).to_le_bytes());
        return Ok(out);
    }
    // tar.gz: se copian las entradas y se agrega el archivo dentro de la carpeta labnas-tray/
    let mut tar_bytes = Vec::new();
    flate2::read::GzDecoder::new(data).read_to_end(&mut tar_bytes).map_err(|e| e.to_string())?;
    let mut builder = tar::Builder::new(Vec::new());
    let mut archive = tar::Archive::new(tar_bytes.as_slice());
    let mut dir = String::from("labnas-tray");
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        if let Some(first) = entry.path().ok().and_then(|p| p.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned())) {
            dir = first;
        }
        let header = entry.header().clone();
        builder.append(&header, &mut entry).map_err(|e| e.to_string())?;
    }
    let mut header = tar::Header::new_gnu();
    header.set_size(conf.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(crate::state::now_unix().max(0) as u64);
    builder.append_data(&mut header, format!("{dir}/{CONF_NAME}"), conf.as_bytes()).map_err(|e| e.to_string())?;
    let tar_out = builder.into_inner().map_err(|e| e.to_string())?;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar_out).map_err(|e| e.to_string())?;
    gz.finish().map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
pub struct DownloadQuery {
    server: Option<String>,
}

/// GET /api/music/tray/{platform}?server=<origen>
pub async fn download_tray(
    State(state): State<AppState>,
    Path(platform): Path<String>,
    Query(q): Query<DownloadQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let name = asset_name(&platform, &tag()).ok_or((StatusCode::NOT_FOUND, "Plataforma no disponible".to_string()))?;
    let path = cache_dir().join(&name);
    if !path.exists() {
        let _guard = FETCH_LOCK.lock().await;
        if !path.exists() {
            fetch(&state, &name, &path).await?;
        }
    }
    let mut data = tokio::fs::read(&path).await.map_err(crate::error::internal)?;
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
    if let Some(url) = q.server.as_deref().and_then(|s| server_url(s, host)) {
        let n = name.clone();
        let original = data.clone();
        match tokio::task::spawn_blocking(move || with_conf(&n, &original, &url)).await {
            Ok(Ok(with)) => data = with,
            // se entrega igual, sin la direccion (la bandeja la buscara o la pedira)
            Ok(Err(e)) => eprintln!("[labnas-tray] no se pudo agregar la configuracion a {name}: {e}"),
            Err(e) => eprintln!("[labnas-tray] {e}"),
        }
    }
    let mime = if name.ends_with(".exe") { "application/vnd.microsoft.portable-executable" } else { "application/gzip" };
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
    fn direccion_solo_si_coincide_con_el_host() {
        assert_eq!(server_url("https://192.168.1.10:3443/", "192.168.1.10:3443").as_deref(), Some("https://192.168.1.10:3443"));
        assert_eq!(server_url("http://LabNAS.local:3001", "labnas.local:3001").as_deref(), Some("http://LabNAS.local:3001"));
        assert_eq!(server_url("https://malo.com", "192.168.1.10:3443"), None);
        assert_eq!(server_url("javascript:alert(1)", "x"), None);
        assert_eq!(server_url("https://a/b", "a/b"), None);
    }

    #[test]
    fn agrega_conf_al_exe_y_al_tar() {
        let out = with_conf("x.exe", b"MZ...", "https://h:1").unwrap();
        assert!(out.starts_with(b"MZ..."));
        let n = u32::from_le_bytes(out[out.len() - 4..].try_into().unwrap()) as usize;
        assert_eq!(&out[out.len() - 12..out.len() - 4], TRAILER_MAGIC);
        let conf = std::str::from_utf8(&out[out.len() - 12 - n..out.len() - 12]).unwrap();
        assert!(conf.contains("url=https://h:1\n"));

        // tar.gz con labnas-tray/labnas-tray
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(3);
        h.set_mode(0o755);
        b.append_data(&mut h, "labnas-tray/labnas-tray", b"ELF".as_slice()).unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&b.into_inner().unwrap()).unwrap();
        let out = with_conf("x.tar.gz", &gz.finish().unwrap(), "http://h:2").unwrap();
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(out.as_slice()).read_to_end(&mut raw).unwrap();
        let mut ar = tar::Archive::new(raw.as_slice());
        let mut names = Vec::new();
        for e in ar.entries().unwrap() {
            let mut e = e.unwrap();
            let path = e.path().unwrap().to_string_lossy().into_owned();
            if path.ends_with(CONF_NAME) {
                let mut c = String::new();
                e.read_to_string(&mut c).unwrap();
                assert!(c.contains("url=http://h:2"));
            }
            names.push(path);
        }
        assert_eq!(names, ["labnas-tray/labnas-tray", "labnas-tray/labnas-tray.conf"]);
    }

    #[test]
    fn nombres_de_asset() {
        assert_eq!(asset_name("windows-x86_64", "v3.5.0").as_deref(), Some("labnas-tray-v3.5.0-windows-x86_64.exe"));
        assert_eq!(asset_name("linux-aarch64", "v3.5.0").as_deref(), Some("labnas-tray-v3.5.0-linux-aarch64.tar.gz"));
        assert_eq!(asset_name("../etc", "v3.5.0"), None);
    }
}
