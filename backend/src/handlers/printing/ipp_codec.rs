//! Codificacion binaria de IPP (RFC 8010): lo justo para que LabNAS actue como
//! impresora de red frente a Windows, macOS o Linux.
//!
//! Un mensaje es: version (2 bytes), operacion o estado (2), id de peticion (4),
//! grupos de atributos y la etiqueta de fin; despues viene el documento.

/// Etiquetas de grupo y de valor
pub mod tag {
    pub const OPERATION: u8 = 0x01;
    pub const JOB: u8 = 0x02;
    pub const END: u8 = 0x03;
    pub const PRINTER: u8 = 0x04;

    pub const NO_VALUE: u8 = 0x13;
    pub const INTEGER: u8 = 0x21;
    pub const BOOLEAN: u8 = 0x22;
    pub const ENUM: u8 = 0x23;
    pub const RESOLUTION: u8 = 0x32;
    pub const RANGE: u8 = 0x33;
    pub const BEG_COLLECTION: u8 = 0x34;
    pub const TEXT_LANG: u8 = 0x35;
    pub const NAME_LANG: u8 = 0x36;
    pub const END_COLLECTION: u8 = 0x37;
    pub const TEXT: u8 = 0x41;
    pub const NAME: u8 = 0x42;
    pub const KEYWORD: u8 = 0x44;
    pub const URI: u8 = 0x45;
    pub const CHARSET: u8 = 0x47;
    pub const LANGUAGE: u8 = 0x48;
    pub const MIME: u8 = 0x49;
    pub const MEMBER_NAME: u8 = 0x4A;
}

/// Operaciones que atiende LabNAS
pub mod op {
    pub const PRINT_JOB: u16 = 0x0002;
    pub const VALIDATE_JOB: u16 = 0x0004;
    pub const CREATE_JOB: u16 = 0x0005;
    pub const SEND_DOCUMENT: u16 = 0x0006;
    pub const CANCEL_JOB: u16 = 0x0008;
    pub const GET_JOB_ATTRIBUTES: u16 = 0x0009;
    pub const GET_JOBS: u16 = 0x000A;
    pub const GET_PRINTER_ATTRIBUTES: u16 = 0x000B;
}

/// Codigos de estado
pub mod status {
    pub const OK: u16 = 0x0000;
    pub const BAD_REQUEST: u16 = 0x0400;
    pub const FORBIDDEN: u16 = 0x0401;
    pub const NOT_POSSIBLE: u16 = 0x0404;
    pub const NOT_FOUND: u16 = 0x0406;
    pub const TOO_LARGE: u16 = 0x0409;
    pub const FORMAT_NOT_SUPPORTED: u16 = 0x040A;
    pub const INTERNAL_ERROR: u16 = 0x0500;
    pub const OPERATION_NOT_SUPPORTED: u16 = 0x0501;
    pub const VERSION_NOT_SUPPORTED: u16 = 0x0503;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    Bool(bool),
    Enum(i32),
    /// Cadena con su etiqueta (texto, nombre, palabra clave, URI, tipo MIME...)
    Str(u8, String),
    /// Resolucion: ancho, alto y unidad (3 = puntos por pulgada)
    Resolution(i32, i32, u8),
    Range(i32, i32),
    Collection(Vec<(String, Vec<Value>)>),
    /// Valores fuera de banda (sin valor, desconocido) u otros tipos que no usamos
    Other(u8, Vec<u8>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(_, s) => Some(s),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i32> {
        match self {
            Value::Int(n) | Value::Enum(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

pub fn kw(s: &str) -> Value {
    Value::Str(tag::KEYWORD, s.to_string())
}
pub fn text(s: &str) -> Value {
    Value::Str(tag::TEXT, s.to_string())
}
pub fn name(s: &str) -> Value {
    Value::Str(tag::NAME, s.to_string())
}
pub fn uri(s: &str) -> Value {
    Value::Str(tag::URI, s.to_string())
}
pub fn mime(s: &str) -> Value {
    Value::Str(tag::MIME, s.to_string())
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    pub values: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub tag: u8,
    pub attrs: Vec<Attr>,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub version: (u8, u8),
    pub op: u16,
    pub request_id: u32,
    pub groups: Vec<Group>,
}

impl Request {
    fn find(&self, group: u8, attr: &str) -> Option<&Attr> {
        self.groups
            .iter()
            .filter(|g| g.tag == group)
            .flat_map(|g| g.attrs.iter())
            .find(|a| a.name == attr)
    }
    pub fn op_attr(&self, attr: &str) -> Option<&Attr> {
        self.find(tag::OPERATION, attr)
    }
    pub fn job_attr(&self, attr: &str) -> Option<&Attr> {
        self.find(tag::JOB, attr)
    }
    pub fn op_str(&self, attr: &str) -> Option<&str> {
        self.op_attr(attr)?.values.first()?.as_str()
    }
    pub fn op_int(&self, attr: &str) -> Option<i32> {
        self.op_attr(attr)?.values.first()?.as_int()
    }
    pub fn job_value(&self, attr: &str) -> Option<&Value> {
        self.job_attr(attr)?.values.first()
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    /// Faltan bytes: seguir leyendo el cuerpo
    Incomplete,
    Bad(String),
}

/// Limite de anidacion de colecciones (evita recursion sin fin con datos hostiles)
const MAX_DEPTH: usize = 8;

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        let end = self.pos.checked_add(n).ok_or(ParseError::Incomplete)?;
        if end > self.buf.len() {
            return Err(ParseError::Incomplete);
        }
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, ParseError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, ParseError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn bytes16(&mut self) -> Result<&'a [u8], ParseError> {
        let n = self.u16()? as usize;
        self.take(n)
    }
}

fn i32_be(b: &[u8]) -> i32 {
    i32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn bad(msg: &str) -> ParseError {
    ParseError::Bad(msg.to_string())
}

/// Lee el valor de una etiqueta (ya leidos la etiqueta y el nombre)
fn read_value(c: &mut Cursor, vtag: u8, depth: usize) -> Result<Value, ParseError> {
    let data = c.bytes16()?;
    let need = |n: usize| if data.len() == n { Ok(()) } else { Err(bad("largo de valor invalido")) };
    Ok(match vtag {
        tag::INTEGER => {
            need(4)?;
            Value::Int(i32_be(data))
        }
        tag::ENUM => {
            need(4)?;
            Value::Enum(i32_be(data))
        }
        tag::BOOLEAN => {
            need(1)?;
            Value::Bool(data[0] != 0)
        }
        tag::RESOLUTION => {
            need(9)?;
            Value::Resolution(i32_be(&data[0..4]), i32_be(&data[4..8]), data[8])
        }
        tag::RANGE => {
            need(8)?;
            Value::Range(i32_be(&data[0..4]), i32_be(&data[4..8]))
        }
        tag::TEXT_LANG | tag::NAME_LANG => {
            // idioma (largo + texto) y luego el texto (largo + texto)
            let mut inner = Cursor { buf: data, pos: 0 };
            let parsed = inner.bytes16().and_then(|_| inner.bytes16());
            let s = parsed.map_err(|_| bad("texto con idioma mal formado"))?;
            let base = if vtag == tag::TEXT_LANG { tag::TEXT } else { tag::NAME };
            Value::Str(base, String::from_utf8_lossy(s).into_owned())
        }
        tag::BEG_COLLECTION => read_collection(c, depth + 1)?,
        0x40..=0x5F => Value::Str(vtag, String::from_utf8_lossy(data).into_owned()),
        _ => Value::Other(vtag, data.to_vec()),
    })
}

fn read_collection(c: &mut Cursor, depth: usize) -> Result<Value, ParseError> {
    if depth > MAX_DEPTH {
        return Err(bad("colecciones demasiado anidadas"));
    }
    let mut members: Vec<(String, Vec<Value>)> = Vec::new();
    loop {
        let t = c.u8()?;
        let _name = c.bytes16()?;
        match t {
            tag::END_COLLECTION => {
                c.bytes16()?;
                return Ok(Value::Collection(members));
            }
            tag::MEMBER_NAME => {
                let member = String::from_utf8_lossy(c.bytes16()?).into_owned();
                let vt = c.u8()?;
                c.bytes16()?; // nombre vacio
                let v = read_value(c, vt, depth)?;
                members.push((member, vec![v]));
            }
            t if t >= 0x10 => {
                // Valor adicional del ultimo miembro
                let v = read_value(c, t, depth)?;
                members
                    .last_mut()
                    .ok_or_else(|| bad("valor de coleccion sin miembro"))?
                    .1
                    .push(v);
            }
            _ => return Err(bad("etiqueta invalida dentro de una coleccion")),
        }
    }
}

/// Interpreta la cabecera IPP. Devuelve la peticion y donde empieza el documento.
pub fn parse_request(buf: &[u8]) -> Result<(Request, usize), ParseError> {
    let mut c = Cursor { buf, pos: 0 };
    let version = (c.u8()?, c.u8()?);
    let op = c.u16()?;
    let request_id = c.u32()?;
    let mut groups: Vec<Group> = Vec::new();
    loop {
        let t = c.u8()?;
        if t == tag::END {
            break;
        }
        if t < 0x10 {
            groups.push(Group { tag: t, attrs: Vec::new() });
            continue;
        }
        let attr_name = String::from_utf8_lossy(c.bytes16()?).into_owned();
        let value = read_value(&mut c, t, 0)?;
        let group = groups.last_mut().ok_or_else(|| bad("atributo fuera de un grupo"))?;
        if attr_name.is_empty() {
            group
                .attrs
                .last_mut()
                .ok_or_else(|| bad("valor adicional sin atributo"))?
                .values
                .push(value);
        } else {
            group.attrs.push(Attr { name: attr_name, values: vec![value] });
        }
    }
    Ok((Request { version, op, request_id, groups }, c.pos))
}

/// Construye una respuesta IPP
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new(version: (u8, u8), status: u16, request_id: u32) -> Self {
        let mut buf = vec![version.0, version.1];
        buf.extend_from_slice(&status.to_be_bytes());
        buf.extend_from_slice(&request_id.to_be_bytes());
        // Grupo de operacion obligatorio: juego de caracteres e idioma
        let mut w = Writer { buf };
        w.group(tag::OPERATION);
        w.attr("attributes-charset", &[Value::Str(tag::CHARSET, "utf-8".into())]);
        w.attr("attributes-natural-language", &[Value::Str(tag::LANGUAGE, "es".into())]);
        w
    }

    pub fn group(&mut self, t: u8) {
        self.buf.push(t);
    }

    pub fn attr(&mut self, attr_name: &str, values: &[Value]) {
        for (i, v) in values.iter().enumerate() {
            self.value(if i == 0 { attr_name } else { "" }, v);
        }
    }

    fn put16(&mut self, data: &[u8]) {
        let n = data.len().min(u16::MAX as usize);
        self.buf.extend_from_slice(&(n as u16).to_be_bytes());
        self.buf.extend_from_slice(&data[..n]);
    }

    fn value(&mut self, attr_name: &str, v: &Value) {
        let (t, data): (u8, Vec<u8>) = match v {
            Value::Int(n) => (tag::INTEGER, n.to_be_bytes().to_vec()),
            Value::Enum(n) => (tag::ENUM, n.to_be_bytes().to_vec()),
            Value::Bool(b) => (tag::BOOLEAN, vec![*b as u8]),
            Value::Str(t, s) => (*t, s.as_bytes().to_vec()),
            Value::Resolution(x, y, u) => {
                let mut d = x.to_be_bytes().to_vec();
                d.extend_from_slice(&y.to_be_bytes());
                d.push(*u);
                (tag::RESOLUTION, d)
            }
            Value::Range(a, b) => {
                let mut d = a.to_be_bytes().to_vec();
                d.extend_from_slice(&b.to_be_bytes());
                (tag::RANGE, d)
            }
            Value::Other(t, d) => (*t, d.clone()),
            Value::Collection(members) => {
                self.buf.push(tag::BEG_COLLECTION);
                self.put16(attr_name.as_bytes());
                self.put16(&[]);
                for (member, values) in members {
                    self.buf.push(tag::MEMBER_NAME);
                    self.put16(&[]);
                    self.put16(member.as_bytes());
                    for mv in values {
                        self.value("", mv);
                    }
                }
                self.buf.push(tag::END_COLLECTION);
                self.put16(&[]);
                self.put16(&[]);
                return;
            }
        };
        self.buf.push(t);
        self.put16(attr_name.as_bytes());
        self.put16(&data);
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.buf.push(tag::END);
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> Vec<u8> {
        // Get-Printer-Attributes armado con el mismo Writer (simetria) mas una coleccion
        let mut w = Writer::new((2, 0), op::GET_PRINTER_ATTRIBUTES, 7);
        w.attr("printer-uri", &[uri("ipp://h:8631/ipp/t/p")]);
        w.attr("requested-attributes", &[kw("printer-name"), kw("media-col-database")]);
        w.group(tag::JOB);
        w.attr(
            "media-col",
            &[Value::Collection(vec![(
                "media-size".into(),
                vec![Value::Collection(vec![
                    ("x-dimension".into(), vec![Value::Int(21590)]),
                    ("y-dimension".into(), vec![Value::Int(27940)]),
                ])],
            )])],
        );
        w.attr("copies", &[Value::Int(2)]);
        w.finish()
    }

    #[test]
    fn ida_y_vuelta() {
        let mut data = sample_request();
        data.extend_from_slice(b"%PDF-1.7");
        let (req, off) = parse_request(&data).unwrap();
        assert_eq!(req.version, (2, 0));
        assert_eq!(req.op, op::GET_PRINTER_ATTRIBUTES);
        assert_eq!(req.request_id, 7);
        assert_eq!(&data[off..], b"%PDF-1.7");
        assert_eq!(req.op_str("printer-uri"), Some("ipp://h:8631/ipp/t/p"));
        let ra: Vec<_> = req.op_attr("requested-attributes").unwrap().values.iter().filter_map(Value::as_str).collect();
        assert_eq!(ra, vec!["printer-name", "media-col-database"]);
        assert_eq!(req.job_value("copies").and_then(Value::as_int), Some(2));
        let Some(Value::Collection(m)) = req.job_value("media-col") else { panic!("sin coleccion") };
        let Value::Collection(size) = &m[0].1[0] else { panic!("sin media-size") };
        assert_eq!(size[0], ("x-dimension".to_string(), vec![Value::Int(21590)]));
    }

    #[test]
    fn incompleto_se_detecta_en_cualquier_corte() {
        let data = sample_request();
        for cut in 0..data.len() {
            assert_eq!(parse_request(&data[..cut]).unwrap_err(), ParseError::Incomplete, "corte {}", cut);
        }
    }

    #[test]
    fn texto_con_idioma() {
        let mut d = vec![1, 1, 0, 2, 0, 0, 0, 1, tag::OPERATION, tag::NAME_LANG];
        d.extend_from_slice(&8u16.to_be_bytes());
        d.extend_from_slice(b"job-name");
        let inner = [&2u16.to_be_bytes()[..], b"es", &4u16.to_be_bytes()[..], b"hola"].concat();
        d.extend_from_slice(&(inner.len() as u16).to_be_bytes());
        d.extend_from_slice(&inner);
        d.push(tag::END);
        let (req, _) = parse_request(&d).unwrap();
        assert_eq!(req.op_str("job-name"), Some("hola"));
    }

    #[test]
    fn datos_hostiles_no_rompen() {
        assert!(matches!(parse_request(&[1, 1, 0, 2, 0, 0, 0, 1, 0x44, 0, 0, 0, 0, 3]), Err(ParseError::Bad(_))));
        // Colecciones anidadas sin fin
        let mut d = vec![2, 0, 0, 2, 0, 0, 0, 1, tag::JOB];
        for _ in 0..20 {
            d.extend_from_slice(&[tag::BEG_COLLECTION, 0, 1, b'x', 0, 0, tag::MEMBER_NAME, 0, 0, 0, 1, b'y']);
        }
        assert!(matches!(parse_request(&d), Err(ParseError::Bad(_))));
    }
}
