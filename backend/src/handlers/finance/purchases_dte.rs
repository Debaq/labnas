//! Lectura de la factura electronica del SII (DTE en XML) y propuesta de datos con IA para
//! PDF o imagenes (idea 55). Un lector pequeño propio: solo busca las etiquetas que importan
//! en el encabezado del documento, no expande entidades externas ni valida la firma.

/// Datos de un documento tributario electronico
#[derive(Debug, Clone, PartialEq)]
pub struct Dte {
    pub tipo: u32,
    pub folio: String,
    pub fecha: String,
    pub rut_emisor: String,
    pub razon_social: String,
    pub neto: Option<f64>,
    pub iva: Option<f64>,
    pub total: f64,
}

impl Dte {
    /// Clase de documento que se guarda en `fin_documents.kind`
    pub fn kind(&self) -> &'static str {
        match self.tipo {
            39 | 41 => "boleta",
            _ => "factura",
        }
    }
}

/// Texto del XML: UTF-8 o, si no calza, ISO-8859-1 (como los emite el SII)
fn as_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

fn unescape(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")).unwrap_or(s);
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Contenido de la primera etiqueta `<name>...</name>` (con o sin prefijo de espacio de nombres)
fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = xml[from..].find('<') {
        let start = from + i + 1;
        let end_name = xml[start..].find(['>', ' ', '/', '\n', '\r', '\t']).map(|e| start + e)?;
        let found = &xml[start..end_name];
        let local = found.rsplit(':').next().unwrap_or(found);
        if local == name {
            let gt = xml[end_name..].find('>')? + end_name;
            if xml.as_bytes()[gt - 1] == b'/' {
                return Some("");
            }
            let body = gt + 1;
            let close = xml[body..].find(&format!("</{}", found))? + body;
            return Some(&xml[body..close]);
        }
        from = start;
    }
    None
}

fn number(s: Option<&str>) -> Option<f64> {
    let v: f64 = unescape(s?).replace(',', ".").parse().ok()?;
    (v.is_finite() && (0.0..1e12).contains(&v)).then_some(v)
}

/// Lee un DTE (factura, factura exenta, boleta...). Error en español si no sirve.
pub fn parse_dte(bytes: &[u8]) -> Result<Dte, String> {
    let xml = as_text(bytes);
    if xml.to_ascii_uppercase().contains("<!ENTITY") {
        return Err("El XML declara entidades: no se acepta".into());
    }
    let doc = tag(&xml, "Documento").ok_or("No parece una factura electronica del SII (falta <Documento>)")?;
    let head = tag(doc, "Encabezado").unwrap_or(doc);
    let id = tag(head, "IdDoc").ok_or("Falta <IdDoc> en el XML")?;
    let tipo: u32 = tag(id, "TipoDTE").and_then(|t| t.trim().parse().ok()).ok_or("Falta el TipoDTE")?;
    if ![33, 34, 39, 41, 46].contains(&tipo) {
        return Err(format!("El documento es de tipo {} y no sirve como respaldo de una compra (se aceptan facturas y boletas)", tipo));
    }
    let folio = tag(id, "Folio").map(unescape).filter(|f| !f.is_empty() && f.len() <= 20).ok_or("Falta el Folio")?;
    let fecha = tag(id, "FchEmis").map(unescape).filter(|f| chrono::NaiveDate::parse_from_str(f, "%Y-%m-%d").is_ok()).ok_or("Falta la fecha de emision (FchEmis) o no es valida")?;
    let emisor = tag(head, "Emisor").ok_or("Falta el <Emisor>")?;
    let rut_emisor = tag(emisor, "RUTEmisor").map(unescape).filter(|r| !r.is_empty() && r.len() <= 12).ok_or("Falta el RUTEmisor")?;
    let razon_social = tag(emisor, "RznSoc").or_else(|| tag(emisor, "RznSocEmisor")).map(unescape).unwrap_or_default();
    let totales = tag(head, "Totales").ok_or("Faltan los <Totales>")?;
    let total = number(tag(totales, "MntTotal")).ok_or("Falta el MntTotal")?;
    Ok(Dte {
        tipo,
        folio,
        fecha,
        rut_emisor,
        razon_social: razon_social.chars().take(200).collect(),
        neto: number(tag(totales, "MntNeto")),
        iva: number(tag(totales, "IVA")),
        total,
    })
}

/// Datos propuestos por la IA (siempre los confirma una persona)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Proposal {
    pub kind: String,
    pub folio: String,
    pub issued_on: String,
    pub issuer_rut: String,
    pub issuer_name: String,
    pub net: Option<f64>,
    pub tax: Option<f64>,
    pub total: Option<f64>,
}

const AI_SYSTEM: &str = "Eres un asistente contable chileno. Recibes el texto de una factura o boleta. Responde SOLO un JSON con las claves: \
tipo (\"factura\" o \"boleta\"), folio (texto), fecha (AAAA-MM-DD), rut_emisor (con guion, ej 76123456-7), razon_social, neto (numero), iva (numero), total (numero). \
Si un dato no aparece, usa null. No inventes nada.";

/// Convierte la respuesta de la IA en datos confiables: descarta lo que no cuadra
pub fn validate_proposal(json: &serde_json::Value) -> Proposal {
    let s = |v: &serde_json::Value, max: usize| -> String {
        v.as_str().map(str::trim).filter(|x| *x != "null").map(|x| x.chars().take(max).collect()).unwrap_or_default()
    };
    let n = |v: &serde_json::Value| v.as_f64().or_else(|| v.as_str().and_then(|t| t.replace('.', "").parse().ok())).filter(|x| x.is_finite() && (0.0..1e12).contains(x));
    let issued_on = s(&json["fecha"], 10);
    Proposal {
        kind: if s(&json["tipo"], 10).to_lowercase() == "boleta" { "boleta" } else { "factura" }.to_string(),
        folio: s(&json["folio"], 20),
        issued_on: if chrono::NaiveDate::parse_from_str(&issued_on, "%Y-%m-%d").is_ok() { issued_on } else { String::new() },
        issuer_rut: s(&json["rut_emisor"], 12),
        issuer_name: s(&json["razon_social"], 200),
        net: n(&json["neto"]),
        tax: n(&json["iva"]),
        total: n(&json["total"]),
    }
}

/// Pide a la IA (si esta configurada) que lea el texto de una factura
pub async fn propose_with_ai(client: &reqwest::Client, cfg: &crate::ai::AiConfig, text: &str) -> Result<Proposal, String> {
    let user = format!("<factura>\n{}\n</factura>", text.replace("</factura>", "").chars().take(12000).collect::<String>());
    let json = crate::ai::chat_json(client, cfg, AI_SYSTEM, &user, 500).await?;
    Ok(validate_proposal(&json))
}

/// Texto legible de un PDF sin comprimir o con streams Flate (lo basico: operadores Tj/TJ).
/// Los PDF escaneados o con fuentes raras darán poco texto: entonces se llena a mano.
pub fn pdf_text(bytes: &[u8]) -> String {
    use std::io::Read;
    let mut out = String::new();
    let mut pos = 0;
    while let Some(i) = find(bytes, b"stream", pos) {
        let mut start = i + 6;
        if bytes.get(start) == Some(&b'\r') {
            start += 1;
        }
        if bytes.get(start) == Some(&b'\n') {
            start += 1;
        }
        let Some(end) = find(bytes, b"endstream", start) else { break };
        let raw = &bytes[start..end];
        let mut data = Vec::new();
        if flate2::read::ZlibDecoder::new(raw).take(8 << 20).read_to_end(&mut data).is_err() {
            data = raw.to_vec();
        }
        out.push_str(&text_operators(&data));
        pos = end + 9;
        if out.len() > 40000 {
            break;
        }
    }
    out
}

fn find(h: &[u8], n: &[u8], from: usize) -> Option<usize> {
    h.get(from..)?.windows(n.len()).position(|w| w == n).map(|p| p + from)
}

/// Cadenas entre parentesis de los operadores de texto de un content stream
fn text_operators(data: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'(' {
            let mut depth = 1;
            let mut j = i + 1;
            let mut piece = Vec::new();
            while j < data.len() && depth > 0 {
                match data[j] {
                    b'\\' if j + 1 < data.len() => {
                        j += 1;
                        piece.push(match data[j] {
                            b'n' => b'\n',
                            b'r' => b'\r',
                            b't' => b'\t',
                            c => c,
                        });
                    }
                    b'(' => {
                        depth += 1;
                        piece.push(b'(');
                    }
                    b')' => {
                        depth -= 1;
                        if depth > 0 {
                            piece.push(b')');
                        }
                    }
                    c => piece.push(c),
                }
                j += 1;
            }
            let t: String = piece.iter().map(|&b| b as char).filter(|c| !c.is_control() || *c == '\n').collect();
            if !t.trim().is_empty() {
                out.push_str(&t);
                out.push(' ');
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="ISO-8859-1"?>
<EnvioDTE xmlns="http://www.sii.cl/SiiDte" version="1.0"><SetDTE ID="SetDoc"><Caratula><RutEmisor>76123456-7</RutEmisor></Caratula>
<DTE version="1.0"><Documento ID="F1234T33"><Encabezado>
<IdDoc><TipoDTE>33</TipoDTE><Folio>1234</Folio><FchEmis>2026-11-10</FchEmis></IdDoc>
<Emisor><RUTEmisor>76123456-7</RUTEmisor><RznSoc>Ferreter&#237;a Los &amp; Andes SpA</RznSoc><GiroEmis>Venta</GiroEmis></Emisor>
<Receptor><RUTRecep>71234500-1</RUTRecep><RznSocRecep>Universidad</RznSocRecep></Receptor>
<Totales><MntNeto>84034</MntNeto><TasaIVA>19</TasaIVA><IVA>15966</IVA><MntTotal>100000</MntTotal></Totales>
</Encabezado><Detalle><NmbItem>Tornillos</NmbItem></Detalle><TED><DD><F>99</F><TD>33</TD></DD></TED></Documento></DTE></SetDTE></EnvioDTE>"#;

    #[test]
    fn lee_una_factura() {
        let d = parse_dte(SAMPLE.as_bytes()).unwrap();
        assert_eq!(d.tipo, 33);
        assert_eq!(d.folio, "1234");
        assert_eq!(d.fecha, "2026-11-10");
        assert_eq!(d.rut_emisor, "76123456-7");
        assert_eq!(d.razon_social, "Ferretería Los & Andes SpA");
        assert_eq!((d.neto, d.iva, d.total), (Some(84034.0), Some(15966.0), 100000.0));
        assert_eq!(d.kind(), "factura");
    }

    #[test]
    fn lee_latin1_y_exenta() {
        let mut b = SAMPLE.replace("33</TipoDTE>", "34</TipoDTE>").replace("&#237;", "\u{ed}").into_bytes();
        // ISO-8859-1 real: el byte suelto 0xED no es UTF-8
        let s = String::from_utf8(b.clone()).unwrap().replace('\u{ed}', "\u{1}");
        b = s.into_bytes().into_iter().map(|c| if c == 1 { 0xED } else { c }).collect();
        let d = parse_dte(&b).unwrap();
        assert_eq!(d.tipo, 34);
        assert!(d.razon_social.starts_with("Ferreter"));
        assert!(d.razon_social.contains('í'));
    }

    #[test]
    fn rechaza_lo_que_no_sirve() {
        assert!(parse_dte(b"<?xml version=\"1.0\"?><hola/>").is_err());
        assert!(parse_dte(SAMPLE.replace("<TipoDTE>33", "<TipoDTE>61").as_bytes()).unwrap_err().contains("tipo 61"));
        assert!(parse_dte(SAMPLE.replace("<MntTotal>100000</MntTotal>", "").as_bytes()).is_err());
        assert!(parse_dte(SAMPLE.replace("2026-11-10", "10/11/2026").as_bytes()).is_err());
        assert!(parse_dte(format!("<!DOCTYPE x [<!ENTITY a \"b\">]>{}", SAMPLE).as_bytes()).is_err());
    }

    #[test]
    fn la_propuesta_de_la_ia_se_depura() {
        let p = validate_proposal(&serde_json::json!({
            "tipo": "Boleta", "folio": "77", "fecha": "ayer", "rut_emisor": "1-9", "razon_social": "X", "neto": "1.000", "iva": -5, "total": 1190
        }));
        assert_eq!(p.kind, "boleta");
        assert_eq!(p.issued_on, "");
        assert_eq!((p.net, p.tax, p.total), (Some(1000.0), None, Some(1190.0)));
    }

    #[test]
    fn saca_texto_de_un_pdf_simple() {
        let pdf = b"%PDF-1.4\n1 0 obj << /Length 40 >>\nstream\nBT (Factura 1234) Tj (Total 5000) Tj ET\nendstream\nendobj";
        let t = pdf_text(pdf);
        assert!(t.contains("Factura 1234") && t.contains("Total 5000"));
    }
}
