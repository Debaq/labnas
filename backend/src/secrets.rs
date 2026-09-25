//! Cifrado de secretos en la base (token del bot, API keys, contraseñas de correo).
//!
//! XChaCha20-Poly1305 con una clave en `~/.labnas/secret.key` (0600, se genera sola).
//! Protege las copias de la base (respaldos, snapshots, copia diaria): sin la clave,
//! los secretos no se leen. Para restaurar una base en otra maquina hay que llevar
//! tambien `secret.key`.
//!
//! Se usa desde SQL con dos funciones registradas en cada conexion:
//!   `labnas_encrypt(x)` / `labnas_decrypt(x)`
//! Asi cada consulta solo envuelve la columna (p.ej. `SELECT labnas_decrypt(bot_token)`).

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::{
    aead::{Aead, Generate, Key, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use std::path::Path;
use std::sync::OnceLock;

const PREFIX: &str = "enc:v1:";
const NONCE_LEN: usize = 24;
const KEY_FILE: &str = "secret.key";

static CIPHER: OnceLock<XChaCha20Poly1305> = OnceLock::new();

/// Carga (o crea) la clave del directorio de datos. Llamar antes de abrir la base.
pub fn init(data_dir: &Path) -> Result<(), String> {
    let cipher = load_or_create(data_dir)?;
    // En tests se inicializa varias veces con homes distintos: gana la primera
    let _ = CIPHER.set(cipher);
    Ok(())
}

fn load_or_create(data_dir: &Path) -> Result<XChaCha20Poly1305, String> {
    use std::os::unix::fs::PermissionsExt;
    let path = data_dir.join(KEY_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
            let key = Key::<XChaCha20Poly1305>::generate();
            let bytes = key.to_vec();
            std::fs::write(&path, &bytes).map_err(|e| format!("No se pudo crear {}: {}", path.display(), e))?;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            println!("[LabNAS] Clave de cifrado creada: {}", path.display());
            bytes
        }
        Err(e) => return Err(format!("No se pudo leer {}: {}", path.display(), e)),
    };
    XChaCha20Poly1305::new_from_slice(&bytes).map_err(|_| format!("{} invalida (debe tener 32 bytes)", path.display()))
}

fn cipher() -> Result<&'static XChaCha20Poly1305, String> {
    CIPHER.get().ok_or_else(|| "Cifrado no inicializado".to_string())
}

pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(PREFIX)
}

/// Cifra `plain`. Si ya esta cifrado lo devuelve igual (nunca cifra dos veces).
pub fn encrypt(plain: &str) -> Result<String, String> {
    if is_encrypted(plain) {
        return Ok(plain.to_string());
    }
    let nonce = XNonce::generate();
    let ct = cipher()?
        .encrypt(&nonce, plain.as_bytes())
        .map_err(|_| "Error cifrando".to_string())?;
    let mut blob = nonce.to_vec();
    blob.extend_from_slice(&ct);
    Ok(format!("{}{}", PREFIX, B64.encode(blob)))
}

/// Descifra; un valor sin prefijo (texto plano de versiones anteriores) se devuelve tal cual.
pub fn decrypt(stored: &str) -> Result<String, String> {
    let Some(b64) = stored.strip_prefix(PREFIX) else {
        return Ok(stored.to_string());
    };
    let blob = B64.decode(b64).map_err(|_| "Secreto con formato invalido".to_string())?;
    if blob.len() < NONCE_LEN {
        return Err("Secreto con formato invalido".to_string());
    }
    let (nonce, ct) = blob.split_at(NONCE_LEN);
    let nonce = XNonce::try_from(nonce).map_err(|_| "Nonce invalido".to_string())?;
    let plain = cipher()?
        .decrypt(&nonce, ct)
        .map_err(|_| "No se pudo descifrar (¿secret.key distinta a la que cifro esta base?)".to_string())?;
    String::from_utf8(plain).map_err(|_| "Secreto no es UTF-8".to_string())
}

/// Registra labnas_encrypt / labnas_decrypt en una conexion SQLite (NULL -> NULL).
pub fn register_sql_functions(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    use rusqlite::functions::FunctionFlags;
    use rusqlite::types::{Value, ValueRef};

    fn wrap(
        f: fn(&str) -> Result<String, String>,
    ) -> impl Fn(&rusqlite::functions::Context) -> rusqlite::Result<Value> {
        move |ctx| match ctx.get_raw(0) {
            ValueRef::Null => Ok(Value::Null),
            ValueRef::Text(t) => {
                let s = std::str::from_utf8(t).map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
                f(s).map(Value::Text).map_err(|e| rusqlite::Error::UserFunctionError(e.into()))
            }
            other => Ok(Value::from(other)),
        }
    }

    // No deterministicas: el cifrado usa un nonce aleatorio
    conn.create_scalar_function("labnas_encrypt", 1, FunctionFlags::SQLITE_UTF8, wrap(encrypt))?;
    conn.create_scalar_function(
        "labnas_decrypt",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        wrap(decrypt),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        let dir = std::env::temp_dir().join(format!("labnas-sec-{}", uuid::Uuid::new_v4()));
        init(&dir).unwrap();
    }

    #[test]
    fn ida_y_vuelta() {
        setup();
        let enc = encrypt("token-secreto").unwrap();
        assert!(enc.starts_with(PREFIX));
        assert!(!enc.contains("token-secreto"));
        assert_ne!(enc, encrypt("token-secreto").unwrap(), "nonce aleatorio");
        assert_eq!(decrypt(&enc).unwrap(), "token-secreto");
    }

    #[test]
    fn compatible_con_texto_plano_y_sin_doble_cifrado() {
        setup();
        assert_eq!(decrypt("viejo-en-claro").unwrap(), "viejo-en-claro");
        let enc = encrypt("x").unwrap();
        assert_eq!(encrypt(&enc).unwrap(), enc);
    }

    #[test]
    fn detecta_alteraciones() {
        setup();
        let enc = encrypt("dato").unwrap();
        let mut blob = B64.decode(&enc[PREFIX.len()..]).unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 1;
        assert!(decrypt(&format!("{}{}", PREFIX, B64.encode(blob))).is_err());
    }

    #[test]
    fn funciones_sql() {
        setup();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        register_sql_functions(&conn).unwrap();
        let out: Option<String> = conn
            .query_row("SELECT labnas_decrypt(labnas_encrypt('abc'))", [], |r| r.get(0))
            .unwrap();
        assert_eq!(out.as_deref(), Some("abc"));
        let null: Option<String> = conn.query_row("SELECT labnas_encrypt(NULL)", [], |r| r.get(0)).unwrap();
        assert_eq!(null, None);
    }
}
