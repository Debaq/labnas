//! Aprobacion de compras firmada con passkey (idea 58). El desafio queda ligado en el servidor
//! a la compra, el paso y el monto, con vencimiento y un solo uso: una firma tomada para otra
//! compra, otro monto o ya usada no sirve. Reusa la verificacion de `handlers::passkeys`.

use axum::http::{HeaderMap, StatusCode};
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::db::db_op;
use crate::error::ApiError;
use crate::handlers::passkeys::{rp_from_headers, unb64, verify_signature};
use crate::state::AppState;

const TTL_SECS: i64 = 300;
const FLAG_UP: u8 = 0x01;

/// Respuesta del autenticador a `navigator.credentials.get()` (todo en base64url)
#[derive(Debug, Clone, Deserialize)]
pub struct Assertion {
    pub id: String,
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    #[serde(rename = "authenticatorData")]
    pub authenticator_data: String,
    pub signature: String,
}

#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
}

fn unauthorized(m: &str) -> ApiError {
    (StatusCode::UNAUTHORIZED, m.to_string())
}

/// Opciones de WebAuthn para firmar la aprobacion de `purchase_id` por `amount`
pub async fn options(
    state: &AppState,
    headers: &HeaderMap,
    username: &str,
    purchase_id: &str,
    step: &str,
    amount: f64,
) -> Result<serde_json::Value, ApiError> {
    let (rp_id, origin) = rp_from_headers(headers)?;
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(|e| crate::error::internal(e.to_string()))?;
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf);
    let (u, p, s, c, r, o) = (username.to_string(), purchase_id.to_string(), step.to_string(), challenge.clone(), rp_id.clone(), origin);
    let creds: Vec<String> = db_op(&state.db, move |conn| {
        let now = chrono::Utc::now().timestamp();
        conn.execute("DELETE FROM fin_passkey_challenges WHERE expires_at < ?1", [now]).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO fin_passkey_challenges (challenge, username, purchase_id, step, amount, rp_id, origin, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![c, u, p, s, amount, r, o, now + TTL_SECS],
        )
        .map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id FROM webauthn_credentials WHERE username = ?1 AND rp_id = ?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![u, r], |row| row.get(0)).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    })
    .await?;
    if creds.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No tienes una passkey registrada en este sitio: crea una en tu perfil (Seguridad) antes de aprobar".into()));
    }
    Ok(serde_json::json!({
        "challenge": challenge,
        "rpId": rp_id,
        "timeout": 120000,
        "userVerification": "preferred",
        "allowCredentials": creds.iter().map(|id| serde_json::json!({ "type": "public-key", "id": id })).collect::<Vec<_>>(),
    }))
}

/// Verifica que `username` firmo esta aprobacion (compra, paso y monto). Consume el desafio.
pub async fn verify(state: &AppState, username: &str, purchase_id: &str, step: &str, amount: f64, a: &Assertion) -> Result<(), ApiError> {
    let client_data = unb64(&a.client_data_json)?;
    let auth_data = unb64(&a.authenticator_data)?;
    let sig = unb64(&a.signature)?;
    let cd: ClientData = serde_json::from_slice(&client_data).map_err(|_| (StatusCode::BAD_REQUEST, "clientDataJSON invalido".to_string()))?;
    if cd.kind != "webauthn.get" {
        return Err((StatusCode::BAD_REQUEST, "Tipo de operacion WebAuthn incorrecto".into()));
    }
    // El desafio se consume aunque la firma falle: un solo intento por desafio
    let ch = cd.challenge.clone();
    let stored: Option<(String, String, String, f64, String, String, i64)> = db_op(&state.db, move |conn| {
        let row = conn
            .query_row(
                "SELECT username, purchase_id, step, amount, rp_id, origin, expires_at FROM fin_passkey_challenges WHERE challenge = ?1",
                [&ch],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM fin_passkey_challenges WHERE challenge = ?1", [&ch]).map_err(|e| e.to_string())?;
        Ok(row)
    })
    .await?;
    let (c_user, c_purchase, c_step, c_amount, rp_id, origin, expires) =
        stored.ok_or_else(|| unauthorized("Desafio vencido o desconocido: vuelve a firmar"))?;
    if expires < chrono::Utc::now().timestamp() {
        return Err(unauthorized("El desafio de la passkey vencio: vuelve a firmar"));
    }
    if c_user != username || c_purchase != purchase_id || c_step != step || (c_amount - amount).abs() > 0.5 {
        return Err(unauthorized("La firma corresponde a otra compra, otro paso u otro monto"));
    }
    if cd.origin != origin {
        return Err(unauthorized("La passkey se uso desde otro sitio"));
    }

    let id = a.id.trim_end_matches('=').to_string();
    let (id2, uname) = (id.clone(), username.to_string());
    let cred: Option<(String, Vec<u8>, i64)> = db_op(&state.db, move |conn| {
        conn.query_row(
            "SELECT rp_id, public_key, sign_count FROM webauthn_credentials WHERE id = ?1 AND username = ?2",
            params![id2, uname],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())
    })
    .await?;
    let (cred_rp, key, stored_count) = cred.ok_or_else(|| unauthorized("Esa passkey no es tuya o ya no existe"))?;
    if cred_rp != rp_id {
        return Err(unauthorized("La passkey es de otro dominio"));
    }
    if auth_data.len() < 37 {
        return Err((StatusCode::BAD_REQUEST, "authenticatorData corto".into()));
    }
    if auth_data[..32] != Sha256::digest(rp_id.as_bytes())[..] {
        return Err(unauthorized("La passkey es de otro dominio"));
    }
    if auth_data[32] & FLAG_UP == 0 {
        return Err(unauthorized("El autenticador no confirmo la presencia del usuario"));
    }
    let mut signed = auth_data.clone();
    signed.extend_from_slice(&Sha256::digest(&client_data));
    verify_signature(&key, &signed, &sig)?;
    let count = u32::from_be_bytes([auth_data[33], auth_data[34], auth_data[35], auth_data[36]]);
    if (count != 0 || stored_count != 0) && i64::from(count) <= stored_count {
        state.log_activity("passkey_rechazada", "Contador de firmas no avanzo (posible clon)", username).await;
        return Err(unauthorized("Passkey rechazada por seguridad (contador invalido)"));
    }
    db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE webauthn_credentials SET sign_count = ?2, last_used = ?3 WHERE id = ?1",
            params![id, i64::from(count), chrono::Utc::now().to_rfc3339()],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    })
    .await
}
