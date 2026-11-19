//! Envio de fotos a los admins por Telegram contra un servidor falso de la API

use axum::{extract::Multipart, routing::post, Json, Router};
use serde_json::json;
use std::sync::{Arc, Mutex};

use super::harness::Server;
use crate::handlers::notifications::{notify_admins_photo, telegram::TEST_TELEGRAM_BASE};

type Seen = Arc<Mutex<Vec<(String, usize, String)>>>;

/// API falsa: acepta sendPhoto (multipart) salvo para el chat 666
async fn fake_telegram() -> (String, Seen) {
    let seen: Seen = Arc::default();
    let s2 = seen.clone();
    let app = Router::new().route(
        "/{bot}/sendPhoto",
        post(move |mut mp: Multipart| {
            let seen = s2.clone();
            async move {
                let (mut chat, mut photo, mut caption) = (String::new(), 0, String::new());
                while let Ok(Some(f)) = mp.next_field().await {
                    match f.name().unwrap_or("").to_string().as_str() {
                        "chat_id" => chat = f.text().await.unwrap(),
                        "caption" => caption = f.text().await.unwrap(),
                        "photo" => photo = f.bytes().await.unwrap().len(),
                        _ => {}
                    }
                }
                if chat == "666" {
                    return (axum::http::StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "description": "Bad Request: chat not found" })));
                }
                seen.lock().unwrap().push((chat, photo, caption));
                (axum::http::StatusCode::OK, Json(json!({ "ok": true, "result": {} })))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    (format!("http://{}", addr), seen)
}

fn exec(s: &Server, sql: &str) {
    s.state.db.get().unwrap().execute_batch(sql).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fotos_a_los_admins() {
    let s = Server::start().await;
    let _admin = s.admin().await;
    let (base, seen) = fake_telegram().await;
    *TEST_TELEGRAM_BASE.lock().unwrap() = Some(base);
    let jpeg = vec![0xFFu8, 0xD8, 1, 2, 3, 0xFF, 0xD9];

    // Sin bot: el error lo dice
    let e = notify_admins_photo(&s.state, jpeg.clone(), "x", false).await.unwrap_err();
    assert!(e.contains("no hay bot"), "{}", e);

    // Bot, pero solo un chat que no es de un admin
    exec(&s, "UPDATE notification_config SET bot_token = labnas_encrypt('123:abc') WHERE id = 1;
              INSERT INTO telegram_chats (chat_id, name, role) VALUES (10, 'Visita', 'observador');");
    let e = notify_admins_photo(&s.state, jpeg.clone(), "x", false).await.unwrap_err();
    assert!(e.contains("ningun chat de administrador"), "{}", e);

    // Un operador con Telegram: solo recibe si la regla avisa tambien a operadores
    exec(&s, "INSERT INTO web_users (username, password_hash, role) SELECT 'ope', password_hash, 'operador' FROM web_users WHERE username = 'admin';
              INSERT INTO telegram_chats (chat_id, name, role, linked_web_user) VALUES (7, 'Ope', 'operador', 'ope');");
    assert!(notify_admins_photo(&s.state, jpeg.clone(), "x", false).await.is_err());
    assert_eq!(notify_admins_photo(&s.state, jpeg.clone(), "al equipo", true).await, Ok(1));
    assert_eq!(seen.lock().unwrap().pop().unwrap().0, "7");
    exec(&s, "DELETE FROM telegram_chats WHERE chat_id = 7;");

    // Chat vinculado a la cuenta web del admin con el rol viejo: le llega igual
    exec(&s, "INSERT INTO telegram_chats (chat_id, name, role, linked_web_user) VALUES (42, 'Admin', 'operador', 'admin');");
    assert_eq!(notify_admins_photo(&s.state, jpeg.clone(), "Persona detectada en Sala", false).await, Ok(1));
    let got = seen.lock().unwrap().clone();
    assert_eq!(got, vec![("42".to_string(), jpeg.len(), "Persona detectada en Sala".to_string())]);

    // Telegram rechaza: se ve su motivo
    exec(&s, "DELETE FROM telegram_chats WHERE chat_id = 42;
              INSERT INTO telegram_chats (chat_id, name, role) VALUES (666, 'Viejo', 'admin');");
    let e = notify_admins_photo(&s.state, jpeg, "x", false).await.unwrap_err();
    assert!(e.contains("chat not found"), "{}", e);

    *TEST_TELEGRAM_BASE.lock().unwrap() = None;
}
