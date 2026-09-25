//! Clasificacion con Groq

use super::*;

// =====================
// Groq AI classification
// =====================

/// Clasificar email con Groq LLM
pub async fn classify_with_groq(
    client: &reqwest::Client,
    api_key: &str,
    email: &EmailMessage,
) -> Result<(String, String, String), String> {
    let body = serde_json::json!({
        "model": "llama-3.3-70b-versatile",
        "messages": [
            {
                "role": "system",
                "content": "Clasifica este correo electronico en una de estas categorias: urgente, tarea, informativo, spam.\nResponde SOLO con un JSON valido (sin markdown, sin ```): {\"clasificacion\": \"...\", \"resumen\": \"...\", \"accion\": \"...\"}\nDonde:\n- clasificacion: urgente, tarea, informativo o spam\n- resumen: resumen en 1-2 oraciones en espanol\n- accion: accion sugerida en espanol (ej: 'Responder con informacion solicitada', 'Archivar', 'Crear tarea de seguimiento')"
            },
            {
                "role": "user",
                "content": format!("De: {}\nAsunto: {}\n\n{}", email.from, email.subject, email.body_preview)
            }
        ],
        "temperature": 0.1,
        "max_tokens": 300
    });

    let resp = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Error conectando a Groq: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Groq respondio {}: {}", status, text));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Error parseando respuesta Groq: {}", e))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "Respuesta de Groq sin contenido".to_string())?;

    // Parsear el JSON de la respuesta
    let parsed: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| format!("Error parseando JSON de Groq: {} - Contenido: {}", e, content))?;

    let clasificacion = parsed["clasificacion"]
        .as_str()
        .unwrap_or("informativo")
        .to_string();
    let resumen = parsed["resumen"]
        .as_str()
        .unwrap_or("Sin resumen")
        .to_string();
    let accion = parsed["accion"]
        .as_str()
        .unwrap_or("Sin accion sugerida")
        .to_string();

    Ok((clasificacion, resumen, accion))
}
