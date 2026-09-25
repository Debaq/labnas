use serde_json::json;

use super::harness::Server;

/// Lectura cruda de la base (sin las funciones de descifrado)
fn raw(s: &Server, sql: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(s.home.join(".labnas/labnas.db")).unwrap();
    conn.query_row(sql, [], |r| r.get(0)).ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn secretos_cifrados_en_la_base() {
    let s = Server::start().await;
    let admin = s.admin().await;

    // API key de Groq (setting)
    assert_eq!(s.post("/api/email/groq-key", &admin, json!({ "key": "gsk_supersecreta" })).await.status, 200);
    let stored = raw(&s, "SELECT value FROM settings WHERE key = 'groq_api_key'").unwrap();
    assert!(stored.starts_with("enc:v1:"), "{}", stored);
    assert!(!stored.contains("gsk_supersecreta"));
    let conn = s.state.db.get().unwrap();
    assert_eq!(crate::db::get_secret_setting(&conn, "groq_api_key").as_deref(), Some("gsk_supersecreta"));
    drop(conn);

    // API key de una impresora 3D: cifrada en la base, en claro para la API
    let r = s.post("/api/printers3d", &admin, json!({
        "name": "Prusa", "ip": "10.0.0.5", "port": 7125, "printer_type": "Moonraker", "api_key": "clave-impresora"
    })).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let stored = raw(&s, "SELECT api_key FROM printers3d").unwrap();
    assert!(stored.starts_with("enc:v1:"));
    let list = s.get("/api/printers3d", &admin).await.json();
    assert_eq!(list[0]["api_key"], "clave-impresora");

    // La clave vive fuera de la base, solo legible por el dueño
    use std::os::unix::fs::PermissionsExt;
    let key = s.home.join(".labnas/secret.key");
    assert_eq!(std::fs::metadata(&key).unwrap().permissions().mode() & 0o777, 0o600);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn migracion_cifra_secretos_en_claro() {
    let s = Server::start().await;
    s.admin().await;

    // Simular una base anterior: secreto en claro y migraciones hasta la 4
    {
        let conn = rusqlite::Connection::open(s.home.join(".labnas/labnas.db")).unwrap();
        conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('lastfm_api_key', 'lastfm-en-claro')", []).unwrap();
        conn.execute("UPDATE notification_config SET bot_token = '123:token-en-claro' WHERE id = 1", []).unwrap();
        conn.pragma_update(None, "user_version", 4).unwrap();
    }

    let s = s.restart().await;
    let lastfm = raw(&s, "SELECT value FROM settings WHERE key = 'lastfm_api_key'").unwrap();
    assert!(lastfm.starts_with("enc:v1:"), "{}", lastfm);
    let token = raw(&s, "SELECT bot_token FROM notification_config WHERE id = 1").unwrap();
    assert!(token.starts_with("enc:v1:"), "{}", token);

    let conn = s.state.db.get().unwrap();
    assert_eq!(crate::db::get_secret_setting(&conn, "lastfm_api_key").as_deref(), Some("lastfm-en-claro"));
    let decrypted: String = conn
        .query_row("SELECT labnas_decrypt(bot_token) FROM notification_config WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(decrypted, "123:token-en-claro");
}
