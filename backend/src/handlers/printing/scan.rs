//! Escaner desde la web: LabNAS usa SANE (`scanimage`) con el escaner conectado
//! al servidor (USB o red). Cada pagina se guarda como JPEG en una sesion
//! temporal; al terminar se descarga o se guarda en una carpeta del NAS como PDF
//! (armado aqui, sin dependencias externas) o como imagenes.

use super::*;
use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::Instant;
use crate::error::internal;

#[cfg_attr(test, allow(dead_code))]
const LIST_TIMEOUT: Duration = Duration::from_secs(40);
const SCAN_TIMEOUT: Duration = Duration::from_secs(240);
#[cfg_attr(test, allow(dead_code))]
const DEVICES_TTL: Duration = Duration::from_secs(300);
const SESSION_TTL: Duration = Duration::from_secs(3600);
const MAX_PAGES: usize = 60;
const JPEG_QUALITY: u8 = 85;
/// Resoluciones que se ofrecen cuando el escaner acepta un rango
const COMMON_DPI: &[u32] = &[75, 100, 150, 200, 300, 400, 600, 1200];

/// Tamaños de hoja (mm). Sin indicarlo, scanimage usa el area maxima que declara el driver,
/// y algunos (brscan3: 215,9 x 355,6 mm en una DCP-165C de vidrio A4) declaran mas de lo
/// que admite el vidrio: sane_start falla con "Invalid argument"
const PAPERS: &[(&str, &str, f64, f64)] = &[
    ("carta", "Carta (216 × 279 mm)", 215.9, 279.4),
    ("a4", "A4 (210 × 297 mm)", 210.0, 297.0),
    ("oficio", "Oficio (216 × 330 mm)", 215.9, 330.2),
    ("legal", "Legal (216 × 356 mm)", 215.9, 355.6),
];
const DEFAULT_PAPER: &str = "carta";

// ═══════════════════════════════════════
// Dispositivos y opciones
// ═══════════════════════════════════════

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ScanDevice {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct ScanDevicesResponse {
    /// `scanimage` esta instalado
    available: bool,
    devices: Vec<ScanDevice>,
    error: Option<String>,
}

type DeviceCache = Option<(Instant, Vec<ScanDevice>)>;
#[cfg_attr(test, allow(dead_code))]
static DEVICES: LazyLock<StdMutex<DeviceCache>> = LazyLock::new(Default::default);
/// Un escaneo a la vez (el escaner es uno solo y SANE no comparte el dispositivo)
static SCAN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
pub static TEST_SCANNERS: StdMutex<Vec<ScanDevice>> = StdMutex::new(Vec::new());

/// Salida de `scanimage -f "%d\t%v %m%n"` -> dispositivos. Se omiten las camaras
/// (backend v4l de SANE): no sirven para escanear documentos y confunden en la lista.
fn parse_devices(out: &str) -> Vec<ScanDevice> {
    out.lines()
        .filter_map(|l| {
            let (id, name) = l.split_once('\t')?;
            let id = id.trim();
            (!id.is_empty() && !id.starts_with("v4l:"))
                .then(|| ScanDevice { id: id.to_string(), name: name.trim().to_string() })
        })
        .collect()
}

#[cfg(test)]
async fn list_devices(_refresh: bool) -> Result<Vec<ScanDevice>, String> {
    Ok(TEST_SCANNERS.lock().unwrap().clone())
}

#[cfg(not(test))]
async fn list_devices(refresh: bool) -> Result<Vec<ScanDevice>, String> {
    if !refresh {
        if let Some((at, list)) = DEVICES.lock().ok().and_then(|d| d.clone()) {
            if at.elapsed() < DEVICES_TTL {
                return Ok(list);
            }
        }
    }
    let mut cmd = Command::new("scanimage");
    cmd.args(["-f", "%d\t%v %m%n"]).env("LANG", "C").kill_on_drop(true);
    let out = match tokio::time::timeout(LIST_TIMEOUT, cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(format!("No se pudo ejecutar scanimage: {}", e)),
        Err(_) => return Err("La busqueda de escaneres tardo demasiado".into()),
    };
    let list = parse_devices(&String::from_utf8_lossy(&out.stdout));
    if let Ok(mut d) = DEVICES.lock() {
        *d = Some((Instant::now(), list.clone()));
    }
    Ok(list)
}

#[cfg(test)]
async fn scanimage_installed() -> bool {
    true
}

#[cfg(not(test))]
async fn scanimage_installed() -> bool {
    Command::new("scanimage").arg("--version").output().await.map(|o| o.status.success()).unwrap_or(false)
}

#[derive(Deserialize)]
pub struct DevicesQuery {
    #[serde(default)]
    refresh: bool,
}

/// GET /api/printing/scan/devices[?refresh=true]
pub async fn scan_devices(Query(q): Query<DevicesQuery>) -> Json<ScanDevicesResponse> {
    if !scanimage_installed().await {
        return Json(ScanDevicesResponse {
            available: false,
            devices: Vec::new(),
            error: Some("SANE (scanimage) no esta instalado en el servidor".into()),
        });
    }
    match list_devices(q.refresh).await {
        Ok(devices) => Json(ScanDevicesResponse { available: true, devices, error: None }),
        Err(e) => Json(ScanDevicesResponse { available: true, devices: Vec::new(), error: Some(e) }),
    }
}

#[derive(Serialize, Default, Debug, PartialEq)]
pub struct ScanOptions {
    modes: Vec<String>,
    default_mode: Option<String>,
    resolutions: Vec<u32>,
    default_resolution: Option<u32>,
    sources: Vec<String>,
    default_source: Option<String>,
    /// Area maxima que declara el escaner (opciones -x / -y), en mm
    max_width_mm: Option<f64>,
    max_height_mm: Option<f64>,
    /// Tamaños de hoja que caben en esa area
    papers: Vec<Paper>,
    default_paper: Option<String>,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Paper {
    id: String,
    label: String,
    width_mm: f64,
    height_mm: f64,
}

/// Limite superior de un rango "0..215.9mm (in steps of 0.0999908) [215.88]"
fn range_max_mm(value: &str) -> Option<f64> {
    let main = value.trim().split(' ').next()?;
    let (_, max) = main.split_once("..")?;
    let max: f64 = max.trim_end_matches("mm").parse().ok()?;
    (max > 0.0 && max < 2000.0).then_some(max)
}

fn fill_papers(o: &mut ScanOptions) {
    let (Some(w), Some(h)) = (o.max_width_mm, o.max_height_mm) else { return };
    // Medio milimetro de tolerancia: los drivers redondean
    o.papers = PAPERS
        .iter()
        .filter(|(_, _, pw, ph)| *pw <= w + 0.5 && *ph <= h + 0.5)
        .map(|(id, label, pw, ph)| Paper { id: (*id).into(), label: (*label).into(), width_mm: *pw, height_mm: *ph })
        .collect();
    o.default_paper = o.papers.iter().find(|p| p.id == DEFAULT_PAPER).or(o.papers.first()).map(|p| p.id.clone());
}

/// "Lineart|Gray|Color [Color]" -> (valores, por defecto). None si la opcion esta inactiva.
fn split_option(rest: &str) -> Option<(Vec<String>, Option<String>)> {
    let rest = rest.trim();
    let (values, default) = match rest.rfind(" [") {
        Some(i) if rest.ends_with(']') => (&rest[..i], Some(rest[i + 2..rest.len() - 1].to_string())),
        _ => (rest, None),
    };
    if default.as_deref() == Some("inactive") {
        return None;
    }
    let values = values.split('|').map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect();
    Some((values, default))
}

/// Resoluciones: lista ("100|200|300dpi") o rango ("75..1200dpi (in steps of 1)")
fn parse_resolutions(values: &[String]) -> Vec<u32> {
    let num = |s: &str| s.trim().trim_end_matches("dpi").trim().parse::<f64>().ok().map(|f| f.round() as u32);
    if let [single] = values {
        let main = single.split(" (").next().unwrap_or(single);
        if let Some((a, b)) = main.split_once("..") {
            if let (Some(a), Some(b)) = (num(a), num(b)) {
                return COMMON_DPI.iter().copied().filter(|d| *d >= a && *d <= b).collect();
            }
        }
    }
    values.iter().filter_map(|v| num(v)).filter(|d| (50..=1200).contains(d)).collect()
}

/// Opciones de `scanimage -d <dev> -A`
fn parse_options(out: &str) -> ScanOptions {
    let mut o = ScanOptions::default();
    for line in out.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("-x ") {
            o.max_width_mm = range_max_mm(v);
            continue;
        }
        if let Some(v) = line.strip_prefix("-y ") {
            o.max_height_mm = range_max_mm(v);
            continue;
        }
        let Some(rest) = line.strip_prefix("--") else { continue };
        let Some((key, value)) = rest.split_once(' ') else { continue };
        let Some((values, default)) = split_option(value) else { continue };
        match key {
            "mode" => {
                o.modes = values;
                o.default_mode = default;
            }
            "source" => {
                o.sources = values;
                o.default_source = default;
            }
            "resolution" => {
                o.resolutions = parse_resolutions(&values);
                o.default_resolution = default.and_then(|d| d.trim_end_matches("dpi").parse::<f64>().ok()).map(|f| f.round() as u32);
            }
            _ => {}
        }
    }
    fill_papers(&mut o);
    o
}

async fn known_device(device: &str) -> Result<(), (StatusCode, String)> {
    let mut list = list_devices(false).await.unwrap_or_default();
    if !list.iter().any(|d| d.id == device) {
        list = list_devices(true).await.unwrap_or_default();
    }
    if list.iter().any(|d| d.id == device) {
        Ok(())
    } else {
        Err((StatusCode::NOT_FOUND, "Escaner no encontrado".into()))
    }
}

#[cfg(test)]
async fn device_options(_device: &str) -> ScanOptions {
    ScanOptions {
        modes: vec!["Gray".into(), "Color".into()],
        default_mode: Some("Color".into()),
        resolutions: vec![150, 300],
        default_resolution: Some(150),
        ..Default::default()
    }
}

/// Argumentos de area (-l -t -x -y) para la hoja elegida, o ninguno si el escaner no
/// informa su area. Sin hoja elegida: Carta (o la primera que quepa)
fn paper_args(paper: Option<&str>, opts: &ScanOptions) -> Result<Vec<String>, (StatusCode, String)> {
    let wanted = paper.filter(|p| !p.is_empty());
    let id = wanted.or(opts.default_paper.as_deref());
    let Some(id) = id else { return Ok(Vec::new()) };
    let Some(p) = opts.papers.iter().find(|p| p.id == id) else {
        return if wanted.is_some() {
            Err((StatusCode::BAD_REQUEST, format!("Tamaño de hoja no admitido por este escaner: {}", id)))
        } else {
            Ok(Vec::new())
        };
    };
    let w = p.width_mm.min(opts.max_width_mm.unwrap_or(p.width_mm));
    let h = p.height_mm.min(opts.max_height_mm.unwrap_or(p.height_mm));
    Ok(vec!["-l".into(), "0".into(), "-t".into(), "0".into(), "-x".into(), format!("{:.1}", w), "-y".into(), format!("{:.1}", h)])
}

#[cfg(not(test))]
async fn device_options(device: &str) -> ScanOptions {
    let mut cmd = Command::new("scanimage");
    cmd.args(["-d", device, "-A"]).env("LANG", "C").kill_on_drop(true);
    match tokio::time::timeout(LIST_TIMEOUT, cmd.output()).await {
        Ok(Ok(o)) => parse_options(&String::from_utf8_lossy(&o.stdout)),
        _ => ScanOptions::default(),
    }
}

#[derive(Deserialize)]
pub struct DeviceQuery {
    device: String,
}

/// GET /api/printing/scan/options?device=...
pub async fn scan_options(Query(q): Query<DeviceQuery>) -> Result<Json<ScanOptions>, (StatusCode, String)> {
    known_device(&q.device).await?;
    let Ok(_guard) = SCAN_LOCK.try_lock() else {
        return Err((StatusCode::CONFLICT, "El escaner esta ocupado".into()));
    };
    Ok(Json(device_options(&q.device).await))
}

// ═══════════════════════════════════════
// Imagen: PNM -> JPEG y PDF
// ═══════════════════════════════════════

#[derive(Debug, PartialEq)]
struct Raster {
    width: usize,
    height: usize,
    /// 1 = gris, 3 = RGB (8 bits por canal)
    channels: usize,
    data: Vec<u8>,
}

/// Lee PNM binario (P4 blanco y negro, P5 gris, P6 color; 8 o 16 bits)
fn parse_pnm(buf: &[u8]) -> Result<Raster, String> {
    let mut pos = 0usize;
    let token = |pos: &mut usize| -> Result<String, String> {
        loop {
            while *pos < buf.len() && buf[*pos].is_ascii_whitespace() {
                *pos += 1;
            }
            if *pos < buf.len() && buf[*pos] == b'#' {
                while *pos < buf.len() && buf[*pos] != b'\n' {
                    *pos += 1;
                }
                continue;
            }
            break;
        }
        let start = *pos;
        while *pos < buf.len() && !buf[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
        if start == *pos {
            return Err("imagen PNM incompleta".into());
        }
        Ok(String::from_utf8_lossy(&buf[start..*pos]).into_owned())
    };
    let magic = token(&mut pos)?;
    let num = |s: String| s.parse::<usize>().map_err(|_| "cabecera PNM invalida".to_string());
    let width = num(token(&mut pos)?)?;
    let height = num(token(&mut pos)?)?;
    let maxval = if magic == "P4" { 1 } else { num(token(&mut pos)?)? };
    pos += 1; // un espacio separa la cabecera de los datos
    if width == 0 || height == 0 || width > 65_535 || height > 65_535 {
        return Err(format!("tamano de imagen no soportado: {}x{}", width, height));
    }
    let data = buf.get(pos..).unwrap_or_default();
    let wide = maxval > 255;
    match magic.as_str() {
        "P4" => {
            let row = width.div_ceil(8);
            if data.len() < row * height {
                return Err("imagen PNM truncada".into());
            }
            let mut out = Vec::with_capacity(width * height);
            for y in 0..height {
                for x in 0..width {
                    let bit = data[y * row + x / 8] & (0x80 >> (x % 8));
                    out.push(if bit != 0 { 0 } else { 255 });
                }
            }
            Ok(Raster { width, height, channels: 1, data: out })
        }
        "P5" | "P6" => {
            let channels = if magic == "P5" { 1 } else { 3 };
            let samples = width * height * channels;
            let need = if wide { samples * 2 } else { samples };
            if data.len() < need {
                return Err("imagen PNM truncada".into());
            }
            let data = if wide { data[..need].chunks(2).map(|c| c[0]).collect() } else { data[..need].to_vec() };
            Ok(Raster { width, height, channels, data })
        }
        other => Err(format!("formato de imagen no soportado: {}", other)),
    }
}

fn encode_jpeg(r: &Raster, dpi: u32) -> Result<Vec<u8>, String> {
    use jpeg_encoder::{ColorType, Encoder, PixelDensity, PixelDensityUnit};
    let mut out = Vec::new();
    let mut enc = Encoder::new(&mut out, JPEG_QUALITY);
    let d = dpi.clamp(1, u16::MAX as u32) as u16;
    enc.set_density(PixelDensity { density: (d, d), unit: PixelDensityUnit::Inches });
    let color = if r.channels == 1 { ColorType::Luma } else { ColorType::Rgb };
    enc.encode(&r.data, r.width as u16, r.height as u16, color).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Pagina de PDF a partir de un JPEG
struct PdfPage<'a> {
    jpeg: &'a [u8],
    width: usize,
    height: usize,
    gray: bool,
    dpi: u32,
}

/// PDF minimo: cada pagina es un JPEG incrustado (DCTDecode) del tamano real escaneado
fn build_pdf(pages: &[PdfPage]) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let n = pages.len();
    // Objetos: 1 catalogo, 2 paginas, luego 3 por pagina (pagina, contenido, imagen)
    let page_obj = |i: usize| 3 + i * 3;
    let obj = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", page_obj(i))).collect();
    obj(&mut out, &mut offsets, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), n).as_bytes());
    for (i, p) in pages.iter().enumerate() {
        let dpi = p.dpi.max(1) as f64;
        let (w, h) = (p.width as f64 * 72.0 / dpi, p.height as f64 * 72.0 / dpi);
        let po = page_obj(i);
        obj(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.2} {h:.2}] /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>",
                po + 2,
                po + 1
            )
            .as_bytes(),
        );
        let content = format!("q {w:.2} 0 0 {h:.2} 0 0 cm /Im0 Do Q");
        obj(&mut out, &mut offsets, format!("<< /Length {} >>\nstream\n{}\nendstream", content.len(), content).as_bytes());
        let mut img = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /{} /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            p.width,
            p.height,
            if p.gray { "DeviceGray" } else { "DeviceRGB" },
            p.jpeg.len()
        )
        .into_bytes();
        img.extend_from_slice(p.jpeg);
        img.extend_from_slice(b"\nendstream");
        obj(&mut out, &mut offsets, &img);
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", o).as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", offsets.len() + 1, xref).as_bytes(),
    );
    out
}

// ═══════════════════════════════════════
// Sesiones de escaneo (paginas en un temporal)
// ═══════════════════════════════════════

#[derive(Clone, Serialize)]
pub struct ScanPage {
    #[serde(skip)]
    path: PathBuf,
    width: usize,
    height: usize,
    dpi: u32,
    gray: bool,
    size: u64,
}

struct ScanSession {
    owner: String,
    dir: PathBuf,
    pages: Vec<ScanPage>,
    touched: Instant,
}

static SESSIONS: LazyLock<StdMutex<HashMap<String, ScanSession>>> = LazyLock::new(Default::default);

/// Borra sesiones abandonadas (y sus temporales)
fn purge_sessions() {
    let stale: Vec<PathBuf> = match SESSIONS.lock() {
        Ok(mut s) => {
            let old: Vec<String> = s.iter().filter(|(_, v)| v.touched.elapsed() > SESSION_TTL).map(|(k, _)| k.clone()).collect();
            old.iter().filter_map(|k| s.remove(k)).map(|v| v.dir).collect()
        }
        Err(_) => Vec::new(),
    };
    for d in stale {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[derive(Serialize)]
pub struct SessionView {
    id: String,
    pages: Vec<ScanPage>,
}

fn session_view(id: &str, user: &str) -> Result<SessionView, (StatusCode, String)> {
    let mut s = SESSIONS.lock().map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Estado corrupto".to_string()))?;
    let sess = s.get_mut(id).filter(|x| x.owner == user).ok_or((StatusCode::NOT_FOUND, "Sesion de escaneo no encontrada o vencida".to_string()))?;
    sess.touched = Instant::now();
    Ok(SessionView { id: id.to_string(), pages: sess.pages.clone() })
}

fn session_pages(id: &str, user: &str) -> Result<Vec<ScanPage>, (StatusCode, String)> {
    Ok(session_view(id, user)?.pages)
}

#[derive(Deserialize)]
pub struct ScanRequest {
    /// Sesion a la que se agrega la pagina (nueva si falta)
    session: Option<String>,
    device: String,
    mode: Option<String>,
    resolution: Option<u32>,
    source: Option<String>,
    /// carta | a4 | oficio | legal
    #[serde(default)]
    paper: Option<String>,
}

/// Mensaje legible a partir del stderr de scanimage
fn scan_error(stderr: &str) -> String {
    let s = stderr.to_lowercase();
    if s.contains("access to resource has been denied") || s.contains("permission denied") {
        "LabNAS no tiene permiso para usar el escaner (ver docs/ESCANER.md: grupo scanner/lp y reglas udev)".into()
    } else if s.contains("device busy") {
        "El escaner esta ocupado".into()
    } else if s.contains("no such file") || (s.contains("invalid argument") && s.contains("open of device")) {
        "No se encontro el escaner: revisa que este encendido y conectado".into()
    } else if s.contains("document feeder") && s.contains("out of documents") {
        "No hay hojas en el alimentador".into()
    } else if s.contains("sane_start") && s.contains("invalid argument") {
        "El escaner rechazo el tamaño de hoja: elige Carta o A4 (algunos escaneres declaran un area mas larga que su vidrio)".into()
    } else {
        let msg = stderr.trim();
        format!("Error al escanear: {}", if msg.is_empty() { "scanimage termino sin imagen" } else { msg })
    }
}

#[cfg(test)]
pub static TEST_SCAN_OUTPUT: StdMutex<Option<Vec<u8>>> = StdMutex::new(None);

async fn run_scanimage(req: &ScanRequest, opts: &ScanOptions) -> Result<Vec<u8>, (StatusCode, String)> {
    let mut args: Vec<String> = vec!["-d".into(), req.device.clone(), "--format=pnm".into()];
    let pick = |v: &Option<String>, list: &[String], flag: &str, args: &mut Vec<String>| -> Result<(), (StatusCode, String)> {
        if let Some(v) = v.as_ref().filter(|v| !v.is_empty()) {
            if !list.contains(v) {
                return Err((StatusCode::BAD_REQUEST, format!("Valor no valido para {}: {}", flag, v)));
            }
            args.push(format!("--{}", flag));
            args.push(v.clone());
        }
        Ok(())
    };
    pick(&req.mode, &opts.modes, "mode", &mut args)?;
    pick(&req.source, &opts.sources, "source", &mut args)?;
    if let Some(r) = req.resolution {
        if !opts.resolutions.is_empty() && !opts.resolutions.contains(&r) {
            return Err((StatusCode::BAD_REQUEST, format!("Resolucion no soportada: {}", r)));
        }
        args.push("--resolution".into());
        args.push(r.to_string());
    }
    args.extend(paper_args(req.paper.as_deref(), opts)?);

    #[cfg(test)]
    if let Some(out) = TEST_SCAN_OUTPUT.lock().unwrap().clone() {
        return Ok(out);
    }

    let mut cmd = Command::new("scanimage");
    cmd.args(&args).env("LANG", "C").kill_on_drop(true);
    let out = match tokio::time::timeout(SCAN_TIMEOUT, cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo ejecutar scanimage: {}", e))),
        Err(_) => return Err((StatusCode::GATEWAY_TIMEOUT, "El escaneo tardo demasiado".into())),
    };
    if !out.status.success() || out.stdout.is_empty() {
        return Err((StatusCode::BAD_GATEWAY, scan_error(&String::from_utf8_lossy(&out.stderr))));
    }
    Ok(out.stdout)
}

/// POST /api/printing/scan/pages: escanea una pagina y la agrega a la sesion
pub async fn scan_page(
    State(state): State<AppState>,
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
    Json(req): Json<ScanRequest>,
) -> Result<Json<SessionView>, (StatusCode, String)> {
    purge_sessions();
    known_device(&req.device).await?;
    if let Some(id) = &req.session {
        if session_pages(id, &session.username)?.len() >= MAX_PAGES {
            return Err((StatusCode::BAD_REQUEST, format!("Maximo {} paginas por documento", MAX_PAGES)));
        }
    }
    let Ok(_guard) = SCAN_LOCK.try_lock() else {
        return Err((StatusCode::CONFLICT, "El escaner esta ocupado: espera a que termine el escaneo en curso".into()));
    };

    let opts = device_options(&req.device).await;
    let dpi = req.resolution.or(opts.default_resolution).unwrap_or(150);
    let pnm = run_scanimage(&req, &opts).await?;
    let (jpeg, raster_info) = tokio::task::spawn_blocking(move || {
        let r = parse_pnm(&pnm)?;
        let jpeg = encode_jpeg(&r, dpi)?;
        Ok::<_, String>((jpeg, (r.width, r.height, r.channels == 1)))
    })
    .await
    .map_err(internal)?
    .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Imagen del escaner no valida: {}", e)))?;

    // Sesion: existente del usuario o nueva
    let id = match &req.session {
        Some(id) => {
            session_view(id, &session.username)?;
            id.clone()
        }
        None => {
            let id = uuid::Uuid::new_v4().simple().to_string();
            let dir = std::env::temp_dir().join(format!("labnas-scan-{}", id));
            tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
            if let Ok(mut s) = SESSIONS.lock() {
                s.insert(id.clone(), ScanSession { owner: session.username.clone(), dir, pages: Vec::new(), touched: Instant::now() });
            }
            id
        }
    };
    let dir = SESSIONS.lock().ok().and_then(|s| s.get(&id).map(|x| x.dir.clone()));
    let Some(dir) = dir else { return Err((StatusCode::NOT_FOUND, "Sesion vencida".into())) };
    let path = dir.join(format!("{}.jpg", uuid::Uuid::new_v4().simple()));
    tokio::fs::write(&path, &jpeg).await.map_err(internal)?;
    let (width, height, gray) = raster_info;
    let page = ScanPage { path, width, height, dpi, gray, size: jpeg.len() as u64 };
    if let Ok(mut s) = SESSIONS.lock() {
        if let Some(sess) = s.get_mut(&id) {
            sess.pages.push(page);
            sess.touched = Instant::now();
        }
    }
    state.log_activity("Escaneo", &format!("Escaneo una pagina ({} ppp) en {}", dpi, req.device), &session.username).await;
    Ok(Json(session_view(&id, &session.username)?))
}

/// GET /api/printing/scan/sessions/{id}
pub async fn get_scan_session(
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<SessionView>, (StatusCode, String)> {
    Ok(Json(session_view(&id, &session.username)?))
}

/// GET /api/printing/scan/sessions/{id}/pages/{n}: vista previa (JPEG)
pub async fn scan_page_image(
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
    Path((id, n)): Path<(String, usize)>,
) -> Result<Response, (StatusCode, String)> {
    let pages = session_pages(&id, &session.username)?;
    let page = pages.get(n).ok_or((StatusCode::NOT_FOUND, "Pagina no encontrada".to_string()))?;
    let data = tokio::fs::read(&page.path).await.map_err(internal)?;
    Ok((
        [(axum::http::header::CONTENT_TYPE, "image/jpeg"), (axum::http::header::CACHE_CONTROL, "private, max-age=3600")],
        data,
    )
        .into_response())
}

/// DELETE /api/printing/scan/sessions/{id}/pages/{n}
pub async fn delete_scan_page(
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
    Path((id, n)): Path<(String, usize)>,
) -> Result<Json<SessionView>, (StatusCode, String)> {
    session_view(&id, &session.username)?;
    let removed = SESSIONS.lock().ok().and_then(|mut s| {
        let sess = s.get_mut(&id)?;
        (n < sess.pages.len()).then(|| sess.pages.remove(n))
    });
    let page = removed.ok_or((StatusCode::NOT_FOUND, "Pagina no encontrada".to_string()))?;
    let _ = tokio::fs::remove_file(&page.path).await;
    Ok(Json(session_view(&id, &session.username)?))
}

/// DELETE /api/printing/scan/sessions/{id}
pub async fn delete_scan_session(
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    session_view(&id, &session.username)?;
    let dir = SESSIONS.lock().ok().and_then(|mut s| s.remove(&id)).map(|s| s.dir);
    if let Some(d) = dir {
        let _ = tokio::fs::remove_dir_all(d).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScanFormat {
    Pdf,
    Jpg,
}

/// Archivos de salida: un PDF con todas las paginas, o un JPEG por pagina
async fn render(pages: &[ScanPage], format: ScanFormat, base: &str) -> Result<Vec<(String, Vec<u8>)>, (StatusCode, String)> {
    if pages.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No hay paginas escaneadas".into()));
    }
    let mut jpegs = Vec::with_capacity(pages.len());
    for p in pages {
        jpegs.push(tokio::fs::read(&p.path).await.map_err(internal)?);
    }
    Ok(match format {
        ScanFormat::Pdf => {
            let list: Vec<PdfPage> = pages
                .iter()
                .zip(&jpegs)
                .map(|(p, j)| PdfPage { jpeg: j, width: p.width, height: p.height, gray: p.gray, dpi: p.dpi })
                .collect();
            vec![(format!("{}.pdf", base), build_pdf(&list))]
        }
        ScanFormat::Jpg if jpegs.len() == 1 => vec![(format!("{}.jpg", base), jpegs.remove(0))],
        ScanFormat::Jpg => jpegs.into_iter().enumerate().map(|(i, j)| (format!("{}-{}.jpg", base, i + 1), j)).collect(),
    })
}

/// Nombre base seguro (sin extension ni rutas)
fn base_name(name: Option<&str>) -> String {
    let default = format!("escaneo-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let raw = name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&default);
    let raw = raw.strip_suffix(".pdf").or_else(|| raw.strip_suffix(".jpg")).unwrap_or(raw);
    crate::storage::sanitize_filename(raw)
        .map(|n| n.chars().filter(|c| !c.is_control()).take(100).collect::<String>())
        .filter(|n| !n.starts_with('.'))
        .unwrap_or(default)
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    format: ScanFormat,
    name: Option<String>,
}

/// GET /api/printing/scan/sessions/{id}/download?format=pdf|jpg (varias imagenes van en un ZIP)
pub async fn download_scan(
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
    Path(id): Path<String>,
    Query(q): Query<DownloadQuery>,
) -> Result<Response, (StatusCode, String)> {
    let pages = session_pages(&id, &session.username)?;
    let base = base_name(q.name.as_deref());
    let mut files = render(&pages, q.format, &base).await?;
    let (name, data, ctype) = if files.len() == 1 {
        let (n, d) = files.remove(0);
        let ctype = if q.format == ScanFormat::Pdf { "application/pdf" } else { "image/jpeg" };
        (n, d, ctype)
    } else {
        let zip = tokio::task::spawn_blocking(move || zip_files(&files))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        (format!("{}.zip", base), zip, "application/zip")
    };
    let disposition = format!("attachment; filename*=UTF-8''{}", urlencoding::encode(&name));
    Ok((
        [(axum::http::header::CONTENT_TYPE, ctype.to_string()), (axum::http::header::CONTENT_DISPOSITION, disposition)],
        data,
    )
        .into_response())
}

fn zip_files(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            z.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
            z.write_all(data).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

#[derive(Deserialize)]
pub struct SaveRequest {
    folder: String,
    name: Option<String>,
    format: ScanFormat,
}

/// Ruta libre: si existe, agrega " (2)", " (3)"...
fn free_path(dir: &std::path::Path, file: &str) -> PathBuf {
    let candidate = dir.join(file);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match file.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{}", e)),
        None => (file.to_string(), String::new()),
    };
    (2..1000).map(|i| dir.join(format!("{} ({}){}", stem, i, ext))).find(|p| !p.exists()).unwrap_or(candidate)
}

/// POST /api/printing/scan/sessions/{id}/save: guarda en una carpeta del NAS
pub async fn save_scan(
    State(state): State<AppState>,
    axum::Extension(session): axum::Extension<crate::state::SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<SaveRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pages = session_pages(&id, &session.username)?;
    let st = crate::storage::Storage::load(&state.db).await?;
    let dir = st.resolve_for(&session, &req.folder, crate::storage::Op::Write)?;
    if !dir.is_dir() {
        return Err((StatusCode::NOT_FOUND, "La carpeta no existe".into()));
    }
    let files = render(&pages, req.format, &base_name(req.name.as_deref())).await?;
    let mut saved = Vec::new();
    for (name, data) in files {
        let path = free_path(&dir, &name);
        tokio::fs::write(&path, &data).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo guardar: {}", e)))?;
        saved.push(path.to_string_lossy().to_string());
    }
    state
        .log_activity("Escaneo", &format!("Guardo {} pagina(s) escaneadas en {}", pages.len(), saved.join(", ")), &session.username)
        .await;
    Ok(Json(serde_json::json!({ "saved": saved })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispositivos_y_opciones() {
        let d = parse_devices("brother3:bus2;dev1\tBrother DCP-165C\nv4l:/dev/video0\tNoname HD Webcam\nairscan:e0:HP\tHP eSCL\n\n");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0], ScanDevice { id: "brother3:bus2;dev1".into(), name: "Brother DCP-165C".into() });

        let brother = "All options specific to device `brother3:bus2;dev1':\n  Mode:\n    --mode Black & White|Gray[Error Diffusion]|True Gray|24bit Color [24bit Color]\n    --resolution 100|150|200|300|400|600|1200|2400|4800|9600dpi [200]\n    --source FlatBed [FlatBed]\n    --brightness -50..50% (in steps of 1) [inactive]\n";
        let o = parse_options(brother);
        assert_eq!(o.modes, vec!["Black & White", "Gray[Error Diffusion]", "True Gray", "24bit Color"]);
        assert_eq!(o.default_mode.as_deref(), Some("24bit Color"));
        assert_eq!(o.resolutions, vec![100, 150, 200, 300, 400, 600, 1200]);
        assert_eq!(o.default_resolution, Some(200));
        assert_eq!(o.sources, vec!["FlatBed"]);

        // Area declarada por brscan3 (Legal) y hojas que caben
        assert_eq!((o.max_width_mm, o.max_height_mm), (None, None));
        let geo = format!("{}    -l 0..215.9mm (in steps of 0.0999908) [0]\n    -t 0..355.6mm (in steps of 0.0999908) [0]\n    -x 0..215.9mm (in steps of 0.0999908) [215.88]\n    -y 0..355.6mm (in steps of 0.0999908) [355.567]\n", brother);
        let o = parse_options(&geo);
        assert_eq!((o.max_width_mm, o.max_height_mm), (Some(215.9), Some(355.6)));
        assert_eq!(o.papers.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["carta", "a4", "oficio", "legal"]);
        assert_eq!(o.default_paper.as_deref(), Some("carta"));
        // Sin elegir: Carta, no el area maxima (que hacia fallar a la DCP-165C)
        assert_eq!(paper_args(None, &o).unwrap(), vec!["-l", "0", "-t", "0", "-x", "215.9", "-y", "279.4"]);
        assert_eq!(paper_args(Some("a4"), &o).unwrap()[5..], ["210.0", "-y", "297.0"]);
        // Un escaner A4 no ofrece Oficio
        let a4 = parse_options("    -x 0..216mm [216]\n    -y 0..297mm [297]\n");
        assert_eq!(a4.papers.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["carta", "a4"]);
        assert_eq!(paper_args(Some("oficio"), &a4).unwrap_err().0, StatusCode::BAD_REQUEST);
        // Sin area informada no se fuerza nada
        assert!(paper_args(None, &ScanOptions::default()).unwrap().is_empty());
        assert!(scan_error("scanimage: sane_start: Invalid argument").contains("Carta o A4"));

        let range = parse_options("    --resolution 75..600dpi (in steps of 1) [75]\n    --mode Lineart|Gray|Color [inactive]\n");
        assert_eq!(range.resolutions, vec![75, 100, 150, 200, 300, 400, 600]);
        assert!(range.modes.is_empty());
    }

    #[test]
    fn pnm_a_jpeg_y_pdf() {
        // P6 2x2 color con comentario
        let mut p6 = b"P6\n# scanimage\n2 2\n255\n".to_vec();
        p6.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9]);
        let r = parse_pnm(&p6).unwrap();
        assert_eq!((r.width, r.height, r.channels), (2, 2, 3));
        // P4 1 bit: 10 pixeles por fila (2 bytes), 1 = negro
        let p4 = [b"P4\n10 1\n".as_slice(), &[0b1000_0000, 0b0100_0000]].concat();
        let r4 = parse_pnm(&p4).unwrap();
        assert_eq!(r4.data[0], 0);
        assert_eq!(r4.data[1], 255);
        assert_eq!(r4.data[9], 0);
        // P5 16 bits: se toma el byte alto
        let p5 = [b"P5 1 1 65535\n".as_slice(), &[0xAB, 0xCD]].concat();
        assert_eq!(parse_pnm(&p5).unwrap().data, vec![0xAB]);
        assert!(parse_pnm(b"P6\n2 2\n255\n\x00").is_err(), "truncado");
        assert!(parse_pnm(b"P3\n1 1\n255\n0 0 0").is_err(), "ASCII no");

        let jpeg = encode_jpeg(&r, 300).unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8]));
        let pdf = build_pdf(&[
            PdfPage { jpeg: &jpeg, width: 2, height: 2, gray: false, dpi: 300 },
            PdfPage { jpeg: &jpeg, width: 2, height: 2, gray: false, dpi: 300 },
        ]);
        assert!(pdf.starts_with(b"%PDF-1.4") && pdf.ends_with(b"%%EOF\n"));
        let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).rposition(|w| w == needle);
        assert!(find(&pdf, b"/Count 2").is_some());
        // startxref apunta a la tabla y la tabla al inicio de cada objeto (offsets en bytes)
        let sx = find(&pdf, b"startxref\n").unwrap() + 10;
        let xref_at: usize = String::from_utf8_lossy(&pdf[sx..]).lines().next().unwrap().parse().unwrap();
        assert!(pdf[xref_at..].starts_with(b"xref\n"));
        let table = String::from_utf8_lossy(&pdf[xref_at..]).into_owned();
        for (i, line) in table.lines().skip(3).take(8).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "objeto {}", i + 1);
        }
    }

    #[test]
    fn nombres() {
        assert_eq!(base_name(Some("../../x/informe.pdf")), "informe");
        assert!(base_name(Some(".oculto")).starts_with("escaneo-"));
        assert!(base_name(None).starts_with("escaneo-"));
        assert!(scan_error("scanimage: open of device x failed: Access to resource has been denied").contains("permiso"));
    }
}
