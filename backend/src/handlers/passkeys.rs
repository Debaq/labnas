//! Passkeys (WebAuthn): iniciar sesion con la huella, Face ID, Windows Hello o una
//! llave de seguridad, sin contraseña.
//!
//! Verificacion propia con criptografia pura en Rust (p256 / rsa / ciborium) en vez
//! de `webauthn-rs`, que depende de OpenSSL. Politica de atestacion "none": no se
//! verifica el fabricante del autenticador, solo su clave publica (como la mayoria
//! de los sitios).
//!
//! Requisitos del navegador: contexto seguro (HTTPS) y un nombre de dominio (no
//! una IP). Sirve por el puente (https://labnas.tudominio.cl) o en la LAN con
//! `https://labnas.local` y el certificado instalado. La passkey queda ligada a ese
//! dominio (`rp_id`).

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};

use crate::db::{db_op, db_op_status};
use crate::state::{AppState, SessionInfo};
use crate::error::{ApiError, internal};

const CHALLENGE_TTL: Duration = Duration::from_secs(300);
const ALG_ES256: i64 = -7;
const ALG_RS256: i64 = -257;
const FLAG_UP: u8 = 0x01;
const FLAG_AT: u8 = 0x40;


fn bad(m: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, m.to_string())
}

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

fn unb64(s: &str) -> Result<Vec<u8>, ApiError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .map_err(|_| bad("Dato base64 invalido"))
}

// ─── Desafios pendientes ───

#[derive(Debug, Clone)]
enum Purpose {
    Register(String),
    Login,
}

struct Pending {
    purpose: Purpose,
    rp_id: String,
    origin: String,
    created: Instant,
}

static CHALLENGES: LazyLock<StdMutex<HashMap<String, Pending>>> = LazyLock::new(Default::default);

fn new_challenge(purpose: Purpose, rp_id: &str, origin: &str) -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("getrandom");
    let c = b64(&buf);
    if let Ok(mut m) = CHALLENGES.lock() {
        m.retain(|_, p| p.created.elapsed() < CHALLENGE_TTL);
        m.insert(c.clone(), Pending { purpose, rp_id: rp_id.to_string(), origin: origin.to_string(), created: Instant::now() });
    }
    c
}

/// Consume un desafio (un solo uso)
fn take_challenge(c: &str) -> Option<Pending> {
    let mut m = CHALLENGES.lock().ok()?;
    let p = m.remove(c)?;
    (p.created.elapsed() < CHALLENGE_TTL).then_some(p)
}

/// Dominio y origen de la pagina que pide la passkey (cabecera Origin)
fn rp_from_headers(headers: &HeaderMap) -> Result<(String, String), ApiError> {
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| bad("Falta la cabecera Origin"))?
        .to_string();
    let url = reqwest::Url::parse(&origin).map_err(|_| bad("Origin invalido"))?;
    let host = match url.host() {
        Some(url::Host::Domain(d)) => d.to_ascii_lowercase(),
        _ => return Err(bad("Las passkeys necesitan un nombre de dominio (no una IP): entra por el puente o por labnas.local con HTTPS")),
    };
    let secure = url.scheme() == "https" || (url.scheme() == "http" && host == "localhost");
    if !secure {
        return Err(bad("Las passkeys necesitan HTTPS"));
    }
    Ok((host, origin))
}

// ─── Datos del autenticador ───

#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
}

struct AuthData<'a> {
    rp_id_hash: &'a [u8],
    flags: u8,
    sign_count: u32,
    /// (credential id, clave COSE) si viene AT
    attested: Option<(Vec<u8>, Vec<u8>)>,
}

fn parse_auth_data(d: &[u8]) -> Result<AuthData<'_>, ApiError> {
    if d.len() < 37 {
        return Err(bad("authenticatorData corto"));
    }
    let flags = d[32];
    let sign_count = u32::from_be_bytes([d[33], d[34], d[35], d[36]]);
    let attested = if flags & FLAG_AT != 0 {
        if d.len() < 55 {
            return Err(bad("Datos de credencial incompletos"));
        }
        let len = u16::from_be_bytes([d[53], d[54]]) as usize;
        let id_end = 55 + len;
        if d.len() <= id_end || len == 0 || len > 1023 {
            return Err(bad("Id de credencial invalido"));
        }
        let cred_id = d[55..id_end].to_vec();
        // La clave COSE es un mapa CBOR; puede seguir una extension: se re-serializa solo la clave
        let key: ciborium::Value = ciborium::de::from_reader(&d[id_end..]).map_err(|_| bad("Clave publica invalida"))?;
        let mut key_bytes = Vec::new();
        ciborium::ser::into_writer(&key, &mut key_bytes).map_err(|_| bad("Clave publica invalida"))?;
        Some((cred_id, key_bytes))
    } else {
        None
    };
    Ok(AuthData { rp_id_hash: &d[..32], flags, sign_count, attested })
}

fn cose_get(map: &[(ciborium::Value, ciborium::Value)], label: i64) -> Option<&ciborium::Value> {
    map.iter().find(|(k, _)| k.as_integer().map(i128::from) == Some(label as i128)).map(|(_, v)| v)
}

fn cose_bytes(map: &[(ciborium::Value, ciborium::Value)], label: i64) -> Option<Vec<u8>> {
    cose_get(map, label).and_then(|v| v.as_bytes().cloned())
}

/// Algoritmo de una clave COSE soportada
fn cose_alg(key: &[u8]) -> Result<i64, ApiError> {
    let v: ciborium::Value = ciborium::de::from_reader(key).map_err(|_| bad("Clave publica invalida"))?;
    let map = v.as_map().ok_or_else(|| bad("Clave publica invalida"))?;
    let alg = cose_get(map, 3).and_then(|a| a.as_integer()).map(i128::from).ok_or_else(|| bad("Clave sin algoritmo"))? as i64;
    match alg {
        ALG_ES256 => {
            let (x, y) = (cose_bytes(map, -2), cose_bytes(map, -3));
            if x.map(|x| x.len()) != Some(32) || y.map(|y| y.len()) != Some(32) {
                return Err(bad("Clave P-256 invalida"));
            }
        }
        ALG_RS256 => {
            if cose_bytes(map, -1).is_none() || cose_bytes(map, -2).is_none() {
                return Err(bad("Clave RSA invalida"));
            }
        }
        _ => return Err(bad("Algoritmo de passkey no soportado (se aceptan ES256 y RS256)")),
    }
    Ok(alg)
}

/// Verifica la firma del autenticador sobre authData || sha256(clientDataJSON)
pub fn verify_signature(key: &[u8], signed: &[u8], sig: &[u8]) -> Result<(), ApiError> {
    let v: ciborium::Value = ciborium::de::from_reader(key).map_err(|_| bad("Clave publica invalida"))?;
    let map = v.as_map().ok_or_else(|| bad("Clave publica invalida"))?;
    let invalid = || (StatusCode::UNAUTHORIZED, "Firma de la passkey invalida".to_string());
    match cose_alg(key)? {
        ALG_ES256 => {
            use p256::ecdsa::signature::Verifier;
            let mut sec1 = vec![0x04];
            sec1.extend(cose_bytes(map, -2).unwrap_or_default());
            sec1.extend(cose_bytes(map, -3).unwrap_or_default());
            let vk = p256::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| bad("Clave P-256 invalida"))?;
            let sig = p256::ecdsa::Signature::from_der(sig).map_err(|_| invalid())?;
            vk.verify(signed, &sig).map_err(|_| invalid())
        }
        ALG_RS256 => {
            use rsa::signature::Verifier;
            let n = cose_bytes(map, -1).unwrap_or_default();
            let e = cose_bytes(map, -2).unwrap_or_default();
            if n.len() < 256 {
                return Err(bad("Clave RSA demasiado corta (minimo 2048 bits)"));
            }
            let pk = rsa::PublicKey::new(rsa::BoxedUint::from_be_slice_vartime(&n), rsa::BoxedUint::from_be_slice_vartime(&e))
                .map_err(|_| bad("Clave RSA invalida"))?;
            let vk = rsa::pkcs1v15::VerifyingKey::<rsa::sha2::Sha256>::new(pk);
            let sig = rsa::pkcs1v15::Signature::try_from(sig).map_err(|_| invalid())?;
            vk.verify(signed, &sig).map_err(|_| invalid())
        }
        _ => Err(invalid()),
    }
}

fn check_client_data(raw: &[u8], kind: &str) -> Result<(ClientData, Pending), ApiError> {
    let cd: ClientData = serde_json::from_slice(raw).map_err(|_| bad("clientDataJSON invalido"))?;
    if cd.kind != kind {
        return Err(bad("Tipo de operacion WebAuthn incorrecto"));
    }
    let pending = take_challenge(&cd.challenge).ok_or((StatusCode::UNAUTHORIZED, "Desafio vencido o desconocido: vuelve a intentarlo".to_string()))?;
    if cd.origin != pending.origin {
        return Err((StatusCode::UNAUTHORIZED, "La passkey se uso desde otro sitio".to_string()));
    }
    Ok((cd, pending))
}

// ─── Registro (usuario con sesion) ───

/// POST /api/auth/passkey/register/options
pub async fn register_options(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (rp_id, origin) = rp_from_headers(&headers)?;
    let (u, r) = (session.username.clone(), rp_id.clone());
    let existing: Vec<String> = db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare("SELECT id FROM webauthn_credentials WHERE username = ?1 AND rp_id = ?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![u, r], |row| row.get(0)).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    })
    .await?;
    let challenge = new_challenge(Purpose::Register(session.username.clone()), &rp_id, &origin);
    Ok(Json(serde_json::json!({
        "challenge": challenge,
        "rp": { "id": rp_id, "name": "LabNAS" },
        "user": { "id": b64(session.username.as_bytes()), "name": session.username, "displayName": session.username },
        "pubKeyCredParams": [{ "type": "public-key", "alg": ALG_ES256 }, { "type": "public-key", "alg": ALG_RS256 }],
        "timeout": 120000,
        "attestation": "none",
        "authenticatorSelection": { "residentKey": "preferred", "userVerification": "preferred" },
        "excludeCredentials": existing.iter().map(|id| serde_json::json!({ "type": "public-key", "id": id })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    #[serde(default)]
    pub name: String,
    pub id: String,
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    #[serde(rename = "attestationObject")]
    pub attestation_object: String,
}

/// POST /api/auth/passkey/register
pub async fn register(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_data = unb64(&req.client_data_json)?;
    let (_, pending) = check_client_data(&client_data, "webauthn.create")?;
    match &pending.purpose {
        Purpose::Register(u) if *u == session.username => {}
        _ => return Err((StatusCode::UNAUTHORIZED, "Desafio de otro usuario".to_string())),
    }

    let att = unb64(&req.attestation_object)?;
    let att: ciborium::Value = ciborium::de::from_reader(att.as_slice()).map_err(|_| bad("attestationObject invalido"))?;
    let att = att.as_map().ok_or_else(|| bad("attestationObject invalido"))?;
    let auth_data = att
        .iter()
        .find(|(k, _)| k.as_text() == Some("authData"))
        .and_then(|(_, v)| v.as_bytes())
        .ok_or_else(|| bad("Falta authData"))?;
    let ad = parse_auth_data(auth_data)?;
    if ad.rp_id_hash != Sha256::digest(pending.rp_id.as_bytes()).as_slice() {
        return Err((StatusCode::UNAUTHORIZED, "La passkey es de otro dominio".to_string()));
    }
    if ad.flags & FLAG_UP == 0 {
        return Err(bad("El autenticador no confirmo la presencia del usuario"));
    }
    let (cred_id, key) = ad.attested.ok_or_else(|| bad("Faltan los datos de la credencial"))?;
    if b64(&cred_id) != req.id.trim_end_matches('=') {
        return Err(bad("Id de credencial inconsistente"));
    }
    let alg = cose_alg(&key)?;

    let name = {
        let n = req.name.trim();
        if n.is_empty() { "Passkey".to_string() } else { crate::textutil::ellipsize(n, 60) }
    };
    let (id, u, r, nm, count) = (b64(&cred_id), session.username.clone(), pending.rp_id.clone(), name.clone(), ad.sign_count);
    db_op_status(&state.db, move |conn| {
        let exists: Option<String> = conn
            .query_row("SELECT id FROM webauthn_credentials WHERE id = ?1", [&id], |r| r.get(0))
            .optional()
            .map_err(internal)?;
        if exists.is_some() {
            return Err((StatusCode::CONFLICT, "Esa passkey ya esta registrada".to_string()));
        }
        conn.execute(
            "INSERT INTO webauthn_credentials (id, username, name, rp_id, alg, public_key, sign_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, u, nm, r, alg, key, count as i64, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(internal)?;
        Ok(())
    })
    .await?;
    state
        .log_activity("passkey_registrada", &format!("{} ({})", name, pending.rp_id), &session.username)
        .await;
    Ok(Json(serde_json::json!({ "ok": true, "name": name, "rp_id": pending.rp_id })))
}

// ─── Inicio de sesion (publico) ───

#[derive(Deserialize, Default)]
pub struct LoginOptionsRequest {
    #[serde(default)]
    pub username: Option<String>,
}

/// POST /api/auth/passkey/login/options
pub async fn login_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<LoginOptionsRequest>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (rp_id, origin) = rp_from_headers(&headers)?;
    // Con usuario: se listan sus passkeys; sin usuario, el navegador ofrece las guardadas (discoverable)
    let username = body.and_then(|Json(b)| b.username).map(|u| u.trim().to_lowercase()).filter(|u| !u.is_empty());
    let allow: Vec<String> = match username {
        Some(u) => {
            let r = rp_id.clone();
            db_op(&state.db, move |conn| {
                let mut stmt = conn
                    .prepare("SELECT id FROM webauthn_credentials WHERE username = ?1 AND rp_id = ?2")
                    .map_err(|e| e.to_string())?;
                let rows = stmt.query_map(params![u, r], |row| row.get(0)).map_err(|e| e.to_string())?;
                Ok(rows.filter_map(|x| x.ok()).collect())
            })
            .await?
        }
        None => Vec::new(),
    };
    let challenge = new_challenge(Purpose::Login, &rp_id, &origin);
    Ok(Json(serde_json::json!({
        "challenge": challenge,
        "rpId": rp_id,
        "timeout": 120000,
        "userVerification": "preferred",
        "allowCredentials": allow.iter().map(|id| serde_json::json!({ "type": "public-key", "id": id })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub id: String,
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    #[serde(rename = "authenticatorData")]
    pub authenticator_data: String,
    pub signature: String,
}

/// POST /api/auth/passkey/login: si la firma es valida, abre sesion (como el login con contraseña)
pub async fn login(
    State(state): State<AppState>,
    bridge: Option<Extension<crate::bridge::BridgeOrigin>>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let client_data = unb64(&req.client_data_json)?;
    let auth_data = unb64(&req.authenticator_data)?;
    let sig = unb64(&req.signature)?;
    let (_, pending) = check_client_data(&client_data, "webauthn.get")?;
    if !matches!(pending.purpose, Purpose::Login) {
        return Err(bad("Desafio de registro usado para iniciar sesion"));
    }

    let id = req.id.trim_end_matches('=').to_string();
    let cred: Option<(String, String, Vec<u8>, i64)> = {
        let id = id.clone();
        db_op(&state.db, move |conn| {
            conn.query_row(
                "SELECT username, rp_id, public_key, sign_count FROM webauthn_credentials WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(|e| e.to_string())
        })
        .await?
    };
    let unknown = || (StatusCode::UNAUTHORIZED, "Passkey desconocida (¿la borraste o es de otro sitio?)".to_string());
    let (username, rp_id, key, stored_count) = cred.ok_or_else(unknown)?;
    if rp_id != pending.rp_id {
        return Err(unknown());
    }

    let ad = parse_auth_data(&auth_data)?;
    if ad.rp_id_hash != Sha256::digest(rp_id.as_bytes()).as_slice() {
        return Err((StatusCode::UNAUTHORIZED, "La passkey es de otro dominio".to_string()));
    }
    if ad.flags & FLAG_UP == 0 {
        return Err((StatusCode::UNAUTHORIZED, "El autenticador no confirmo la presencia del usuario".to_string()));
    }
    let mut signed = auth_data.clone();
    signed.extend_from_slice(&Sha256::digest(&client_data));
    verify_signature(&key, &signed, &sig)?;
    // Contador que no avanza: posible clon de la llave (las passkeys sincronizadas usan 0)
    if (ad.sign_count != 0 || stored_count != 0) && i64::from(ad.sign_count) <= stored_count {
        state.log_activity("passkey_rechazada", "Contador de firmas no avanzo (posible clon)", &username).await;
        return Err((StatusCode::UNAUTHORIZED, "Passkey rechazada por seguridad (contador invalido)".to_string()));
    }

    let (u, i, c) = (username.clone(), id.clone(), ad.sign_count as i64);
    let user = db_op(&state.db, move |conn| {
        conn.execute(
            "UPDATE webauthn_credentials SET sign_count = ?2, last_used = ?3 WHERE id = ?1",
            params![i, c, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT role, perm_terminal, perm_impresion, perm_archivos_escritura FROM web_users WHERE username = ?1",
            [&u],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?, r.get::<_, bool>(2)?, r.get::<_, bool>(3)?)),
        )
        .optional()
        .map_err(|e| e.to_string())
    })
    .await?
    .ok_or_else(unknown)?;

    let role = crate::handlers::auth::role_from_str(&user.0);
    let permissions = crate::models::notifications::UserPermissions { terminal: user.1, impresion: user.2, archivos_escritura: user.3 };
    let detail = match &bridge {
        Some(Extension(o)) => {
            crate::handlers::auth::notify_internet_login(&state, &username, &o.client_ip);
            format!("Inicio de sesion con passkey por el puente PHP desde {}", o.client_ip)
        }
        None => "Inicio de sesion con passkey".to_string(),
    };
    state.log_activity("login_passkey", &detail, &username).await;
    crate::handlers::auth::issue_login(&state, &username, role, permissions, bridge.is_some()).await
}

// ─── Gestion (usuario con sesion) ───

#[derive(Serialize)]
pub struct PasskeyView {
    pub id: String,
    pub name: String,
    pub rp_id: String,
    pub created_at: String,
    pub last_used: Option<String>,
}

/// GET /api/auth/passkeys
pub async fn list(State(state): State<AppState>, Extension(session): Extension<SessionInfo>) -> Result<Json<Vec<PasskeyView>>, ApiError> {
    let u = session.username.clone();
    db_op(&state.db, move |conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, rp_id, created_at, last_used FROM webauthn_credentials WHERE username = ?1 ORDER BY created_at")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&u], |r| Ok(PasskeyView { id: r.get(0)?, name: r.get(1)?, rp_id: r.get(2)?, created_at: r.get(3)?, last_used: r.get(4)? }))
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    })
    .await
    .map(Json)
}

/// DELETE /api/auth/passkeys/{id}
pub async fn delete(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let (u, i) = (session.username.clone(), id.clone());
    let n = db_op(&state.db, move |conn| {
        conn.execute("DELETE FROM webauthn_credentials WHERE id = ?1 AND username = ?2", params![i, u])
            .map_err(|e| e.to_string())
    })
    .await?;
    if n == 0 {
        return Err((StatusCode::NOT_FOUND, "Passkey no encontrada".to_string()));
    }
    state.log_activity("passkey_eliminada", &id, &session.username).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
pub mod testing {
    //! Autenticador de software para los tests (ES256)
    use super::*;
    use p256::ecdsa::{signature::Signer, SigningKey};

    pub struct SoftKey {
        pub key: SigningKey,
        pub cred_id: Vec<u8>,
        pub count: u32,
    }

    impl SoftKey {
        pub fn new() -> Self {
            let mut seed = [0u8; 32];
            getrandom::fill(&mut seed).unwrap();
            seed[0] |= 1;
            let mut cred_id = vec![0u8; 16];
            getrandom::fill(&mut cred_id).unwrap();
            Self { key: SigningKey::from_slice(&seed).unwrap(), cred_id, count: 0 }
        }

        fn cose(&self) -> Vec<u8> {
            let point = self.key.verifying_key().to_sec1_point(false);
            let bytes = point.as_bytes();
            let map = ciborium::Value::Map(vec![
                (1.into(), 2.into()),
                (3.into(), ALG_ES256.into()),
                ((-1).into(), 1.into()),
                ((-2).into(), ciborium::Value::Bytes(bytes[1..33].to_vec())),
                ((-3).into(), ciborium::Value::Bytes(bytes[33..65].to_vec())),
            ]);
            let mut out = Vec::new();
            ciborium::ser::into_writer(&map, &mut out).unwrap();
            out
        }

        fn auth_data(&self, rp_id: &str, attested: bool) -> Vec<u8> {
            let mut d = Sha256::digest(rp_id.as_bytes()).to_vec();
            d.push(FLAG_UP | 0x04 | if attested { FLAG_AT } else { 0 });
            d.extend(self.count.to_be_bytes());
            if attested {
                d.extend([0u8; 16]);
                d.extend((self.cred_id.len() as u16).to_be_bytes());
                d.extend(&self.cred_id);
                d.extend(self.cose());
            }
            d
        }

        pub fn client_data(kind: &str, challenge: &str, origin: &str) -> Vec<u8> {
            serde_json::to_vec(&serde_json::json!({ "type": kind, "challenge": challenge, "origin": origin })).unwrap()
        }

        /// Respuesta a navigator.credentials.create()
        pub fn create(&self, rp_id: &str, challenge: &str, origin: &str) -> serde_json::Value {
            let att = ciborium::Value::Map(vec![
                ("fmt".into(), "none".into()),
                ("attStmt".into(), ciborium::Value::Map(vec![])),
                ("authData".into(), ciborium::Value::Bytes(self.auth_data(rp_id, true))),
            ]);
            let mut att_bytes = Vec::new();
            ciborium::ser::into_writer(&att, &mut att_bytes).unwrap();
            serde_json::json!({
                "name": "Llave de prueba",
                "id": b64(&self.cred_id),
                "clientDataJSON": b64(&Self::client_data("webauthn.create", challenge, origin)),
                "attestationObject": b64(&att_bytes),
            })
        }

        /// Respuesta a navigator.credentials.get()
        pub fn get(&mut self, rp_id: &str, challenge: &str, origin: &str) -> serde_json::Value {
            self.count += 1;
            let ad = self.auth_data(rp_id, false);
            let cd = Self::client_data("webauthn.get", challenge, origin);
            let mut signed = ad.clone();
            signed.extend_from_slice(&Sha256::digest(&cd));
            let sig: p256::ecdsa::Signature = self.key.sign(&signed);
            serde_json::json!({
                "id": b64(&self.cred_id),
                "clientDataJSON": b64(&cd),
                "authenticatorData": b64(&ad),
                "signature": b64(&sig.to_der().to_bytes()),
            })
        }
    }
}
