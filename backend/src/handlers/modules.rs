use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::db;
use crate::state::AppState;

pub async fn list_modules(
    State(state): State<AppState>,
) -> Result<Json<Vec<db::ModuleInfo>>, (StatusCode, String)> {
    db::db_op(&state.db, |conn| Ok(db::get_modules(conn))).await.map(Json)
}

#[derive(Deserialize)]
pub struct ToggleBody {
    pub enabled: bool,
    /// Apagar aunque otro modulo encendido lo use
    #[serde(default)]
    pub force: bool,
}

/// Aviso si apagar `id` le quita algo a otro modulo encendido (None = nada que avisar)
pub fn disable_warning(id: &str, enabled: &std::collections::HashSet<String>) -> Option<String> {
    let users: Vec<String> = db::MODULE_USES
        .iter()
        .filter(|(m, u, _)| *u == id && enabled.contains(*m))
        .map(|(m, _, why)| format!("{} lo usa para {}", db::module_name(m), why))
        .collect();
    (!users.is_empty()).then(|| users.join("; "))
}

pub async fn toggle_module(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ToggleBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if id == "dashboard" {
        return Err((StatusCode::BAD_REQUEST, "No se puede desactivar el dashboard".into()));
    }

    if !body.enabled && !body.force {
        let on = state.enabled_modules.lock().await.clone();
        if let Some(why) = disable_warning(&id, &on) {
            return Err((StatusCode::CONFLICT, why));
        }
    }

    let id_clone = id.clone();
    let enabled = body.enabled;
    db::db_op(&state.db, move |conn| {
        db::set_module_enabled(conn, &id_clone, enabled)
    }).await?;

    // Update in-memory cache
    {
        let mut cache = state.enabled_modules.lock().await;
        if enabled {
            cache.insert(id.clone());
        } else {
            cache.remove(&id);
        }
    }

    // Tareas de fondo del modulo, sin reiniciar
    if enabled {
        crate::module_tasks::start(&state, &id);
    } else {
        crate::module_tasks::stop(&state, &id).await;
    }

    // Apagar la terminal corta tambien las sesiones abiertas desde Telegram
    if id == "terminal" && !enabled {
        let sessions: Vec<_> = state.tg_terminals.lock().await.drain().map(|(_, s)| s).collect();
        for mut s in sessions {
            let _ = s.child.kill().await;
        }
    }

    // Al encender: lo que usa y esta apagado (la interfaz lo menciona)
    let missing: Vec<&str> = if enabled {
        let on = state.enabled_modules.lock().await;
        db::module_uses(&id).into_iter().filter(|m| !on.contains(*m)).collect()
    } else {
        Vec::new()
    };

    Ok(Json(serde_json::json!({ "ok": true, "id": id, "enabled": enabled, "missing": missing })))
}

#[derive(Deserialize)]
pub struct ReorderItem {
    pub id: String,
    pub order: i32,
}

pub async fn reorder_modules(
    State(state): State<AppState>,
    Json(items): Json<Vec<ReorderItem>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pairs: Vec<(String, i32)> = items.into_iter().map(|i| (i.id, i.order)).collect();
    db::db_op(&state.db, move |conn| {
        db::set_module_order(conn, &pairs)
    }).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avisa_si_otro_modulo_lo_usa() {
        let on: std::collections::HashSet<String> = ["email".to_string(), "tasks".to_string()].into();
        assert!(disable_warning("tasks", &on).unwrap().contains("Correo"));
        assert!(disable_warning("email", &on).is_none());
        // Plazos apagado: apagar Portafolio no le quita nada a nadie
        assert!(disable_warning("portfolio", &on).is_none());
    }

    #[test]
    fn dependencias_entre_modulos_conocidos() {
        let ids = db::module_ids();
        for (m, u, _) in db::MODULE_USES {
            assert!(ids.contains(m) && ids.contains(u), "{m} -> {u}");
        }
    }
}
