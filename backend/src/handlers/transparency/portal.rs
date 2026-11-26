//! Contenido del portal publico: se arma siempre desde los datos reales (Portafolio,
//! Finanzas, Inventario, uso de maquinas e informes publicados). Lo usan igual la pagina
//! publica y la vista previa del administrador, asi lo que se ve antes de guardar es lo que
//! saldra afuera. Nada de datos personales: ni numeros de serie, ni responsables, ni nombres
//! de quien uso una maquina.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde_json::{json, Value};

/// Secciones que se pueden publicar
pub const SECTIONS: &[&str] = &["proyectos", "montos", "bienes", "uso", "informes"];

pub type Sections = BTreeMap<String, bool>;

/// Secciones con todas las claves conocidas (las que faltan quedan apagadas)
pub fn normalize(raw: &Sections) -> Sections {
    SECTIONS.iter().map(|s| (s.to_string(), raw.get(*s).copied().unwrap_or(false))).collect()
}

pub fn load_sections(conn: &Connection) -> Sections {
    let raw: String = conn.query_row("SELECT sections FROM transp_config WHERE id = 1", [], |r| r.get(0)).unwrap_or_default();
    normalize(&serde_json::from_str::<Sections>(&raw).unwrap_or_default())
}

pub struct Config {
    pub sections: Sections,
    pub deadline_days: i64,
    pub intro: String,
}

pub fn load_config(conn: &Connection) -> Config {
    let (deadline_days, intro): (i64, String) = conn
        .query_row("SELECT deadline_days, intro FROM transp_config WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or((20, String::new()));
    Config { sections: load_sections(conn), deadline_days, intro }
}

fn lab(conn: &Connection) -> Value {
    conn.query_row("SELECT lab_name, institution, logo_url FROM branding WHERE id = 1", [], |r| {
        Ok(json!({ "lab_name": r.get::<_, String>(0)?, "institution": r.get::<_, String>(1)?, "logo_url": r.get::<_, String>(2)? }))
    })
    .unwrap_or_else(|_| json!({ "lab_name": "LabNAS", "institution": "", "logo_url": "" }))
}

/// Arma el portal con esas secciones (solo las activas aparecen)
pub fn build(conn: &Connection, sections: &Sections, intro: &str, deadline_days: i64) -> rusqlite::Result<Value> {
    let on = |s: &str| sections.get(s).copied().unwrap_or(false);
    let mut out = json!({
        "lab": lab(conn),
        "intro": intro,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "sections": SECTIONS.iter().filter(|s| on(s)).collect::<Vec<_>>(),
        "deadline_days": deadline_days,
    });
    if on("proyectos") {
        out["proyectos"] = projects(conn)?;
    }
    if on("montos") {
        out["montos"] = amounts(conn)?;
    }
    if on("bienes") {
        out["bienes"] = assets(conn)?;
    }
    if on("uso") {
        out["uso"] = usage(conn)?;
    }
    if on("informes") {
        out["informes"] = reports(conn)?;
    }
    Ok(out)
}

/// Solo los proyectos que llevan presupuesto en Finanzas: publicar es una decision, no un
/// efecto de haber anotado algo en el Portafolio
fn projects(conn: &Connection) -> rusqlite::Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.title, p.institution, p.funding_source, p.start_date, p.end_date, p.status
           FROM portfolio_entries p JOIN fin_projects f ON f.project_id = p.id
          ORDER BY p.start_date DESC, p.title",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "institution": r.get::<_, String>(2)?,
            "funding_source": r.get::<_, String>(3)?, "start_date": r.get::<_, String>(4)?,
            "end_date": r.get::<_, Option<String>>(5)?, "status": r.get::<_, String>(6)?,
        }))
    })?;
    Ok(Value::Array(rows.collect::<rusqlite::Result<_>>()?))
}

fn amounts(conn: &Connection) -> rusqlite::Result<Value> {
    let mut projects = conn.prepare(
        "SELECT p.id, p.title, f.currency FROM portfolio_entries p JOIN fin_projects f ON f.project_id = p.id ORDER BY p.start_date DESC, p.title",
    )?;
    let list: Vec<(String, String, String)> =
        projects.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for (id, title, currency) in list {
        // Aprobado, comprometido y gastado por categoria (lo sin item queda aparte)
        let mut cats: BTreeMap<String, [f64; 3]> = BTreeMap::new();
        let mut stmt = conn.prepare("SELECT category, SUM(approved) FROM fin_budget_items WHERE project_id = ?1 GROUP BY category")?;
        for row in stmt.query_map([&id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))? {
            let (c, a) = row?;
            cats.entry(c).or_default()[0] += a;
        }
        let mut stmt = conn.prepare(
            "SELECT COALESCE(i.category, 'sin_asignar'), SUM(c.committed), SUM(c.spent)
               FROM fin_commitments c LEFT JOIN fin_budget_items i ON i.id = c.budget_item_id
              WHERE c.project_id = ?1 GROUP BY 1",
        )?;
        for row in stmt.query_map([&id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, f64>(2)?)))? {
            let (c, committed, spent) = row?;
            let e = cats.entry(c).or_default();
            e[1] += committed;
            e[2] += spent;
        }
        let total = cats.values().fold([0.0; 3], |t, v| [t[0] + v[0], t[1] + v[1], t[2] + v[2]]);
        out.push(json!({
            "project_id": id, "title": title, "currency": currency,
            "approved": total[0], "committed": total[1], "spent": total[2],
            "categories": cats.iter().map(|(c, v)| json!({ "category": c, "approved": v[0], "committed": v[1], "spent": v[2] })).collect::<Vec<_>>(),
        }));
    }
    Ok(Value::Array(out))
}

/// Bienes comprados con un proyecto: nombre, numero de inventario y costo. Sin numero de
/// serie ni responsable.
fn assets(conn: &Connection) -> rusqlite::Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT i.name, i.inventory_number, i.cost, i.quantity, p.title
           FROM inventory_items i
           JOIN fin_projects f ON f.project_id = i.project_id
           JOIN portfolio_entries p ON p.id = i.project_id
          ORDER BY p.title, i.name LIMIT 5000",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(json!({
            "name": r.get::<_, String>(0)?, "inventory_number": r.get::<_, String>(1)?, "cost": r.get::<_, f64>(2)?,
            "quantity": r.get::<_, f64>(3)?, "project": r.get::<_, String>(4)?,
        }))
    })?;
    Ok(Value::Array(rows.collect::<rusqlite::Result<_>>()?))
}

/// Horas de uso por maquina y mes (sesiones ya cerradas), sin nombres de personas
fn usage(conn: &Connection) -> rusqlite::Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT m.name, substr(u.started_at, 1, 7) AS month,
                SUM(MAX(0.0, (julianday(u.ended_at) - julianday(u.started_at)) * 24.0)), COUNT(*)
           FROM machine_usage u JOIN machines m ON m.id = u.machine_id
          WHERE u.ended_at IS NOT NULL
          GROUP BY m.id, month ORDER BY month DESC, m.name",
    )?;
    let rows = stmt.query_map([], |r| {
        let hours: f64 = r.get(2)?;
        Ok(json!({ "machine": r.get::<_, String>(0)?, "month": r.get::<_, String>(1)?, "hours": (hours * 10.0).round() / 10.0, "sessions": r.get::<_, i64>(3)? }))
    })?;
    Ok(Value::Array(rows.collect::<rusqlite::Result<_>>()?))
}

fn reports(conn: &Connection) -> rusqlite::Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.title, r.report_date, p.title, f.name, f.size, f.sha256
           FROM transp_reports r JOIN fin_files f ON f.id = r.file_id LEFT JOIN portfolio_entries p ON p.id = r.project_id
          WHERE r.published = 1 ORDER BY r.report_date DESC, r.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "date": r.get::<_, String>(2)?,
            "project": r.get::<_, Option<String>>(3)?, "file_name": r.get::<_, String>(4)?,
            "size": r.get::<_, i64>(5)?, "sha256": r.get::<_, String>(6)?,
        }))
    })?;
    Ok(Value::Array(rows.collect::<rusqlite::Result<_>>()?))
}
