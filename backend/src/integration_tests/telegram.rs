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
    let e = notify_admins_photo(&s.state, jpeg.clone(), "x", &[]).await.unwrap_err();
    assert!(e.contains("no hay bot"), "{}", e);

    // Bot, pero solo un chat que no es de un admin
    exec(&s, "UPDATE notification_config SET bot_token = labnas_encrypt('123:abc') WHERE id = 1;
              INSERT INTO telegram_chats (chat_id, name, role) VALUES (10, 'Visita', 'observador');");
    let e = notify_admins_photo(&s.state, jpeg.clone(), "x", &[]).await.unwrap_err();
    assert!(e.contains("ningun chat de destino"), "{}", e);

    // Una persona autorizada con Telegram: solo recibe si se la incluye
    exec(&s, "INSERT INTO web_users (username, password_hash, role) SELECT 'ope', password_hash, 'operador' FROM web_users WHERE username = 'admin';
              INSERT INTO telegram_chats (chat_id, name, role, linked_web_user) VALUES (7, 'Ope', 'operador', 'ope');");
    assert!(notify_admins_photo(&s.state, jpeg.clone(), "x", &[]).await.is_err());
    assert_eq!(notify_admins_photo(&s.state, jpeg.clone(), "al equipo", &["ope".to_string()]).await, Ok(1));
    assert_eq!(seen.lock().unwrap().pop().unwrap().0, "7");
    exec(&s, "DELETE FROM telegram_chats WHERE chat_id = 7;");

    // Chat vinculado a la cuenta web del admin con el rol viejo: le llega igual
    exec(&s, "INSERT INTO telegram_chats (chat_id, name, role, linked_web_user) VALUES (42, 'Admin', 'operador', 'admin');");
    assert_eq!(notify_admins_photo(&s.state, jpeg.clone(), "Persona detectada en Sala", &[]).await, Ok(1));
    let got = seen.lock().unwrap().clone();
    assert_eq!(got, vec![("42".to_string(), jpeg.len(), "Persona detectada en Sala".to_string())]);

    // Telegram rechaza: se ve su motivo
    exec(&s, "DELETE FROM telegram_chats WHERE chat_id = 42;
              INSERT INTO telegram_chats (chat_id, name, role) VALUES (666, 'Viejo', 'admin');");
    let e = notify_admins_photo(&s.state, jpeg, "x", &[]).await.unwrap_err();
    assert!(e.contains("chat not found"), "{}", e);

    *TEST_TELEGRAM_BASE.lock().unwrap() = None;
}

/// Los recordatorios se arman fuera del runtime async y dejan marcado el aviso
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recordatorio_de_tarea_insistente() {
    let s = Server::start().await;
    let collect = || crate::db::bg(&s.state.db, crate::handlers::notifications::collect_reminders);

    // Sin bot no hay nada que mandar
    assert_eq!(collect().await, Ok(None));

    exec(&s, "UPDATE notification_config SET bot_token = labnas_encrypt('123:abc') WHERE id = 1;
              INSERT INTO telegram_chats (chat_id, name, role) VALUES (5, 'Ana', 'operador');
              INSERT INTO tasks (id, title, assigned_to, created_by, insistent, reminder_minutes, created_at)
              VALUES ('t1', 'Cambiar filamento', '[\"Ana\"]', 'admin', 1, 10, '2026-01-01T00:00:00Z');");

    let (token, msgs) = collect().await.unwrap().expect("debe haber recordatorio");
    assert_eq!(token, "123:abc");
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].0.contains("Cambiar filamento"), "{}", msgs[0].0);
    assert_eq!(msgs[0].1, vec![5]);

    // Quedo marcado: en el siguiente minuto no se repite
    let (_, msgs) = collect().await.unwrap().unwrap();
    assert!(msgs.is_empty());
}

type Sent = Arc<Mutex<Vec<(&'static str, String)>>>;

/// API falsa que anota cada sendPhoto/sendMessage, y una camara que entrega un JPEG
async fn fake_telegram_y_camara() -> (String, Sent) {
    let seen: Sent = Arc::default();
    let (s1, s2) = (seen.clone(), seen.clone());
    let app = Router::new()
        .route(
            "/{bot}/sendPhoto",
            post(move |mut mp: Multipart| {
                let seen = s1.clone();
                async move {
                    while let Ok(Some(f)) = mp.next_field().await {
                        if f.name() == Some("chat_id") {
                            let chat = f.text().await.unwrap();
                            seen.lock().unwrap().push(("foto", chat));
                        }
                    }
                    Json(json!({ "ok": true, "result": {} }))
                }
            }),
        )
        .route(
            "/{bot}/sendMessage",
            post(move |Json(b): Json<serde_json::Value>| {
                let seen = s2.clone();
                async move {
                    seen.lock().unwrap().push(("texto", b["chat_id"].to_string()));
                    Json(json!({ "ok": true, "result": {} }))
                }
            }),
        )
        .route("/cam.jpg", axum::routing::get(|| async { vec![0xFFu8, 0xD8, 1, 2, 0xFF, 0xD9] }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    (format!("http://{}", addr), seen)
}

fn ordenados(seen: &Sent, kind: &str) -> Vec<String> {
    let mut v: Vec<String> = seen.lock().unwrap().iter().filter(|(k, _)| *k == kind).map(|(_, c)| c.clone()).collect();
    v.sort();
    v
}

/// Fin de impresion: foto al personal y a quien encargo la pieza; texto al resto
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fin_de_impresion_con_foto() {
    let s = Server::start().await;
    let _admin = s.admin().await;
    let (base, seen) = fake_telegram_y_camara().await;
    *TEST_TELEGRAM_BASE.lock().unwrap() = Some(base.clone());
    exec(&s, &format!(
        "UPDATE notification_config SET bot_token = labnas_encrypt('123:abc') WHERE id = 1;
         INSERT INTO telegram_chats (chat_id, name, role) VALUES (1, 'Jefa', 'admin'), (2, 'Ope', 'operador'),
                (3, 'Visita', 'observador'), (4, 'Ana', 'observador'), (5, 'Nuevo', 'pendiente');
         UPDATE telegram_chats SET linked_web_user = 'ana' WHERE chat_id = 4;
         INSERT INTO printers3d (id, name, ip, port, printer_type, camera_url) VALUES ('p1', 'Prusa', '127.0.0.1', 1, 'OctoPrint', '{}/cam.jpg');
         INSERT INTO print_queue (id, title, printer_id, requested_by, status, created_at, updated_at)
                VALUES ('q1', 'Soporte', 'p1', 'ana', 'imprimiendo', 'x', 'x');",
        base
    ));
    let printer = crate::handlers::printers3d::list_printers(axum::extract::State(s.state.clone()))
        .await
        .unwrap()
        .0
        .into_iter()
        .find(|p| p.id == "p1")
        .unwrap();

    crate::handlers::printers3d::announce_finished(&s.state, &printer, "soporte.gcode", "1h 5m");
    for _ in 0..50 {
        if seen.lock().unwrap().len() >= 4 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // Ana encargo la pieza: recibe la foto aunque sea observadora; la visita, solo el texto
    assert_eq!(ordenados(&seen, "foto"), vec!["1", "2", "4"]);
    assert_eq!(ordenados(&seen, "texto"), vec!["3"]);

    // Sin foto (impresora sin camara o la camara no responde): texto a todos los activos
    seen.lock().unwrap().clear();
    let n = crate::handlers::notifications::notify_print_finished(&s.state, None, "fin", "fin", &[]).await;
    assert_eq!(n, 4);
    assert_eq!(ordenados(&seen, "texto"), vec!["1", "2", "3", "4"]);
}
