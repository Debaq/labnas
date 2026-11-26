//! Roles editables y permisos por modulo.
//!
//! Cada rol tiene, en cada modulo, un nivel: ninguno, ver, usar o gestionar. Una persona
//! hereda los niveles de su rol y el admin puede subir o bajar un modulo solo para ella
//! (`user_module_overrides`). `admin` lo puede todo y no se edita; `pendiente` no es un rol
//! de la tabla: es una cuenta que nadie aprobo todavia.
//!
//! Los roles que vienen de una plantilla (`roles.template`) reciben al arrancar el nivel de
//! la plantilla para los modulos que aun no tienen fila (modulos nuevos); los roles creados
//! a mano quedan sin acceso a los modulos nuevos hasta que el admin lo decida.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ADMIN: &str = "admin";
pub const PENDING: &str = "pendiente";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    #[default]
    Ninguno = 0,
    /// Consultar
    Ver = 1,
    /// Lo de todos los dias: imprimir, reservar, encolar, crear lo propio
    Usar = 2,
    /// Lo que antes era de operador: editar lo de otros, aprobar, controlar equipos
    Gestionar = 3,
}

impl Level {
    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => Level::Ver,
            2 => Level::Usar,
            3 => Level::Gestionar,
            _ => Level::Ninguno,
        }
    }
}

/// Lo que puede hacer una sesion (o un chat de Telegram), ya con las excepciones aplicadas
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Grants {
    pub admin: bool,
    pub levels: BTreeMap<String, Level>,
}

impl Grants {
    pub fn admin() -> Self {
        Self { admin: true, levels: BTreeMap::new() }
    }

    pub fn level(&self, module: &str) -> Level {
        if self.admin {
            return Level::Gestionar;
        }
        if module == "dashboard" {
            return Level::Ver;
        }
        self.levels.get(module).copied().unwrap_or_default()
    }

    pub fn can(&self, module: &str, level: Level) -> bool {
        self.level(module) >= level
    }

    /// Niveles de todos los modulos (para la interfaz)
    pub fn all_levels(&self) -> BTreeMap<String, Level> {
        crate::db::module_ids()
            .into_iter()
            .map(|m| {
                let l = self.level(m);
                (m.to_string(), l)
            })
            .collect()
    }
}

/// Nivel que da una plantilla en un modulo
pub fn template_level(template: &str, module: &str) -> Level {
    use Level::*;
    // files, network, printing, printers3d, tasks, notes, email, inventory, portfolio,
    // sensors, terminal, music, cameras, machines, deadlines
    let row: [Level; 15] = match template {
        // Rol de antes: preserva lo que podia hacer (gestiona donde habia rutas de operador)
        "operador" => [Usar, Gestionar, Usar, Gestionar, Gestionar, Usar, Usar, Usar, Usar, Ver, Usar, Usar, Usar, Gestionar, Gestionar],
        "tecnico" => [Usar, Gestionar, Usar, Gestionar, Usar, Usar, Usar, Gestionar, Ver, Ver, Usar, Usar, Usar, Gestionar, Usar],
        "investigador" => [Usar, Ver, Usar, Usar, Gestionar, Usar, Usar, Usar, Gestionar, Ver, Ninguno, Usar, Ninguno, Usar, Gestionar],
        "administrativo" => [Usar, Ninguno, Usar, Ver, Usar, Usar, Usar, Gestionar, Ver, Ninguno, Ninguno, Usar, Ninguno, Ver, Gestionar],
        "docente" => [Usar, Ver, Usar, Usar, Gestionar, Usar, Usar, Ver, Gestionar, Ver, Ninguno, Usar, Ninguno, Usar, Gestionar],
        "ayudante" => [Usar, Ver, Usar, Gestionar, Usar, Usar, Usar, Ver, Usar, Ver, Ninguno, Usar, Ninguno, Usar, Usar],
        "estudiante" => [Ver, Ninguno, Usar, Usar, Usar, Usar, Ninguno, Ver, Usar, Ver, Ninguno, Usar, Ninguno, Usar, Ver],
        // Rol de antes: preserva lo que podia hacer (escribir archivos era un permiso aparte)
        "observador" => [Ver, Usar, Usar, Usar, Usar, Usar, Usar, Usar, Usar, Ver, Ninguno, Usar, Ninguno, Usar, Usar],
        "externo" => [Ninguno, Ninguno, Ninguno, Usar, Ninguno, Ninguno, Ninguno, Ninguno, Ninguno, Ninguno, Ninguno, Ninguno, Ninguno, Usar, Ninguno],
        "auditor" => [Ver, Ver, Ver, Ver, Ver, Ver, Ninguno, Ver, Ver, Ver, Ninguno, Ver, Ninguno, Ver, Ver],
        _ => return Ninguno,
    };
    const COLS: [&str; 15] = [
        "files", "network", "printing", "printers3d", "tasks", "notes", "email", "inventory", "portfolio",
        "sensors", "terminal", "music", "cameras", "machines", "deadlines",
    ];
    COLS.iter().position(|c| *c == module).map(|i| row[i]).unwrap_or(Ninguno)
}

/// Completa los niveles de los roles con plantilla para los modulos que no tienen fila
pub fn seed_levels(conn: &Connection) {
    let Ok(mut stmt) = conn.prepare("SELECT id, template FROM roles WHERE template IS NOT NULL AND id != 'admin'") else {
        return;
    };
    let roles: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    for (id, template) in roles {
        for m in crate::db::module_ids() {
            if m == "dashboard" {
                continue;
            }
            let _ = conn.execute(
                "INSERT OR IGNORE INTO role_modules (role_id, module_id, level) VALUES (?1, ?2, ?3)",
                params![id, m, template_level(&template, m) as i64],
            );
        }
    }
}

/// Niveles de un rol, sin excepciones
pub fn role_levels(conn: &Connection, role: &str) -> BTreeMap<String, Level> {
    let mut out = BTreeMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT module_id, level FROM role_modules WHERE role_id = ?1") {
        if let Ok(rows) = stmt.query_map(params![role], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) {
            for (m, l) in rows.flatten() {
                out.insert(m, Level::from_i64(l));
            }
        }
    }
    out
}

/// Excepciones de una persona
pub fn user_overrides(conn: &Connection, username: &str) -> BTreeMap<String, Level> {
    let mut out = BTreeMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT module_id, level FROM user_module_overrides WHERE username = ?1") {
        if let Ok(rows) = stmt.query_map(params![username], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) {
            for (m, l) in rows.flatten() {
                out.insert(m, Level::from_i64(l));
            }
        }
    }
    out
}

/// Lo que puede un rol (sin persona: chats de Telegram sin cuenta vinculada)
pub fn grants_for_role(conn: &Connection, role: &str) -> Grants {
    match role {
        ADMIN => Grants::admin(),
        PENDING => Grants::default(),
        _ => Grants { admin: false, levels: role_levels(conn, role) },
    }
}

/// Lo que puede una persona: su rol mas sus excepciones
pub fn grants_for_user(conn: &Connection, username: &str, role: &str) -> Grants {
    let mut g = grants_for_role(conn, role);
    if !g.admin && role != PENDING {
        g.levels.extend(user_overrides(conn, username));
    }
    g
}

/// Cuenta web: (rol, permisos, vencimiento)
pub fn load_user(conn: &Connection, username: &str) -> Option<(String, Grants, Option<i64>)> {
    let (role, expires): (String, Option<i64>) = conn
        .query_row(
            "SELECT role, expires_at FROM web_users WHERE username = ?1",
            params![username],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .ok()
        .flatten()?;
    let grants = grants_for_user(conn, username, &role);
    Some((role, grants, expires))
}

pub fn role_exists(conn: &Connection, role: &str) -> bool {
    role == PENDING
        || conn
            .query_row("SELECT 1 FROM roles WHERE id = ?1", params![role], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
}

pub fn is_expired(expires_at: Option<i64>) -> bool {
    expires_at.is_some_and(|t| crate::state::now_unix() >= t)
}

/// Condicion SQL sobre `telegram_chats`: chats con al menos `level` en `module` (o admin).
/// Con cuenta vinculada manda la persona (rol + excepciones); sin cuenta, el rol del chat.
/// `module` y `level` vienen del codigo, nunca del usuario.
pub fn chats_with_level_sql(module: &'static str, level: Level) -> String {
    let l = level as i64;
    format!(
        "(role = 'admin' OR linked_web_user IN (SELECT u.username FROM web_users u \
           LEFT JOIN user_module_overrides o ON o.username = u.username AND o.module_id = '{module}' \
           LEFT JOIN role_modules r ON r.role_id = u.role AND r.module_id = '{module}' \
           WHERE (u.expires_at IS NULL OR u.expires_at > strftime('%s','now')) \
             AND (u.role = 'admin' OR COALESCE(o.level, r.level, 0) >= {l})) \
         OR (linked_web_user IS NULL AND role IN (SELECT role_id FROM role_modules WHERE module_id = '{module}' AND level >= {l})))"
    )
}

/// Cuentas web vigentes con al menos `level` en `module` (incluye a los admin)
pub fn users_with_level(conn: &Connection, module: &'static str, level: Level) -> rusqlite::Result<Vec<String>> {
    let sql = format!(
        "SELECT u.username FROM web_users u \
           LEFT JOIN user_module_overrides o ON o.username = u.username AND o.module_id = '{module}' \
           LEFT JOIN role_modules r ON r.role_id = u.role AND r.module_id = '{module}' \
         WHERE (u.expires_at IS NULL OR u.expires_at > strftime('%s','now')) \
           AND (u.role = 'admin' OR (u.role != 'pendiente' AND COALESCE(o.level, r.level, 0) >= {}))",
        level as i64
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    rows.collect()
}

/// Vuelve a calcular los permisos de las sesiones abiertas (tras editar roles o personas)
pub async fn refresh_sessions(state: &crate::state::AppState) {
    let users: Vec<String> = {
        let s = state.sessions.lock().await;
        let mut v: Vec<String> = s.values().map(|x| x.username.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    let fresh = crate::db::db_op(&state.db, move |conn| {
        Ok(users
            .into_iter()
            .map(|u| {
                let info = load_user(conn, &u);
                (u, info)
            })
            .collect::<Vec<_>>())
    })
    .await
    .unwrap_or_default();
    let mut sessions = state.sessions.lock().await;
    for (username, info) in fresh {
        match info {
            Some((role, grants, expires)) => {
                for s in sessions.values_mut().filter(|s| s.username == username) {
                    s.role = crate::models::notifications::UserRole(role.clone());
                    s.grants = grants.clone();
                    s.account_expires = expires;
                }
            }
            None => sessions.retain(|_, s| s.username != username),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plantillas_cubren_todos_los_modulos() {
        // Si se agrega un modulo, decidir su nivel en cada plantilla (y en COLS)
        for m in crate::db::module_ids() {
            if m == "dashboard" {
                continue;
            }
            assert!(
                ["operador", "observador", "auditor"].iter().any(|t| template_level(t, m) != Level::Ninguno),
                "el modulo {m} no aparece en las plantillas"
            );
        }
    }

    #[test]
    fn operador_y_observador_conservan_lo_de_antes() {
        assert_eq!(template_level("operador", "machines"), Level::Gestionar);
        assert_eq!(template_level("operador", "printers3d"), Level::Gestionar);
        assert_eq!(template_level("operador", "terminal"), Level::Usar);
        assert_eq!(template_level("observador", "terminal"), Level::Ninguno);
        assert_eq!(template_level("observador", "cameras"), Level::Ninguno);
        assert_eq!(template_level("observador", "printers3d"), Level::Usar);
        assert_eq!(template_level("auditor", "files"), Level::Ver);
        assert_eq!(template_level("auditor", "terminal"), Level::Ninguno);
    }

    #[test]
    fn admin_y_dashboard() {
        let g = Grants::admin();
        assert!(g.can("terminal", Level::Gestionar));
        let none = Grants::default();
        assert!(none.can("dashboard", Level::Ver));
        assert!(!none.can("files", Level::Ver));
    }
}
