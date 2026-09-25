//! Estadisticas de paginas y costos por impresora y usuario

use super::*;

/// Cuenta páginas reales de un archivo usando pdfinfo (para PDFs)
pub(super) async fn count_file_pages(file_path: &str) -> u64 {
    if file_path.to_lowercase().ends_with(".pdf") {
        if let Ok(output) = Command::new("pdfinfo")
            .arg(file_path)
            .output()
            .await
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.starts_with("Pages:") {
                    if let Some(n) = line.split(':').nth(1) {
                        if let Ok(pages) = n.trim().parse::<u64>() {
                            return pages;
                        }
                    }
                }
            }
        }
    }
    1 // No-PDF o error: asumir 1 página
}

/// Calcula páginas efectivas considerando rango, total real y copias
pub(super) fn calculate_printed_pages(pages_range: &Option<String>, total_pages: u64, copies: u32) -> u64 {
    let page_count = match pages_range {
        Some(pg) if !pg.is_empty() => {
            let mut count: u64 = 0;
            for part in pg.split(',') {
                let part = part.trim();
                if let Some((start, end)) = part.split_once('-') {
                    if let (Ok(s), Ok(e)) = (start.trim().parse::<u64>(), end.trim().parse::<u64>()) {
                        if e >= s {
                            count += e - s + 1;
                        }
                    }
                } else if part.parse::<u64>().is_ok() {
                    count += 1;
                }
            }
            if count == 0 { total_pages } else { count }
        }
        _ => total_pages, // Sin rango = todas las páginas del documento
    };
    page_count * copies as u64
}

/// Clasifica tipo de papel desde la opción media de CUPS
pub(super) fn classify_paper(options: &HashMap<String, String>) -> &'static str {
    let media = options
        .get("media")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if media.contains("legal") || media.contains("oficio") || media.contains("folio") {
        "oficio"
    } else if media.contains("photo")
        || media.contains("glossy")
        || media.contains("transparency")
        || media.contains("envelope")
        || media.contains("label")
        || media.contains("a3")
    {
        "special"
    } else {
        "carta" // Letter, A4, o sin especificar
    }
}

/// Registra estadísticas de impresión en la DB (global + por usuario)
pub(super) async fn track_print_stats(
    state: &AppState,
    printer_name: &str,
    file_path: &str,
    copies: &Option<String>,
    pages: &Option<String>,
    options: &HashMap<String, String>,
    username: &str,
) {
    let num_copies = copies
        .as_ref()
        .and_then(|c| c.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1);
    let doc_pages = count_file_pages(file_path).await;
    let total_pages = calculate_printed_pages(pages, doc_pages, num_copies);
    let paper_type = classify_paper(options).to_string();
    let pname = printer_name.to_string();
    let uname = username.to_string();

    let _ = db_op(&state.db, move |conn| {
        // Ensure printer row exists
        conn.execute(
            "INSERT OR IGNORE INTO cups_printers (name) VALUES (?1)",
            params![&pname],
        ).map_err(|e| format!("DB: {}", e))?;

        // Update global stats
        let (jobs_col, pages_col) = ("total_jobs", "total_pages");
        let paper_col = match paper_type.as_str() {
            "oficio" => "pages_oficio",
            "special" => "pages_special",
            _ => "pages_carta",
        };
        conn.execute(
            &format!(
                "UPDATE cups_printers SET {}={} + 1, {}={} + ?1, {}={} + ?1 WHERE name = ?2",
                jobs_col, jobs_col, pages_col, pages_col, paper_col, paper_col
            ),
            params![total_pages as i64, &pname],
        ).map_err(|e| format!("DB: {}", e))?;

        // User stats: ensure row exists
        conn.execute(
            "INSERT OR IGNORE INTO cups_printer_user_stats (printer_name, username) VALUES (?1, ?2)",
            params![&pname, &uname],
        ).map_err(|e| format!("DB: {}", e))?;

        conn.execute(
            &format!(
                "UPDATE cups_printer_user_stats SET {}={} + 1, {}={} + ?1, {}={} + ?1 WHERE printer_name = ?2 AND username = ?3",
                jobs_col, jobs_col, pages_col, pages_col, paper_col, paper_col
            ),
            params![total_pages as i64, &pname, &uname],
        ).map_err(|e| format!("DB: {}", e))?;

        Ok(())
    }).await;
}

pub(super) fn calculate_estimated_cost(
    costs: &crate::models::printing::PrinterCosts,
    stats: &crate::models::printing::PrinterStats,
) -> f64 {
    let ink = stats.total_pages as f64 * costs.ink_per_page;
    let paper_carta = stats.pages_carta as f64 * costs.paper_carta;
    let paper_oficio = stats.pages_oficio as f64 * costs.paper_oficio;
    let paper_special = stats.pages_special as f64 * costs.paper_special;
    ink + paper_carta + paper_oficio + paper_special
}

/// ink_per_page, paper_carta, paper_oficio, paper_special, total_jobs, total_pages, pages_carta, pages_oficio, pages_special
pub(super) type PrinterStatsRow = (f64, f64, f64, f64, i64, i64, i64, i64, i64);

/// GET /api/printing/printers/{name}/stats
pub async fn get_printer_stats(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<PrinterStatsResponse>, (StatusCode, String)> {
    let resp = db_op(&state.db, move |conn| {
        use rusqlite::OptionalExtension;
        let row: Option<PrinterStatsRow> = conn.query_row(
            "SELECT ink_per_page, paper_carta, paper_oficio, paper_special, total_jobs, total_pages, pages_carta, pages_oficio, pages_special FROM cups_printers WHERE name = ?1",
            params![name],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?)),
        ).optional().map_err(|e| format!("DB: {}", e))?;

        match row {
            Some((ink, carta, oficio, special, jobs, pages, pc, po, ps)) => {
                let costs = PrinterCosts { ink_per_page: ink, paper_carta: carta, paper_oficio: oficio, paper_special: special };
                let stats = PrinterStats { total_jobs: jobs as u64, total_pages: pages as u64, pages_carta: pc as u64, pages_oficio: po as u64, pages_special: ps as u64 };
                let estimated_cost = calculate_estimated_cost(&costs, &stats);
                Ok(PrinterStatsResponse { costs, stats, estimated_cost })
            }
            None => Ok(PrinterStatsResponse {
                costs: PrinterCosts::default(),
                stats: PrinterStats::default(),
                estimated_cost: 0.0,
            }),
        }
    }).await?;
    Ok(Json(resp))
}

/// POST /api/printing/printers/{name}/costs
pub async fn set_printer_costs(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(costs): Json<PrinterCosts>,
) -> Result<StatusCode, (StatusCode, String)> {
    validate_printer_name(&name)?;
    db_op(&state.db, move |conn| {
        // Ensure printer row exists
        conn.execute(
            "INSERT OR IGNORE INTO cups_printers (name) VALUES (?1)",
            params![&name],
        ).map_err(|e| format!("DB: {}", e))?;
        conn.execute(
            "UPDATE cups_printers SET ink_per_page=?1, paper_carta=?2, paper_oficio=?3, paper_special=?4 WHERE name=?5",
            params![costs.ink_per_page, costs.paper_carta, costs.paper_oficio, costs.paper_special, &name],
        ).map_err(|e| format!("DB: {}", e))?;
        Ok(())
    }).await?;
    Ok(StatusCode::OK)
}

/// POST /api/printing/printers/{name}/stats/reset
pub async fn reset_printer_stats(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    validate_printer_name(&name)?;
    db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE cups_printers SET total_jobs=0, total_pages=0, pages_carta=0, pages_oficio=0, pages_special=0 WHERE name=?1",
            params![&name],
        ).map_err(|e| format!("DB: {}", e))?;
        conn.execute(
            "DELETE FROM cups_printer_user_stats WHERE printer_name=?1",
            params![&name],
        ).map_err(|e| format!("DB: {}", e))?;
        Ok(())
    }).await?;
    Ok(StatusCode::OK)
}

/// Construye costos por usuario desde la DB
pub(super) fn build_user_costs(
    conn: &rusqlite::Connection,
    filter_user: Option<&str>,
) -> Result<AllUserCostsResponse, String> {
    // Read all printers with costs and global stats
    let mut stmt = conn.prepare(
        "SELECT name, ink_per_page, paper_carta, paper_oficio, paper_special, total_jobs, total_pages, pages_carta, pages_oficio, pages_special FROM cups_printers"
    ).map_err(|e| format!("DB: {}", e))?;

    struct PrinterRow {
        name: String,
        costs: PrinterCosts,
        stats: PrinterStats,
    }

    let printers: Vec<PrinterRow> = stmt.query_map([], |row| {
        Ok(PrinterRow {
            name: row.get(0)?,
            costs: PrinterCosts {
                ink_per_page: row.get(1)?,
                paper_carta: row.get(2)?,
                paper_oficio: row.get(3)?,
                paper_special: row.get(4)?,
            },
            stats: PrinterStats {
                total_jobs: row.get::<_, i64>(5)? as u64,
                total_pages: row.get::<_, i64>(6)? as u64,
                pages_carta: row.get::<_, i64>(7)? as u64,
                pages_oficio: row.get::<_, i64>(8)? as u64,
                pages_special: row.get::<_, i64>(9)? as u64,
            },
        })
    }).map_err(|e| format!("DB: {}", e))?
      .filter_map(|r| r.ok())
      .collect();

    let mut general_cost = 0.0;
    let mut general_jobs = 0u64;
    let mut general_pages = 0u64;

    // Build a costs map for lookup
    let mut costs_map: std::collections::HashMap<String, PrinterCosts> = std::collections::HashMap::new();
    for p in &printers {
        let pcost = calculate_estimated_cost(&p.costs, &p.stats);
        general_cost += pcost;
        general_jobs += p.stats.total_jobs;
        general_pages += p.stats.total_pages;
        costs_map.insert(p.name.clone(), p.costs.clone());
    }

    // Read user stats
    let user_stmt = if let Some(filter) = filter_user {
        let mut s = conn.prepare(
            "SELECT printer_name, username, total_jobs, total_pages, pages_carta, pages_oficio, pages_special FROM cups_printer_user_stats WHERE username = ?1"
        ).map_err(|e| format!("DB: {}", e))?;
        let rows: Vec<(String, String, PrinterStats)> = s.query_map(params![filter], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                PrinterStats {
                    total_jobs: row.get::<_, i64>(2)? as u64,
                    total_pages: row.get::<_, i64>(3)? as u64,
                    pages_carta: row.get::<_, i64>(4)? as u64,
                    pages_oficio: row.get::<_, i64>(5)? as u64,
                    pages_special: row.get::<_, i64>(6)? as u64,
                },
            ))
        }).map_err(|e| format!("DB: {}", e))?
          .filter_map(|r| r.ok())
          .collect();
        rows
    } else {
        let mut s = conn.prepare(
            "SELECT printer_name, username, total_jobs, total_pages, pages_carta, pages_oficio, pages_special FROM cups_printer_user_stats"
        ).map_err(|e| format!("DB: {}", e))?;
        let rows: Vec<(String, String, PrinterStats)> = s.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                PrinterStats {
                    total_jobs: row.get::<_, i64>(2)? as u64,
                    total_pages: row.get::<_, i64>(3)? as u64,
                    pages_carta: row.get::<_, i64>(4)? as u64,
                    pages_oficio: row.get::<_, i64>(5)? as u64,
                    pages_special: row.get::<_, i64>(6)? as u64,
                },
            ))
        }).map_err(|e| format!("DB: {}", e))?
          .filter_map(|r| r.ok())
          .collect();
        rows
    };

    let mut user_map: std::collections::BTreeMap<String, Vec<UserPrinterStats>> =
        std::collections::BTreeMap::new();

    for (printer_name, username, stats) in &user_stmt {
        let costs = costs_map.get(printer_name).cloned().unwrap_or_default();
        let est = calculate_estimated_cost(&costs, stats);
        user_map
            .entry(username.clone())
            .or_default()
            .push(UserPrinterStats {
                printer: printer_name.clone(),
                stats: stats.clone(),
                estimated_cost: est,
            });
    }

    let users = user_map
        .into_iter()
        .map(|(username, printers)| {
            let total_cost: f64 = printers.iter().map(|p| p.estimated_cost).sum();
            let total_jobs: u64 = printers.iter().map(|p| p.stats.total_jobs).sum();
            let total_pages: u64 = printers.iter().map(|p| p.stats.total_pages).sum();
            UserCostsResponse {
                username,
                total_cost,
                total_jobs,
                total_pages,
                printers,
            }
        })
        .collect();

    Ok(AllUserCostsResponse {
        users,
        general_cost,
        general_jobs,
        general_pages,
    })
}

/// GET /api/printing/user-costs — Admin: costos de todos los usuarios
pub async fn get_all_user_costs(
    State(state): State<AppState>,
) -> Result<Json<AllUserCostsResponse>, (StatusCode, String)> {
    let resp = db_op(&state.db, |conn| {
        build_user_costs(conn, None)
    }).await?;
    Ok(Json(resp))
}

/// GET /api/printing/my-costs — Costos del usuario actual + totales generales
pub async fn get_my_costs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AllUserCostsResponse>, (StatusCode, String)> {
    let (username, _) = extract_session(&state, &headers)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "No autorizado".to_string()))?;
    let resp = db_op(&state.db, move |conn| {
        build_user_costs(conn, Some(&username))
    }).await?;
    Ok(Json(resp))
}

// ── Impresion duplex manual (asistente paso a paso) ──
