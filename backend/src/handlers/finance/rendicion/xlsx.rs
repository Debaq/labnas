//! Libro de Excel (.xlsx) minimo: un zip con unos pocos XML, sin dependencias nuevas.
//! La primera fila de cada hoja va en negrita (encabezados). Los textos van "en linea".

use std::io::Write;

pub enum Cell {
    T(String),
    N(f64),
}

pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}

pub fn t(s: impl Into<String>) -> Cell {
    Cell::T(s.into())
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && !matches!(c, '\n' | '\t') => {}
            c => o.push(c),
        }
    }
    o
}

fn col_letter(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

fn sheet_name(name: &str, i: usize) -> String {
    let n: String = name.chars().filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\')).take(31).collect();
    if n.trim().is_empty() {
        format!("Hoja{}", i + 1)
    } else {
        n
    }
}

fn sheet_xml(s: &Sheet) -> String {
    let ncols = s.rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let mut x = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    );
    x.push_str(&format!("<cols><col min=\"1\" max=\"{}\" width=\"22\" customWidth=\"1\"/></cols><sheetData>", ncols));
    for (ri, row) in s.rows.iter().enumerate() {
        x.push_str(&format!("<row r=\"{}\">", ri + 1));
        for (ci, c) in row.iter().enumerate() {
            let r = format!("{}{}", col_letter(ci), ri + 1);
            let style = if ri == 0 { " s=\"1\"" } else { "" };
            match c {
                Cell::T(v) => x.push_str(&format!("<c r=\"{}\"{} t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", r, style, esc(v))),
                Cell::N(v) if v.is_finite() => x.push_str(&format!("<c r=\"{}\"{}><v>{}</v></c>", r, style, v)),
                Cell::N(_) => {}
            }
        }
        x.push_str("</row>");
    }
    x.push_str("</sheetData></worksheet>");
    x
}

pub fn build(sheets: &[Sheet]) -> Result<Vec<u8>, String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut put = |name: &str, data: &str| -> Result<(), String> {
            z.start_file(name, opts).map_err(|e| e.to_string())?;
            z.write_all(data.as_bytes()).map_err(|e| e.to_string())
        };
        let mut ct = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
        );
        let mut wb = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>",
        );
        let mut rels = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        for (i, s) in sheets.iter().enumerate() {
            let n = i + 1;
            ct.push_str(&format!("<Override PartName=\"/xl/worksheets/sheet{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>", n));
            wb.push_str(&format!("<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"/>", esc(&sheet_name(&s.name, i)), n, n));
            rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{}.xml\"/>", n, n));
        }
        ct.push_str("</Types>");
        wb.push_str("</sheets></workbook>");
        rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>", sheets.len() + 1));
        put("[Content_Types].xml", &ct)?;
        put(
            "_rels/.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>",
        )?;
        put("xl/workbook.xml", &wb)?;
        put("xl/_rels/workbook.xml.rels", &rels)?;
        put(
            "xl/styles.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs></styleSheet>",
        )?;
        for (i, s) in sheets.iter().enumerate() {
            put(&format!("xl/worksheets/sheet{}.xml", i + 1), &sheet_xml(s))?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

/// CSV para Excel en español: BOM UTF-8, separador ';' y coma decimal. Los textos que
/// empiezan como una formula se anteponen con una comilla (inyeccion de formulas).
pub fn csv(rows: &[Vec<Cell>]) -> String {
    let mut o = String::from("\u{feff}");
    for r in rows {
        let line: Vec<String> = r
            .iter()
            .map(|c| match c {
                Cell::N(v) => format!("{}", v).replace('.', ","),
                Cell::T(v) => {
                    let mut v = v.replace('\r', " ");
                    if v.starts_with(['=', '+', '-', '@', '\t']) {
                        v.insert(0, '\'');
                    }
                    if v.contains([';', '"', '\n']) {
                        format!("\"{}\"", v.replace('"', "\"\""))
                    } else {
                        v
                    }
                }
            })
            .collect();
        o.push_str(&line.join(";"));
        o.push_str("\r\n");
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letras_de_columna() {
        assert_eq!((col_letter(0), col_letter(25), col_letter(26), col_letter(27)), ("A".into(), "Z".into(), "AA".into(), "AB".into()));
    }

    #[test]
    fn xlsx_es_un_zip_con_las_hojas() {
        let bytes = build(&[Sheet { name: "Gastos: <1>".into(), rows: vec![vec![t("Folio"), t("Monto")], vec![t("A&B"), Cell::N(1500.0)]] }]).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut z.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut s).unwrap();
        assert!(s.contains("A&amp;B") && s.contains("<v>1500</v>"));
        assert!(z.by_name("xl/workbook.xml").is_ok());
    }

    #[test]
    fn csv_protege_formulas_y_separadores() {
        let c = csv(&[vec![t("=1+1"), t("a;b"), Cell::N(1.5)]]);
        assert_eq!(c, "\u{feff}'=1+1;\"a;b\";1,5\r\n");
    }
}
