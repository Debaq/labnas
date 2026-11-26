//! Registro de uso real de maquinas (handlers::machine_usage)

use chrono::{Duration, Local};
use serde_json::json;

use super::harness::Server;

fn at(offset_min: i64) -> String {
    (Local::now().naive_local() + Duration::minutes(offset_min)).format("%Y-%m-%dT%H:%M").to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sesiones_de_uso() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let ana = s.user(&admin, "ana", "observador").await;
    let eva = s.user(&admin, "eva", "observador").await;

    let r = s.post("/api/machines", &op, json!({ "name": "Laser CO2", "kind": "laser", "requires_certification": true })).await;
    assert_eq!(r.status, 201);
    let m = r.json()["id"].as_str().unwrap().to_string();
    let base = format!("/api/machines/{}", m);

    // Sin induccion: ana no, el staff si
    assert_eq!(s.post(&format!("{base}/usage/start"), &ana, json!({})).await.status, 403);
    let g = s.post(&format!("{base}/certifications"), &op, json!({ "username": "ana" })).await;
    assert_eq!(g.status, 200, "{}", g.text);

    // Reserva en curso de ana: la sesion se liga a ella
    let b = s.post(&format!("{base}/bookings"), &ana, json!({ "start": at(-2), "end": at(60) })).await;
    assert_eq!(b.status, 201, "{}", b.text);
    let bid = b.json()["id"].as_str().unwrap().to_string();

    let st = s.post(&format!("{base}/usage/start"), &ana, json!({ "project": "Tesis" })).await;
    assert_eq!(st.status, 201, "{}", st.text);
    let sid = st.json()["id"].as_str().unwrap().to_string();
    assert_eq!(st.json()["booking_id"], bid.as_str());

    // Una sola abierta; la ficha muestra quien la usa
    let dup = s.post(&format!("{base}/usage/start"), &op, json!({})).await;
    assert_eq!(dup.status, 409);
    assert_eq!(s.get(&base, &eva).await.json()["in_use"]["username"], "ana");
    assert_eq!(s.get(&format!("{base}/usage"), &eva).await.json()["open"]["id"], sid.as_str());

    // Terminar: otra persona no, el staff si; ana tambien
    assert_eq!(s.post(&format!("/api/machines/usage/{sid}/stop"), &eva, json!({})).await.status, 403);
    let stop = s.post(&format!("/api/machines/usage/{sid}/stop"), &ana, json!({ "note": "salio bien", "project": "Tesis 2" })).await;
    assert_eq!(stop.status, 200, "{}", stop.text);
    assert_eq!(stop.json()["project"], "Tesis 2");
    assert!(stop.json()["ended_at"].is_string());
    assert_eq!(s.post(&format!("/api/machines/usage/{sid}/stop"), &ana, json!({})).await.status, 409);
    assert!(s.get(&base, &eva).await.json()["in_use"].is_null());

    // Informe: solo staff
    assert_eq!(s.get("/api/machines/usage/report", &ana).await.status, 403);
    let rep = s.get("/api/machines/usage/report", &op).await;
    assert_eq!(rep.status, 200, "{}", rep.text);
    assert_eq!(rep.json()["sessions"], 1);
    assert_eq!(rep.json()["by_user"][0]["key"], "ana");
    assert_eq!(rep.json()["by_project"][0]["key"], "Tesis 2");

    // Fuera de servicio: no se empieza
    let f = s.post(&format!("{base}/faults"), &ana, json!({ "description": "no enciende" })).await;
    assert_eq!(f.status, 201, "{}", f.text);
    assert_eq!(s.post(&format!("{base}/usage/start"), &op, json!({})).await.status, 409);

    // El horometro sigue funcionando (PUT en la misma ruta) y no se cuenta doble
    let u = s.put(&format!("{base}/usage"), &op, json!({ "hours": 50 })).await;
    assert_eq!(u.status, 200);
    assert_eq!(u.json()["usage_hours"], 50.0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn horas_de_mantencion_usan_sesiones_sin_contar_doble() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let op = s.user(&admin, "op", "operador").await;
    let m = s.post("/api/machines", &op, json!({ "name": "CNC", "kind": "cnc" })).await.json()["id"].as_str().unwrap().to_string();
    let base = format!("/api/machines/{}", m);

    // Sesion terminada de 3 h metida directo a la base: 1 sesion ligada a una reserva que ya paso
    // (la reserva no se suma aparte) y 1 sesion suelta
    let ov = s.get(&format!("{base}/maintenance"), &op).await.json();
    assert_eq!(ov["usage_hours"], 0.0);
    crate::db::db_op(&s.state.db, move |conn| {
        let (a, b, c, d) = (at(-300), at(-120), at(-100), at(-60));
        conn.execute(
            "INSERT INTO machine_bookings (id, machine_id, username, kind, start, end, purpose, status, created_at)
             VALUES ('bk1', ?1, 'op', 'reserva', ?2, ?3, '', 'confirmada', 'x')",
            rusqlite::params![m, a, b],
        )
        .unwrap();
        conn.execute("INSERT INTO machine_usage (id, machine_id, username, booking_id, started_at, ended_at) VALUES ('u1', ?1, 'op', 'bk1', ?2, ?3)", rusqlite::params![m, a, at(-240)]).unwrap();
        conn.execute("INSERT INTO machine_usage (id, machine_id, username, started_at, ended_at) VALUES ('u2', ?1, 'op', ?2, ?3)", rusqlite::params![m, c, d]).unwrap();
        Ok(())
    })
    .await
    .unwrap();
    // 1 h (sesion ligada, no la reserva de 3 h) + 0,7 h (sesion suelta) = 1,7
    let ov = s.get(&format!("{base}/maintenance"), &op).await.json();
    assert_eq!(ov["usage_hours"], 1.7);
}
