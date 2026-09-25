//! Comandos de proyectos y tareas

use super::*;

// =====================
// Tasks & Projects
// =====================

pub(super) async fn handle_project_command(state: &AppState, creator: &str, text: &str) -> String {
    let name = text.strip_prefix("/proyecto ").unwrap_or("").trim();
    if name.is_empty() {
        return "Uso: `/proyecto Nombre del proyecto`".to_string();
    }

    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let members_json = serde_json::to_string(&vec![creator.to_string()]).unwrap_or_else(|_| "[]".to_string());
    let created_at = chrono::Utc::now().to_rfc3339();

    let id_clone = id.clone();
    let name_str = name.to_string();
    let creator_str = creator.to_string();

    let result = crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO projects (id, name, description, created_by, members, member_tags, created_at) VALUES (?1, ?2, '', ?3, ?4, '{}', ?5)",
            params![id_clone, name_str, creator_str, members_json, created_at],
        )
        .map_err(|e| format!("DB: {}", e))?;
        Ok(())
    })
    .await;

    match result {
        Ok(()) => format!("Proyecto *{}* creado (ID: `{}`)", name, id),
        Err(e) => format!("Error: {}", e.1),
    }
}

pub(super) async fn handle_list_projects(state: &AppState, user: &str) -> String {
    let user_str = user.to_string();
    let result = crate::db::db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, created_by, members FROM projects")
            .map_err(|e| format!("DB: {}", e))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| format!("DB: {}", e))?;

        let mut projects = Vec::new();
        for r in rows {
            projects.push(r.map_err(|e| format!("DB row: {}", e))?);
        }

        if projects.is_empty() {
            return Ok("*Proyectos*\n\nNo hay proyectos. Crea uno con `/proyecto Nombre`".to_string());
        }

        let mut msg = format!("*Proyectos* ({})\n", projects.len());
        for (id, name, created_by, members_json) in &projects {
            let members: Vec<String> = serde_json::from_str(members_json).unwrap_or_default();

            // Count tasks for this project
            let task_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE project_id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let done_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE project_id = ?1 AND status = 'completada'",
                    params![id],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let is_member = members.contains(&user_str) || created_by == &user_str;
            let badge = if is_member { "" } else { " (no eres miembro)" };
            msg.push_str(&format!(
                "\n`{}` *{}*{}\n  {}/{} tareas completadas\n",
                id, name, badge, done_count, task_count
            ));
        }
        Ok(msg)
    })
    .await;

    match result {
        Ok(msg) => msg,
        Err(e) => format!("Error: {}", e.1),
    }
}

pub(super) async fn handle_task_command(state: &AppState, creator: &str, text: &str) -> String {
    let args = text.strip_prefix("/tarea ").unwrap_or("").trim();
    if args.is_empty() {
        return "Uso: `/tarea Titulo @persona !confirmar !insistente`\nEj: `/tarea Revisar servidor @all !confirmar`".to_string();
    }

    // Parse: title, @mentions, !flags
    let mut title_parts = Vec::new();
    let mut assigned = Vec::new();
    let mut requires_confirmation = false;
    let mut insistent = false;
    let mut reminder_minutes: u32 = 8;
    let mut project_id: Option<String> = None;

    for word in args.split_whitespace() {
        if let Some(mention) = word.strip_prefix('@') {
            assigned.push(mention.to_string());
        } else if word == "!confirmar" || word == "!confirmacion" {
            requires_confirmation = true;
        } else if word == "!insistente" || word == "!insistir" {
            insistent = true;
        } else if let Some(mins) = word.strip_prefix("!cada") {
            if let Ok(m) = mins.parse::<u32>() {
                if m >= 1 {
                    reminder_minutes = m;
                    insistent = true;
                }
            }
        } else if let Some(pid) = word.strip_prefix('#') {
            project_id = Some(pid.to_string());
        } else {
            title_parts.push(word);
        }
    }

    let title = title_parts.join(" ");
    if title.is_empty() {
        return "La tarea necesita un titulo.".to_string();
    }

    if assigned.is_empty() {
        assigned.push(creator.to_string());
    }

    // If insistent, it also requires confirmation
    if insistent {
        requires_confirmation = true;
    }

    let id = uuid::Uuid::new_v4().to_string()[..6].to_string();
    let assigned_json = serde_json::to_string(&assigned).unwrap_or_else(|_| "[]".to_string());
    let created_at = chrono::Utc::now().to_rfc3339();

    let id_clone = id.clone();
    let title_clone = title.clone();
    let creator_str = creator.to_string();

    let result = crate::db::db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO tasks (id, project_id, title, description, assigned_to, status, created_by, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at) VALUES (?1, ?2, ?3, '', ?4, 'pendiente', ?5, ?6, ?7, ?8, '[]', '[]', ?9)",
            params![id_clone, project_id, title_clone, assigned_json, creator_str, requires_confirmation, insistent, reminder_minutes, created_at],
        )
        .map_err(|e| format!("DB: {}", e))?;
        Ok(())
    })
    .await;

    match result {
        Ok(()) => {
            let assign_str = assigned.join(", ");
            let flags = [
                if requires_confirmation { "confirmar" } else { "" },
                if insistent { "insistente" } else { "" },
            ].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
            let flags_str = if flags.is_empty() { String::new() } else { format!(" ({})", flags) };
            format!("Tarea *{}* creada{}\nID: `{}`\nAsignada a: {}", title, flags_str, id, assign_str)
        }
        Err(e) => format!("Error: {}", e.1),
    }
}

pub(super) async fn handle_list_tasks(state: &AppState, user: &str) -> String {
    let user_str = user.to_string();
    let result = crate::db::db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare("SELECT id, title, status, assigned_to, created_by, requires_confirmation, insistent FROM tasks WHERE status NOT IN ('completada', 'rechazada')")
            .map_err(|e| format!("DB: {}", e))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, bool>(6)?,
                ))
            })
            .map_err(|e| format!("DB: {}", e))?;

        let mut my_tasks = Vec::new();
        for r in rows {
            let (id, title, status, assigned_json, created_by, req_conf, insist) = r.map_err(|e| format!("DB row: {}", e))?;
            let assigned: Vec<String> = serde_json::from_str(&assigned_json).unwrap_or_default();
            let is_mine = assigned.contains(&"all".to_string())
                || assigned.iter().any(|a| a.to_lowercase() == user_str.to_lowercase())
                || created_by == user_str;
            if is_mine {
                my_tasks.push((id, title, status, created_by, req_conf, insist));
            }
        }

        if my_tasks.is_empty() {
            return Ok("*Mis tareas*\n\nNo tienes tareas pendientes.".to_string());
        }

        let mut msg = format!("*Mis tareas* ({})\n", my_tasks.len());
        for (id, title, status, created_by, req_conf, insist) in &my_tasks {
            let status_str = match status.as_str() {
                "pendiente" => "pendiente",
                "en_progreso" => "en progreso",
                _ => "?",
            };
            let flags = [
                if *req_conf { "confirmar" } else { "" },
                if *insist { "insistente" } else { "" },
            ].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
            let flags_str = if flags.is_empty() { String::new() } else { format!(" [{}]", flags) };

            msg.push_str(&format!("\n`{}` *{}*{}\n  Estado: {} | Por: {}\n", id, title, flags_str, status_str, created_by));
        }
        msg.push_str("\nUsa `/hecho ID` o `/confirmar ID`");
        Ok(msg)
    })
    .await;

    match result {
        Ok(msg) => msg,
        Err(e) => format!("Error: {}", e.1),
    }
}

pub(super) async fn handle_confirm(state: &AppState, user: &str, text: &str, accept: bool) -> String {
    let cmd = if accept { "/confirmar " } else { "/rechazar " };
    let id = text.strip_prefix(cmd).unwrap_or("").trim();
    if id.is_empty() {
        return format!("Uso: `{}<ID>`", cmd);
    }

    let id_str = id.to_string();
    let user_str = user.to_string();

    let result = crate::db::db_op(&state.db, move |conn| {
        let row = conn
            .query_row(
                "SELECT title, confirmed_by, rejected_by FROM tasks WHERE id = ?1",
                params![id_str],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| format!("DB: {}", e))?;

        let Some((title, confirmed_json, rejected_json)) = row else {
            return Ok(format!("Tarea `{}` no encontrada.", id_str));
        };

        if accept {
            let mut confirmed: Vec<String> = serde_json::from_str(&confirmed_json).unwrap_or_default();
            if !confirmed.contains(&user_str) {
                confirmed.push(user_str);
            }
            let confirmed_json = serde_json::to_string(&confirmed).unwrap_or_else(|_| "[]".to_string());
            conn.execute(
                "UPDATE tasks SET confirmed_by = ?1 WHERE id = ?2",
                params![confirmed_json, id_str],
            )
            .map_err(|e| format!("DB: {}", e))?;
            Ok(format!("Tarea *{}* confirmada por ti.", title))
        } else {
            let mut rejected: Vec<String> = serde_json::from_str(&rejected_json).unwrap_or_default();
            if !rejected.contains(&user_str) {
                rejected.push(user_str);
            }
            let rejected_json = serde_json::to_string(&rejected).unwrap_or_else(|_| "[]".to_string());
            conn.execute(
                "UPDATE tasks SET rejected_by = ?1, status = 'rechazada' WHERE id = ?2",
                params![rejected_json, id_str],
            )
            .map_err(|e| format!("DB: {}", e))?;
            Ok(format!("Tarea *{}* rechazada.", title))
        }
    })
    .await;

    match result {
        Ok(msg) => msg,
        Err(e) => format!("Error: {}", e.1),
    }
}

pub(super) async fn handle_done(state: &AppState, user: &str, text: &str) -> String {
    let id = text.strip_prefix("/hecho ").unwrap_or("").trim();
    if id.is_empty() {
        return "Uso: `/hecho <ID>`".to_string();
    }

    let id_str = id.to_string();
    let user_str = user.to_string();

    let result = crate::db::db_op(&state.db, move |conn| {
        let row = conn
            .query_row(
                "SELECT title, assigned_to, created_by FROM tasks WHERE id = ?1",
                params![id_str],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| format!("DB: {}", e))?;

        let Some((title, assigned_json, created_by)) = row else {
            return Ok(format!("Tarea `{}` no encontrada.", id_str));
        };

        let assigned: Vec<String> = serde_json::from_str(&assigned_json).unwrap_or_default();
        let is_assigned = assigned.contains(&"all".to_string())
            || assigned.iter().any(|a| a.to_lowercase() == user_str.to_lowercase());

        if created_by != user_str && !is_assigned {
            return Ok("No tienes permiso para completar esta tarea.".to_string());
        }

        conn.execute(
            "UPDATE tasks SET status = 'completada' WHERE id = ?1",
            params![id_str],
        )
        .map_err(|e| format!("DB: {}", e))?;

        Ok(format!("Tarea *{}* completada!", title))
    })
    .await;

    match result {
        Ok(msg) => msg,
        Err(e) => format!("Error: {}", e.1),
    }
}

pub(super) async fn handle_progress(state: &AppState, user: &str, text: &str) -> String {
    let project_name = text.strip_prefix("/avance").unwrap_or("").trim();
    let project_name_str = project_name.to_string();
    let _ = user;

    let result = crate::db::db_op(&state.db, move |conn| {
        if project_name_str.is_empty() {
            // Show all projects progress
            let mut stmt = conn
                .prepare("SELECT id, name FROM projects")
                .map_err(|e| format!("DB: {}", e))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| format!("DB: {}", e))?;
            let mut projects = Vec::new();
            for r in rows {
                projects.push(r.map_err(|e| format!("DB row: {}", e))?);
            }

            if projects.is_empty() {
                return Ok("*Avance*\n\nNo hay proyectos.".to_string());
            }

            let mut msg = String::from("*Avance de proyectos*\n");
            for (id, name) in &projects {
                let total: i64 = conn
                    .query_row("SELECT COUNT(*) FROM tasks WHERE project_id = ?1", params![id], |row| row.get(0))
                    .unwrap_or(0);
                let done: i64 = conn
                    .query_row("SELECT COUNT(*) FROM tasks WHERE project_id = ?1 AND status = 'completada'", params![id], |row| row.get(0))
                    .unwrap_or(0);
                let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u64 } else { 0 };
                let bar = progress_bar(pct as f64);
                msg.push_str(&format!("\n*{}*\n{} {}% ({}/{})\n", name, bar, pct, done, total));
            }
            return Ok(msg);
        }

        // Find specific project
        let project = conn
            .query_row(
                "SELECT id, name FROM projects WHERE LOWER(name) LIKE '%' || LOWER(?1) || '%' OR id = ?1",
                params![project_name_str],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|e| format!("DB: {}", e))?;

        let Some((project_id, project_name)) = project else {
            return Ok(format!("Proyecto '{}' no encontrado.", project_name_str));
        };

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE project_id = ?1", params![project_id], |row| row.get(0))
            .unwrap_or(0);
        let done: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE project_id = ?1 AND status = 'completada'", params![project_id], |row| row.get(0))
            .unwrap_or(0);
        let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u64 } else { 0 };
        let bar = progress_bar(pct as f64);

        let mut msg = format!("*{}*\n{} {}% ({}/{})\n", project_name, bar, pct, done, total);

        // List tasks
        let mut stmt = conn
            .prepare("SELECT id, title, status FROM tasks WHERE project_id = ?1")
            .map_err(|e| format!("DB: {}", e))?;
        let rows = stmt
            .query_map(params![project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("DB: {}", e))?;

        for r in rows {
            let (tid, ttitle, tstatus) = r.map_err(|e| format!("DB row: {}", e))?;
            let icon = match tstatus.as_str() {
                "completada" => "done",
                "rechazada" => "x",
                "en_progreso" => ">>",
                _ => "  ",
            };
            msg.push_str(&format!("\n[{}] `{}` {}", icon, tid, ttitle));
        }
        Ok(msg)
    })
    .await;

    match result {
        Ok(msg) => msg,
        Err(e) => format!("Error: {}", e.1),
    }
}
