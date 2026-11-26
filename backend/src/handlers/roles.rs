//! Roles editables (solo admin): listar, crear, editar niveles por modulo, borrar y
//! restaurar la plantilla. La logica de permisos vive en `crate::roles`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::db::{db_op, db_op_status};
use crate::error::{internal, ApiError};
use crate::roles::{Level, ADMIN, PENDING};
use crate::state::{AppState, SessionInfo};

#[derive(Debug, Serialize)]
pub struct RoleOut {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Plantilla de origen (sus niveles se usan para los modulos nuevos)
    pub template: Option<String>,
    /// Vencimiento sugerido al asignar el rol (dias)
    pub expiry_days: Option<i64>,
    /// `admin`: no se edita
    pub locked: bool,
    pub levels: BTreeMap<String, Level>,
    /// Cuentas web con este rol
    pub users: i64,
}

#[derive(Debug, Serialize)]
pub struct RolesResponse {
    /// Modulos que se pueden configurar (sin el dashboard, que ven todos)
    pub modules: Vec<&'static str>,
    pub roles: Vec<RoleOut>,
}

/// Ids de rol: minusculas, numeros, `_` y `-` (van en las listas de acceso por carpeta)
pub fn valid_role_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Nombre visible de un rol
pub fn role_name(conn: &Connection, id: &str) -> String {
    if id == PENDING {
        return "Pendiente".to_string();
    }
    conn.query_row("SELECT name FROM roles WHERE id = ?1", params![id], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
        .unwrap_or_else(|| id.to_string())
}

fn configurable_modules() -> Vec<&'static str> {
    crate::modules::ids().into_iter().filter(|m| *m != "dashboard").collect()
}

fn load_roles(conn: &Connection) -> Result<Vec<RoleOut>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.name, r.description, r.template, r.expiry_days,
                    (SELECT COUNT(*) FROM web_users u WHERE u.role = r.id)
             FROM roles r ORDER BY r.display_order, r.name",
        )
        .map_err(|e| e.to_string())?;
    let mut roles: Vec<RoleOut> = stmt
        .query_map([], |r| {
            let id: String = r.get(0)?;
            Ok(RoleOut {
                locked: id == ADMIN,
                id,
                name: r.get(1)?,
                description: r.get(2)?,
                template: r.get(3)?,
                expiry_days: r.get(4)?,
                levels: BTreeMap::new(),
                users: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let modules = configurable_modules();
    for role in roles.iter_mut() {
        let stored = crate::roles::role_levels(conn, &role.id);
        role.levels = modules
            .iter()
            .map(|m| {
                let l = if role.locked { Level::Gestionar } else { stored.get(*m).copied().unwrap_or_default() };
                (m.to_string(), l)
            })
            .collect();
    }
    Ok(roles)
}

/// GET /api/roles
pub async fn list(State(state): State<AppState>) -> Result<Json<RolesResponse>, ApiError> {
    let roles = db_op(&state.db, load_roles).await?;
    Ok(Json(RolesResponse { modules: configurable_modules(), roles }))
}

fn clean_text(raw: &str, max: usize) -> String {
    let s: String = raw.chars().filter(|c| !c.is_control()).collect();
    crate::textutil::take_chars(s.trim(), max).trim().to_string()
}

/// Id a partir del nombre: "Jefe de taller" -> "jefe_de_taller"
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let s = crate::textutil::take_chars(out.trim_matches('_'), 28).trim_matches('_').to_string();
    if s.is_empty() { "rol".to_string() } else { s }
}

#[derive(Debug, Deserialize)]
pub struct CreateRole {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Rol del que se copian los niveles (y la plantilla)
    #[serde(default)]
    pub copy_from: Option<String>,
}

/// POST /api/roles
pub async fn create(
    State(state): State<AppState>,
    Extension(me): Extension<SessionInfo>,
    Json(req): Json<CreateRole>,
) -> Result<Json<RoleOut>, ApiError> {
    let name = clean_text(&req.name, 40);
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Ponle un nombre al rol".into()));
    }
    let description = clean_text(&req.description, 200);
    let copy_from = req.copy_from.clone().filter(|c| c != ADMIN && c != PENDING);
    let n = name.clone();
    let created = db_op_status(&state.db, move |conn| {
        let base = slug(&n);
        let mut id = base.clone();
        let mut i = 2;
        while id == ADMIN || id == PENDING || crate::roles::role_exists(conn, &id) {
            id = format!("{}_{}", base, i);
            i += 1;
        }
        let tx = conn.unchecked_transaction().map_err(internal)?;
        let template: Option<String> = match &copy_from {
            Some(src) => tx
                .query_row("SELECT template FROM roles WHERE id = ?1", params![src], |r| r.get::<_, Option<String>>(0))
                .optional()
                .map_err(internal)?
                .ok_or((StatusCode::BAD_REQUEST, "El rol a copiar no existe".to_string()))?,
            None => None,
        };
        tx.execute(
            "INSERT INTO roles (id, name, description, template, display_order) VALUES (?1, ?2, ?3, ?4, 200)",
            params![id, n, description, template],
        )
        .map_err(internal)?;
        if let Some(src) = &copy_from {
            tx.execute(
                "INSERT INTO role_modules (role_id, module_id, level) SELECT ?1, module_id, level FROM role_modules WHERE role_id = ?2",
                params![id, src],
            )
            .map_err(internal)?;
        }
        tx.commit().map_err(internal)?;
        load_roles(conn).map_err(internal)?.into_iter().find(|r| r.id == id).ok_or(internal("rol no creado"))
    })
    .await?;
    state.log_activity("Rol creado", &format!("{} ({})", created.name, created.id), &me.username).await;
    Ok(Json(created))
}

#[derive(Debug, Deserialize)]
pub struct UpdateRole {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub expiry_days: Option<i64>,
    pub levels: BTreeMap<String, Level>,
}

/// PUT /api/roles/{id}
pub async fn update(
    State(state): State<AppState>,
    Extension(me): Extension<SessionInfo>,
    Path(id): Path<String>,
    Json(req): Json<UpdateRole>,
) -> Result<Json<RoleOut>, ApiError> {
    if id == ADMIN {
        return Err((StatusCode::BAD_REQUEST, "El rol Administrador no se edita: lo puede todo".into()));
    }
    let name = clean_text(&req.name, 40);
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Ponle un nombre al rol".into()));
    }
    let modules = configurable_modules();
    if let Some(bad) = req.levels.keys().find(|m| !modules.contains(&m.as_str())) {
        return Err((StatusCode::BAD_REQUEST, format!("Modulo desconocido: {}", bad)));
    }
    if req.expiry_days.is_some_and(|d| !(1..=3650).contains(&d)) {
        return Err((StatusCode::BAD_REQUEST, "El vencimiento va de 1 a 3650 dias".into()));
    }
    let description = clean_text(&req.description, 200);
    let (i, n) = (id.clone(), name.clone());
    let levels = req.levels.clone();
    let expiry = req.expiry_days;
    let updated = db_op_status(&state.db, move |conn| {
        let tx = conn.unchecked_transaction().map_err(internal)?;
        let rows = tx
            .execute(
                "UPDATE roles SET name = ?1, description = ?2, expiry_days = ?3 WHERE id = ?4",
                params![n, description, expiry, i],
            )
            .map_err(internal)?;
        if rows == 0 {
            return Err((StatusCode::NOT_FOUND, "Rol no encontrado".to_string()));
        }
        for (m, l) in &levels {
            tx.execute(
                "INSERT INTO role_modules (role_id, module_id, level) VALUES (?1, ?2, ?3)
                 ON CONFLICT(role_id, module_id) DO UPDATE SET level = excluded.level",
                params![i, m, *l as i64],
            )
            .map_err(internal)?;
        }
        tx.commit().map_err(internal)?;
        load_roles(conn).map_err(internal)?.into_iter().find(|r| r.id == i).ok_or(internal("rol no encontrado"))
    })
    .await?;
    after_change(&state).await;
    state.log_activity("Rol editado", &format!("{} ({})", updated.name, updated.id), &me.username).await;
    Ok(Json(updated))
}

/// POST /api/roles/{id}/reset: vuelve a los niveles de su plantilla
pub async fn reset(
    State(state): State<AppState>,
    Extension(me): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<Json<RoleOut>, ApiError> {
    let i = id.clone();
    let role = db_op_status(&state.db, move |conn| {
        let template: Option<String> = conn
            .query_row("SELECT template FROM roles WHERE id = ?1 AND id != 'admin'", params![i], |r| r.get(0))
            .optional()
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Rol no encontrado".to_string()))?;
        if template.is_none() {
            return Err((StatusCode::BAD_REQUEST, "Este rol no viene de una plantilla".to_string()));
        }
        conn.execute("DELETE FROM role_modules WHERE role_id = ?1", params![i]).map_err(internal)?;
        crate::roles::seed_levels(conn);
        load_roles(conn).map_err(internal)?.into_iter().find(|r| r.id == i).ok_or(internal("rol no encontrado"))
    })
    .await?;
    after_change(&state).await;
    state.log_activity("Rol restaurado", &role.id, &me.username).await;
    Ok(Json(role))
}

/// DELETE /api/roles/{id}: solo si nadie lo usa
pub async fn remove(
    State(state): State<AppState>,
    Extension(me): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if id == ADMIN {
        return Err((StatusCode::BAD_REQUEST, "El rol Administrador no se puede borrar".into()));
    }
    let i = id.clone();
    db_op_status(&state.db, move |conn| {
        let (users, chats): (i64, i64) = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM web_users WHERE role = ?1), (SELECT COUNT(*) FROM telegram_chats WHERE role = ?1)",
                params![i],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(internal)?;
        if users > 0 || chats > 0 {
            return Err((
                StatusCode::CONFLICT,
                format!("El rol esta en uso ({} cuentas, {} chats de Telegram): cambialas de rol antes", users, chats),
            ));
        }
        let rows = conn.execute("DELETE FROM roles WHERE id = ?1", params![i]).map_err(internal)?;
        if rows == 0 {
            return Err((StatusCode::NOT_FOUND, "Rol no encontrado".to_string()));
        }
        Ok(())
    })
    .await?;
    state.log_activity("Rol borrado", &id, &me.username).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Las sesiones abiertas toman los niveles nuevos y la interfaz recarga su menu
async fn after_change(state: &AppState) {
    crate::roles::refresh_sessions(state).await;
    state.events.publish("auth.changed", (), crate::events::Audience::All, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_de_rol() {
        assert_eq!(slug("Jefe de Taller"), "jefe_de_taller");
        assert_eq!(slug("Técnico ñandú"), "tecnico_nandu");
        assert_eq!(slug("  ¡¡ !!"), "rol");
        assert!(valid_role_id("jefe_de_taller"));
        assert!(!valid_role_id("Jefe"));
        assert!(!valid_role_id(""));
        assert!(!valid_role_id("a b"));
    }
}
