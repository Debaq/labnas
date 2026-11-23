use chrono::{Duration, Local};
use serde_json::json;

use super::harness::Server;

/// `YYYY-MM-DD` dentro de `n` dias
fn in_days(n: i64) -> String {
    (Local::now().date_naive() + Duration::days(n)).format("%Y-%m-%d").to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plazos_contadores_y_portafolio() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let ana = s.user(&admin, "ana", "observador").await;
    let eva = s.user(&admin, "eva", "observador").await;
    let op = s.user(&admin, "op", "operador").await;

    // validacion
    assert_eq!(s.post("/api/deadlines", &ana, json!({ "title": "", "due_date": in_days(3) })).await.status, 400);
    assert_eq!(s.post("/api/deadlines", &ana, json!({ "title": "X", "due_date": "mañana" })).await.status, 400);
    assert_eq!(s.post("/api/deadlines", &ana, json!({ "title": "X", "due_date": in_days(3), "category": "fiesta" })).await.status, 400);
    assert_eq!(s.post("/api/deadlines", &ana, json!({ "title": "X", "due_date": in_days(3), "portfolio_id": "nope" })).await.status, 400);

    // contador suelto: dias que faltan y color
    let r = s.post("/api/deadlines", &ana, json!({ "title": "Informe FIC", "category": "informe", "due_date": in_days(5) })).await;
    assert_eq!(r.status, 201);
    let informe = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(r.json()["days_left"], 5);
    assert_eq!(r.json()["level"], "urgente");
    assert_eq!(r.json()["notify"], "creador");

    // proyecto propio en curso: cierre + hito pendiente; el cumplido y los externos no salen
    let proyecto = json!({
        "id": "x", "created_at": "2026-01-01T00:00:00Z", "title": "Fondef Prótesis", "scope": "own", "status": "active",
        "end_date": in_days(40), "participants": ["eva"],
        "milestones": [
            { "id": "h1", "title": "Prototipo", "date": in_days(20), "completed": false },
            { "id": "h2", "title": "Compra", "date": in_days(2), "completed": true }
        ]
    });
    let pid = s.post("/api/portfolio", &op, proyecto).await.json()["id"].as_str().unwrap().to_string();
    let externo = json!({ "id": "x", "created_at": "2026-01-01T00:00:00Z", "title": "Ajeno", "scope": "external", "status": "active", "end_date": in_days(10) });
    assert_eq!(s.post("/api/portfolio", &op, externo).await.status, 200);

    let list = s.get("/api/deadlines", &eva).await.json();
    let items = list.as_array().unwrap();
    let keys: Vec<&str> = items.iter().map(|d| d["key"].as_str().unwrap()).collect();
    assert_eq!(keys, vec![format!("d:{}", informe), format!("m:{}:h1", pid), format!("p:{}", pid)], "ordenado por fecha");
    assert_eq!(items[1]["source"], "hito");
    assert_eq!(items[1]["project"], "Fondef Prótesis");
    assert_eq!(items[1]["level"], "pronto");
    assert_eq!(items[2]["title"], "Cierre de Fondef Prótesis");
    assert_eq!(items[2]["level"], "ok");

    // vinculado a un proyecto
    let r = s.post("/api/deadlines", &eva, json!({ "title": "Rendición", "category": "rendicion", "due_date": in_days(1), "portfolio_id": pid, "notify": "staff" })).await;
    assert_eq!(r.json()["project"], "Fondef Prótesis");
    let rendicion = r.json()["id"].as_str().unwrap().to_string();

    // editar / cumplir / borrar: quien lo creo o staff
    let url = format!("/api/deadlines/{}", informe);
    let editado = json!({ "title": "Informe FIC final", "category": "informe", "due_date": in_days(9) });
    assert_eq!(s.put(&url, &eva, editado.clone()).await.status, 403);
    let r = s.put(&url, &ana, editado).await;
    assert_eq!(r.json()["days_left"], 9);
    assert_eq!(r.json()["level"], "pronto");
    assert_eq!(s.post(&format!("{}/done", url), &eva, json!({ "done": true })).await.status, 403);
    let r = s.post(&format!("{}/done", url), &op, json!({ "done": true })).await;
    assert_eq!(r.json()["level"], "cumplido");
    assert_eq!(r.json()["done_by"], "op");
    let activos = s.get("/api/deadlines", &ana).await.json();
    assert!(!activos.as_array().unwrap().iter().any(|d| d["key"] == format!("d:{}", informe)));
    let hechos = s.get("/api/deadlines?done=true", &ana).await.json();
    assert_eq!(hechos.as_array().unwrap().len(), 1);
    assert_eq!(s.post(&format!("{}/done", url), &ana, json!({ "done": false })).await.json()["level"], "pronto");

    // avisos: cada umbral una sola vez, y a los destinatarios correctos
    let alerts = crate::db::db_op(&s.state.db, |c| crate::handlers::deadlines::pending_alerts(c, Local::now().date_naive()).map_err(|e| e.to_string())).await.unwrap();
    let mut got: Vec<(String, Option<Vec<String>>)> = alerts.into_iter().map(|a| (a.deadline.title, a.to)).collect();
    got.sort();
    let s3 = |v: &[&str]| Some(v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
    assert_eq!(got, vec![
        ("Informe FIC final".into(), s3(&["ana"])),
        ("Prototipo".into(), s3(&["admin", "eva", "op"])),
        ("Rendición".into(), s3(&["admin", "eva", "op"])),
    ], "el cierre a 40 dias todavia no avisa");
    let again = crate::db::db_op(&s.state.db, |c| crate::handlers::deadlines::pending_alerts(c, Local::now().date_naive()).map_err(|e| e.to_string())).await.unwrap();
    assert!(again.is_empty(), "no repite");
    // al dia siguiente la rendicion vence hoy: nuevo aviso
    let next = crate::db::db_op(&s.state.db, |c| crate::handlers::deadlines::pending_alerts(c, Local::now().date_naive() + Duration::days(1)).map_err(|e| e.to_string())).await.unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].deadline.days_left, 0);

    assert_eq!(s.delete(&format!("/api/deadlines/{}", rendicion), &ana).await.status, 403);
    assert_eq!(s.delete(&format!("/api/deadlines/{}", rendicion), &eva).await.status, 204);
    assert_eq!(s.delete(&format!("/api/deadlines/{}", rendicion), &eva).await.status, 404);

    // cuenta sin aprobar y modulo apagado
    let pend = s.register("pepe", "pepe-password").await.json()["token"].as_str().unwrap().to_string();
    assert_eq!(s.get("/api/deadlines", &pend).await.status, 403);
    assert_eq!(s.put("/api/modules/deadlines", &admin, json!({ "enabled": false })).await.status, 200);
    assert_eq!(s.get("/api/deadlines", &ana).await.status, 404);
}
