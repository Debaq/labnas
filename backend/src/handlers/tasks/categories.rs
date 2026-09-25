//! Categorias de eventos

use super::*;

// ── Event Categories ──

use crate::models::tasks::EventCategory;

pub async fn list_categories(State(state): State<AppState>) -> Result<Json<Vec<EventCategory>>, (StatusCode, String)> {
    let cats = db_op(&state.db, |conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, color FROM event_categories ORDER BY name")
            .map_err(|e| format!("list_categories: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(EventCategory {
                    id: row.get("id")?,
                    name: row.get("name")?,
                    color: row.get("color")?,
                })
            })
            .map_err(|e| format!("list_categories query: {}", e))?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        Ok(rows)
    })
    .await?;
    Ok(Json(cats))
}

#[derive(Debug, Deserialize)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub color: String,
}

pub async fn create_category(
    State(state): State<AppState>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<Json<EventCategory>, (StatusCode, String)> {
    if req.name.trim().is_empty() || req.color.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Nombre y color requeridos".to_string()));
    }
    let cat = EventCategory {
        id: uuid::Uuid::new_v4().to_string()[..6].to_string(),
        name: req.name.trim().to_string(),
        color: req.color.trim().to_string(),
    };

    let c = cat.clone();
    db_op(&state.db, move |conn| {
        conn.execute(
            "INSERT INTO event_categories (id, name, color) VALUES (?1, ?2, ?3)",
            params![c.id, c.name, c.color],
        ).map_err(|e| format!("create_category: {}", e))?;
        Ok(())
    })
    .await?;

    Ok(Json(cat))
}

pub async fn update_category(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<Json<EventCategory>, (StatusCode, String)> {
    let result = db_op(&state.db, move |conn| {
        let mut cat: EventCategory = conn
            .query_row(
                "SELECT id, name, color FROM event_categories WHERE id = ?1",
                params![id],
                |row| {
                    Ok(EventCategory {
                        id: row.get("id")?,
                        name: row.get("name")?,
                        color: row.get("color")?,
                    })
                },
            )
            .map_err(|_| "Categoria no encontrada".to_string())?;

        if !req.name.trim().is_empty() { cat.name = req.name.trim().to_string(); }
        if !req.color.trim().is_empty() { cat.color = req.color.trim().to_string(); }

        conn.execute(
            "UPDATE event_categories SET name = ?1, color = ?2 WHERE id = ?3",
            params![cat.name, cat.color, cat.id],
        ).map_err(|e| format!("update_category: {}", e))?;

        Ok(cat)
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    Ok(Json(result))
}

pub async fn delete_category(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    db_op(&state.db, move |conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM event_categories WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|e| format!("delete_category: {}", e))?;

        if count == 0 {
            return Err("Categoria no encontrada".to_string());
        }

        // Limpiar la categoria de los eventos que la usan
        conn.execute(
            "UPDATE calendar_events SET category = NULL WHERE category = ?1",
            params![id],
        ).map_err(|e| format!("delete_category clear: {}", e))?;

        conn.execute("DELETE FROM event_categories WHERE id = ?1", params![id])
            .map_err(|e| format!("delete_category: {}", e))?;

        Ok(())
    })
    .await
    .map_err(|(_status, msg)| {
        if msg.contains("no encontrada") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    Ok(StatusCode::NO_CONTENT)
}
