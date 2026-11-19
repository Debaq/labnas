//! Bitacora inalterable (idea 57 de docs/HOJA_DE_RUTA.md).
//!
//! Cada registro queda encadenado al anterior con su huella SHA-256:
//!
//! ```text
//! hash = sha256_hex( JSON compacto de [seq, timestamp, username, kind, subject, annuls, data, prev_hash] )
//! ```
//!
//! - `data` es el texto JSON tal como quedo guardado (se hashea el texto, no se re-serializa).
//! - `annuls` es `null` o el `seq` del registro que se anula.
//! - El primer registro usa `prev_hash` = 64 ceros.
//!
//! Nada se edita ni se borra: triggers de SQLite lo impiden y un error se corrige con
//! un registro nuevo de tipo `anulacion`. Si alguien alterara la base a mano, la cadena
//! deja de cuadrar desde ese registro. Como quien controla el servidor podria recalcular
//! la cadena entera, la huella final se "ancla" fuera (Telegram a los admins, o
//! copiandola en un correo o acta): con eso cualquier reescritura anterior se nota.
//! La receta para verificar sin LabNAS esta en docs/BITACORA.md.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// `prev_hash` del primer registro
pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Tipos de registro que escribe el propio LabNAS (los modulos agregan los suyos)
pub const KIND_NOTE: &str = "nota";
pub const KIND_ANNUL: &str = "anulacion";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Entry {
    pub seq: i64,
    pub timestamp: String,
    pub username: String,
    pub kind: String,
    pub subject: String,
    pub annuls: Option<i64>,
    /// Texto JSON exacto que entra en la huella
    pub data: String,
    pub prev_hash: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VerifyReport {
    pub ok: bool,
    pub count: i64,
    /// Ultimo registro (si la cadena no esta vacia)
    pub head_seq: Option<i64>,
    pub head_hash: String,
    /// Primer registro donde la cadena deja de cuadrar
    pub broken_at: Option<i64>,
    pub reason: Option<String>,
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

/// Huella de un registro (ver la receta arriba)
#[allow(clippy::too_many_arguments)]
pub fn compute_hash(
    seq: i64,
    timestamp: &str,
    username: &str,
    kind: &str,
    subject: &str,
    annuls: Option<i64>,
    data: &str,
    prev_hash: &str,
) -> String {
    let preimage = serde_json::to_string(&(seq, timestamp, username, kind, subject, annuls, data, prev_hash))
        .expect("una tupla de textos y numeros siempre se serializa");
    sha256_hex(preimage.as_bytes())
}

pub(crate) const COLS: &str = "seq, timestamp, username, kind, subject, annuls, data, prev_hash, hash";

pub(crate) fn row_to_entry(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
        seq: r.get(0)?,
        timestamp: r.get(1)?,
        username: r.get(2)?,
        kind: r.get(3)?,
        subject: r.get(4)?,
        annuls: r.get(5)?,
        data: r.get(6)?,
        prev_hash: r.get(7)?,
        hash: r.get(8)?,
    })
}

/// Agrega un registro al final de la cadena. Corre en una transaccion IMMEDIATE:
/// dos escrituras simultaneas no pueden leer el mismo "ultimo registro".
pub fn append(
    conn: &Connection,
    username: &str,
    kind: &str,
    subject: &str,
    annuls: Option<i64>,
    data: &serde_json::Value,
) -> Result<Entry, String> {
    let data = serde_json::to_string(data).map_err(|e| e.to_string())?;
    conn.execute_batch("BEGIN IMMEDIATE").map_err(|e| e.to_string())?;
    let res = append_in_tx(conn, username, kind, subject, annuls, data);
    let end = if res.is_ok() { "COMMIT" } else { "ROLLBACK" };
    conn.execute_batch(end).map_err(|e| e.to_string())?;
    res
}

fn append_in_tx(
    conn: &Connection,
    username: &str,
    kind: &str,
    subject: &str,
    annuls: Option<i64>,
    data: String,
) -> Result<Entry, String> {
    let last: Option<(i64, String)> = conn
        .query_row("SELECT seq, hash FROM ledger_entries ORDER BY seq DESC LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let (seq, prev_hash) = match last {
        Some((s, h)) => (s + 1, h),
        None => (1, GENESIS.to_string()),
    };
    let timestamp = chrono::Utc::now().to_rfc3339();
    let hash = compute_hash(seq, &timestamp, username, kind, subject, annuls, &data, &prev_hash);
    conn.execute(
        "INSERT INTO ledger_entries (seq, timestamp, username, kind, subject, annuls, data, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![seq, timestamp, username, kind, subject, annuls, data, prev_hash, hash],
    )
    .map_err(|e| e.to_string())?;
    Ok(Entry {
        seq,
        timestamp,
        username: username.to_string(),
        kind: kind.to_string(),
        subject: subject.to_string(),
        annuls,
        data,
        prev_hash,
        hash,
    })
}

/// Recorre la cadena completa y recalcula cada huella
pub fn verify(conn: &Connection) -> Result<VerifyReport, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {} FROM ledger_entries ORDER BY seq ASC", COLS))
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut expected_seq = 1i64;
    let mut prev = GENESIS.to_string();
    let mut count = 0i64;
    let mut head_seq = None;
    let broken = |seq: i64, reason: String, count: i64, head_seq: Option<i64>, prev: String| VerifyReport {
        ok: false,
        count,
        head_seq,
        head_hash: prev,
        broken_at: Some(seq),
        reason: Some(reason),
    };

    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let e = row_to_entry(row).map_err(|e| e.to_string())?;
        if e.seq != expected_seq {
            let reason = format!("falta el registro {} (sigue el {})", expected_seq, e.seq);
            return Ok(broken(expected_seq, reason, count, head_seq, prev));
        }
        if e.prev_hash != prev {
            return Ok(broken(e.seq, "no apunta a la huella del registro anterior".into(), count, head_seq, prev));
        }
        let h = compute_hash(e.seq, &e.timestamp, &e.username, &e.kind, &e.subject, e.annuls, &e.data, &e.prev_hash);
        if h != e.hash {
            return Ok(broken(e.seq, "el contenido no corresponde a su huella (fue modificado)".into(), count, head_seq, prev));
        }
        prev = e.hash;
        head_seq = Some(e.seq);
        expected_seq += 1;
        count += 1;
    }
    Ok(VerifyReport { ok: true, count, head_seq, head_hash: prev, broken_at: None, reason: None })
}

pub fn get(conn: &Connection, seq: i64) -> Result<Option<Entry>, String> {
    conn.query_row(&format!("SELECT {} FROM ledger_entries WHERE seq = ?1", COLS), [seq], row_to_entry)
        .optional()
        .map_err(|e| e.to_string())
}

/// `seq` del registro que anula a `seq`, si existe
pub fn annulled_by(conn: &Connection, seq: i64) -> Result<Option<i64>, String> {
    conn.query_row("SELECT seq FROM ledger_entries WHERE annuls = ?1 ORDER BY seq LIMIT 1", [seq], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::MIGRATION_LEDGER).unwrap();
        conn
    }

    #[test]
    fn cadena_valida_y_detecta_alteraciones() {
        let conn = db();
        let empty = verify(&conn).unwrap();
        assert!(empty.ok);
        assert_eq!((empty.count, empty.head_hash.as_str()), (0, GENESIS));

        let a = append(&conn, "ana", KIND_NOTE, "", None, &json!({ "title": "Compra ñandú", "text": "línea\n\"dos\"" })).unwrap();
        let b = append(&conn, "beto", KIND_ANNUL, "", Some(1), &json!({ "reason": "error" })).unwrap();
        assert_eq!((a.seq, a.prev_hash.as_str()), (1, GENESIS));
        assert_eq!((b.seq, &b.prev_hash), (2, &a.hash));
        let rep = verify(&conn).unwrap();
        assert!(rep.ok, "{:?}", rep);
        assert_eq!((rep.count, rep.head_seq, rep.head_hash.clone()), (2, Some(2), b.hash.clone()));
        assert_eq!(annulled_by(&conn, 1).unwrap(), Some(2));

        // La base no deja editar ni borrar
        assert!(conn.execute("UPDATE ledger_entries SET data = '{}' WHERE seq = 1", []).is_err());
        assert!(conn.execute("DELETE FROM ledger_entries WHERE seq = 2", []).is_err());
        // Una bifurcacion (dos registros con el mismo anterior) tampoco entra
        assert!(conn
            .execute(
                "INSERT INTO ledger_entries (seq, timestamp, username, kind, subject, annuls, data, prev_hash, hash)
                 VALUES (3, 't', 'x', 'nota', '', NULL, '{}', ?1, 'h')",
                [&a.hash],
            )
            .is_err());

        // Alguien con acceso al archivo quita las protecciones y cambia un dato: se nota
        conn.execute_batch("DROP TRIGGER ledger_no_update; DROP TRIGGER ledger_no_delete;").unwrap();
        conn.execute("UPDATE ledger_entries SET username = 'otro' WHERE seq = 1", []).unwrap();
        let rep = verify(&conn).unwrap();
        assert!(!rep.ok);
        assert_eq!((rep.broken_at, rep.count), (Some(1), 0));

        // ...y si recalcula esa huella, el siguiente ya no apunta a ella
        let e = get(&conn, 1).unwrap().unwrap();
        let h = compute_hash(1, &e.timestamp, "otro", &e.kind, &e.subject, e.annuls, &e.data, &e.prev_hash);
        conn.execute("UPDATE ledger_entries SET hash = ?1 WHERE seq = 1", [h]).unwrap();
        assert_eq!(verify(&conn).unwrap().broken_at, Some(2));

        // Un hueco tambien se nota
        conn.execute("DELETE FROM ledger_entries WHERE seq = 1", []).unwrap();
        let rep = verify(&conn).unwrap();
        assert_eq!((rep.ok, rep.broken_at), (false, Some(1)), "hueco al inicio");
    }

    #[test]
    fn cortar_el_final_solo_lo_delata_el_ancla() {
        let conn = db();
        append(&conn, "ana", KIND_NOTE, "", None, &json!({})).unwrap();
        let ancla = append(&conn, "ana", KIND_NOTE, "", None, &json!({})).unwrap();
        conn.execute_batch("DROP TRIGGER ledger_no_delete;").unwrap();
        conn.execute("DELETE FROM ledger_entries WHERE seq = 2", []).unwrap();
        let rep = verify(&conn).unwrap();
        // La cadena que queda cuadra, pero ya no termina en la huella anclada
        assert!(rep.ok);
        assert_ne!(rep.head_hash, ancla.hash);
    }

    #[test]
    fn receta_de_la_huella_es_estable() {
        // Si esto cambia, las cadenas ya guardadas dejan de verificar: no tocar
        let h = compute_hash(1, "2026-11-19T12:00:00+00:00", "ana", "nota", "", None, "{\"a\":\"ñ\"}", GENESIS);
        let pre = r#"[1,"2026-11-19T12:00:00+00:00","ana","nota","",null,"{\"a\":\"ñ\"}","0000000000000000000000000000000000000000000000000000000000000000"]"#;
        assert_eq!(h, sha256_hex(pre.as_bytes()));
        // Mismo resultado que el verificador en Python de docs/BITACORA.md
        // (json.dumps(..., ensure_ascii=False, separators=(",", ":")))
        let h = compute_hash(1, "2026-11-19T12:00:00+00:00", "ana", "nota", "", None, "ñ\u{1}\n\t\"/\u{7f}", GENESIS);
        assert_eq!(h, "1a0b00f13c695a2ddc430e0446a52f192c24c71f3caf37e0ddccab7573e419d8");
    }
}
