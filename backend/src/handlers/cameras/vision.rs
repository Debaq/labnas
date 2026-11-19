//! Vision local para la deteccion en camaras: JPEG <-> RGB, YOLOX-nano (ONNX con
//! `tract`, Rust puro: sirve en el binario estatico) y deteccion de movimiento.
//!
//! YOLOX (Megvii, Apache-2.0). Entrada 416x416 BGR 0..255 con relleno 114 (letterbox);
//! salida [1, 3549, 85] por celda de las grillas 8/16/32: x, y, w, h (relativos a la
//! celda), objetividad y 80 clases COCO (ya con sigmoide).

use serde::Serialize;
use std::path::Path;
use tract_onnx::prelude::*;

pub const INPUT: usize = 416;
const STRIDES: [usize; 3] = [8, 16, 32];
const NMS_IOU: f32 = 0.45;

/// Imagen RGB 8 bits
pub struct Rgb {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

pub fn decode_jpeg(jpeg: &[u8]) -> Result<Rgb, String> {
    use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(jpeg, opts);
    let data = d.decode().map_err(|e| format!("JPEG invalido: {:?}", e))?;
    let (w, h) = d.dimensions().ok_or("JPEG sin dimensiones")?;
    if data.len() != w * h * 3 {
        return Err("JPEG con formato de color no soportado".into());
    }
    Ok(Rgb { w, h, data })
}

pub fn encode_jpeg(img: &Rgb, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, quality)
        .encode(&img.data, img.w as u16, img.h as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Clases COCO del modelo: (nombre en ingles, nombre en español)
pub const COCO: [(&str, &str); 80] = [
    ("person", "persona"), ("bicycle", "bicicleta"), ("car", "auto"), ("motorcycle", "moto"),
    ("airplane", "avion"), ("bus", "bus"), ("train", "tren"), ("truck", "camion"), ("boat", "bote"),
    ("traffic light", "semaforo"), ("fire hydrant", "grifo"), ("stop sign", "señal pare"),
    ("parking meter", "parquimetro"), ("bench", "banca"), ("bird", "pajaro"), ("cat", "gato"),
    ("dog", "perro"), ("horse", "caballo"), ("sheep", "oveja"), ("cow", "vaca"), ("elephant", "elefante"),
    ("bear", "oso"), ("zebra", "cebra"), ("giraffe", "jirafa"), ("backpack", "mochila"),
    ("umbrella", "paraguas"), ("handbag", "cartera"), ("tie", "corbata"), ("suitcase", "maleta"),
    ("frisbee", "frisbee"), ("skis", "esquis"), ("snowboard", "snowboard"), ("sports ball", "pelota"),
    ("kite", "volantin"), ("baseball bat", "bate"), ("baseball glove", "guante de beisbol"),
    ("skateboard", "skate"), ("surfboard", "tabla de surf"), ("tennis racket", "raqueta"),
    ("bottle", "botella"), ("wine glass", "copa"), ("cup", "taza"), ("fork", "tenedor"),
    ("knife", "cuchillo"), ("spoon", "cuchara"), ("bowl", "bol"), ("banana", "platano"),
    ("apple", "manzana"), ("sandwich", "sandwich"), ("orange", "naranja"), ("broccoli", "brocoli"),
    ("carrot", "zanahoria"), ("hot dog", "completo"), ("pizza", "pizza"), ("donut", "dona"),
    ("cake", "torta"), ("chair", "silla"), ("couch", "sillon"), ("potted plant", "planta"),
    ("bed", "cama"), ("dining table", "mesa"), ("toilet", "inodoro"), ("tv", "televisor"),
    ("laptop", "notebook"), ("mouse", "mouse"), ("remote", "control remoto"), ("keyboard", "teclado"),
    ("cell phone", "celular"), ("microwave", "microondas"), ("oven", "horno"), ("toaster", "tostadora"),
    ("sink", "lavaplatos"), ("refrigerator", "refrigerador"), ("book", "libro"), ("clock", "reloj"),
    ("vase", "florero"), ("scissors", "tijeras"), ("teddy bear", "peluche"),
    ("hair drier", "secador"), ("toothbrush", "cepillo de dientes"),
];

pub fn class_index(name: &str) -> Option<usize> {
    COCO.iter().position(|(en, es)| *en == name || *es == name)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Detection {
    pub label: String,
    pub score: f32,
    /// Caja en pixeles de la imagen original (esquina superior izquierda, ancho, alto)
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(skip)]
    pub class: usize,
}

/// Escala manteniendo proporcion a INPUTxINPUT, relleno 114, CHW en orden BGR (0..255)
pub fn letterbox(img: &Rgb) -> (Vec<f32>, f32) {
    let s = INPUT;
    let r = (s as f32 / img.w as f32).min(s as f32 / img.h as f32);
    let (nw, nh) = (((img.w as f32 * r) as usize).min(s), ((img.h as f32 * r) as usize).min(s));
    let mut t = vec![114.0f32; 3 * s * s];
    for y in 0..nh {
        let sy = (((y as f32 + 0.5) / r - 0.5).max(0.0) as usize).min(img.h - 1);
        for x in 0..nw {
            let sx = (((x as f32 + 0.5) / r - 0.5).max(0.0) as usize).min(img.w - 1);
            let p = (sy * img.w + sx) * 3;
            for c in 0..3 {
                t[c * s * s + y * s + x] = img.data[p + 2 - c] as f32;
            }
        }
    }
    (t, r)
}

fn iou(a: &Detection, b: &Detection) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.w).min(b.x + b.w);
    let y2 = (a.y + a.h).min(b.y + b.h);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Decodifica la salida de YOLOX (filas de 85) y aplica NMS por clase.
/// `keep` decide que clases interesan; `ratio` es el de `letterbox`.
pub fn decode_yolox(out: &[f32], ratio: f32, min_score: f32, keep: &dyn Fn(usize) -> bool) -> Vec<Detection> {
    let cols = 5 + COCO.len();
    let mut found = Vec::new();
    let mut row_i = 0;
    for &stride in &STRIDES {
        let g = INPUT / stride;
        for gy in 0..g {
            for gx in 0..g {
                let Some(row) = out.get(row_i * cols..(row_i + 1) * cols) else { return nms(found) };
                row_i += 1;
                let (class, cls_score) = row[5..]
                    .iter()
                    .enumerate()
                    .fold((0, 0.0f32), |best, (i, &v)| if v > best.1 { (i, v) } else { best });
                let score = row[4] * cls_score;
                if score < min_score || !keep(class) {
                    continue;
                }
                let s = stride as f32;
                let (cx, cy) = ((row[0] + gx as f32) * s, (row[1] + gy as f32) * s);
                let (w, h) = (row[2].exp() * s, row[3].exp() * s);
                found.push(Detection {
                    label: COCO[class].1.to_string(),
                    score,
                    x: (cx - w / 2.0) / ratio,
                    y: (cy - h / 2.0) / ratio,
                    w: w / ratio,
                    h: h / ratio,
                    class,
                });
            }
        }
    }
    nms(found)
}

fn nms(mut dets: Vec<Detection>) -> Vec<Detection> {
    dets.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<Detection> = Vec::new();
    for d in dets {
        if kept.iter().all(|k| k.class != d.class || iou(k, &d) < NMS_IOU) {
            kept.push(d);
        }
    }
    kept
}

type Plan = std::sync::Arc<TypedRunnableModel>;

/// Modelo YOLOX cargado (se carga una vez y se reutiliza)
pub struct Yolox(Plan);

impl Yolox {
    pub fn load(path: &Path) -> Result<Self, String> {
        let plan = tract_onnx::onnx()
            .model_for_path(path)
            .and_then(|m| m.with_input_fact(0, f32::fact([1, 3, INPUT, INPUT]).into()))
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable())
            .map_err(|e| format!("No se pudo cargar el modelo: {}", e))?;
        Ok(Self(plan))
    }

    pub fn detect(&self, img: &Rgb, min_score: f32, keep: &dyn Fn(usize) -> bool) -> Result<Vec<Detection>, String> {
        let (input, ratio) = letterbox(img);
        let tensor: Tensor = tract_ndarray::Array4::from_shape_vec((1, 3, INPUT, INPUT), input)
            .map_err(|e| e.to_string())?
            .into();
        let out = self.0.run(tvec!(tensor.into())).map_err(|e| format!("Error del modelo: {}", e))?;
        let view = out[0].to_plain_array_view::<f32>().map_err(|e| e.to_string())?;
        let flat: Vec<f32> = view.iter().copied().collect();
        Ok(decode_yolox(&flat, ratio, min_score, keep))
    }
}

// ─── Movimiento (sin IA) ───

/// Version gris reducida (64 px de ancho) para comparar cuadros
pub fn gray_small(img: &Rgb) -> (usize, usize, Vec<u8>) {
    let w = 64.min(img.w).max(1);
    let h = ((img.h * w) / img.w).max(1);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let p = ((y * img.h / h) * img.w + (x * img.w / w)) * 3;
            let d = &img.data[p..p + 3];
            out.push(((d[0] as u32 * 299 + d[1] as u32 * 587 + d[2] as u32 * 114) / 1000) as u8);
        }
    }
    (w, h, out)
}

/// Fraccion (0..1) de la imagen que cambio entre dos cuadros grises del mismo tamaño
pub fn motion_ratio(prev: &[u8], cur: &[u8]) -> f32 {
    if prev.len() != cur.len() || cur.is_empty() {
        return 0.0;
    }
    let changed = prev.iter().zip(cur).filter(|(a, b)| a.abs_diff(**b) > 30).count();
    changed as f32 / cur.len() as f32
}

// ─── Foto marcada ───

/// Dibuja el borde de cada deteccion (rojo, 3 px)
pub fn draw_boxes(img: &mut Rgb, dets: &[Detection]) {
    let t = 3usize;
    for d in dets {
        let x0 = d.x.max(0.0) as usize;
        let y0 = d.y.max(0.0) as usize;
        let x1 = ((d.x + d.w) as usize).min(img.w.saturating_sub(1));
        let y1 = ((d.y + d.h) as usize).min(img.h.saturating_sub(1));
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        let mut put = |x: usize, y: usize| {
            let p = (y * img.w + x) * 3;
            img.data[p..p + 3].copy_from_slice(&[230, 30, 30]);
        };
        for k in 0..t {
            for x in x0..=x1 {
                put(x, (y0 + k).min(y1));
                put(x, y1.saturating_sub(k).max(y0));
            }
            for y in y0..=y1 {
                put((x0 + k).min(x1), y);
                put(x1.saturating_sub(k).max(x0), y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(w: usize, h: usize, v: u8) -> Rgb {
        Rgb { w, h, data: vec![v; w * h * 3] }
    }

    #[test]
    fn letterbox_mantiene_proporcion_y_rellena() {
        let mut img = rgb(832, 416, 0);
        img.data[0] = 200; // R del primer pixel
        let (t, r) = letterbox(&img);
        assert_eq!(r, 0.5);
        assert_eq!(t.len(), 3 * INPUT * INPUT);
        // BGR: el canal 2 es el rojo
        assert_eq!(t[2 * INPUT * INPUT], 200.0);
        // Bajo la imagen (208 filas) queda el relleno 114
        assert_eq!(t[300 * INPUT + 10], 114.0);
    }

    #[test]
    fn decodifica_y_suprime_duplicados() {
        let cols = 85;
        let rows: usize = STRIDES.iter().map(|s| (INPUT / s).pow(2)).sum();
        assert_eq!(rows, 3549);
        let mut out = vec![0.0f32; rows * cols];
        // Celda (gx=10, gy=5) de la grilla de 8: persona al 90%, caja de 8*e^1 px
        let mut set = |i: usize, cls: usize, obj: f32| {
            let r = &mut out[i * cols..(i + 1) * cols];
            r[0] = 0.5;
            r[1] = 0.5;
            r[2] = 1.0;
            r[3] = 1.0;
            r[4] = obj;
            r[5 + cls] = 1.0;
        };
        set(5 * 52 + 10, 0, 0.9);
        set(5 * 52 + 11, 0, 0.8); // vecina casi igual: NMS la quita
        set(30 * 52 + 40, 16, 0.7); // perro en otro lugar
        set(0, 0, 0.1); // bajo el umbral

        let all = decode_yolox(&out, 1.0, 0.3, &|_| true);
        assert_eq!(all.len(), 2, "{:?}", all);
        assert_eq!((all[0].label.as_str(), all[1].label.as_str()), ("persona", "perro"));
        let p = &all[0];
        assert!((p.x + p.w / 2.0 - 84.0).abs() < 0.01 && (p.y + p.h / 2.0 - 44.0).abs() < 0.01);
        assert!((p.w - 8.0 * 1f32.exp()).abs() < 0.01);
        // Solo personas
        let only = decode_yolox(&out, 2.0, 0.3, &|c| c == 0);
        assert_eq!(only.len(), 1);
        assert!((only[0].x - all[0].x / 2.0).abs() < 0.01, "escala a la imagen original");
        assert_eq!(class_index("laptop"), class_index("notebook"));
    }

    #[test]
    fn movimiento_y_recuadros() {
        let a = rgb(128, 96, 10);
        let mut b = rgb(128, 96, 10);
        b.data[..128 * 48 * 3].fill(200); // mitad superior cambia
        let (w, h, ga) = gray_small(&a);
        let (_, _, gb) = gray_small(&b);
        assert_eq!((w, h), (64, 48));
        assert_eq!(motion_ratio(&ga, &ga), 0.0);
        assert!((motion_ratio(&ga, &gb) - 0.5).abs() < 0.05);

        let mut img = rgb(50, 40, 0);
        draw_boxes(&mut img, &[Detection { label: "x".into(), score: 1.0, x: 10.0, y: 10.0, w: 20.0, h: 15.0, class: 0 }]);
        assert_eq!(&img.data[(10 * 50 + 15) * 3..(10 * 50 + 15) * 3 + 3], &[230, 30, 30]);
        assert_eq!(&img.data[(17 * 50 + 20) * 3..(17 * 50 + 20) * 3 + 3], &[0, 0, 0], "el interior no se pinta");
        let jpeg = encode_jpeg(&img, 80).unwrap();
        let back = decode_jpeg(&jpeg).unwrap();
        assert_eq!((back.w, back.h), (50, 40));
    }

    /// Con el modelo real (no viene en el repo): LABNAS_TEST_MODEL_DIR con
    /// yolox_nano.onnx y bus.jpg
    #[test]
    #[ignore]
    fn modelo_real_detecta_personas() {
        let dir = std::path::PathBuf::from(std::env::var("LABNAS_TEST_MODEL_DIR").expect("LABNAS_TEST_MODEL_DIR"));
        let m = Yolox::load(&dir.join("yolox_nano.onnx")).unwrap();
        let img = decode_jpeg(&std::fs::read(dir.join("bus.jpg")).unwrap()).unwrap();
        let d = m.detect(&img, 0.5, &|c| c == 0).unwrap();
        assert!(d.len() >= 3, "{:?}", d);
        assert!(d.iter().all(|x| x.label == "persona"));
    }
}
