//! Finanzas: presupuesto por item, ejecucion, avisos, bienes por proyecto, caja chica con
//! arqueo, ingresos y permisos (handlers::finance::budget)

use serde_json::json;

use super::harness::Server;

// PNG minimo valido (solo importa la firma)
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn marca(n: &str) -> String {
    format!("data:image/png;base64,{}{}", PNG, n)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn presupuesto_ejecucion_y_permisos() {
    let s = Server::start().await;
    let admin = s.admin().await;
    let adm = s.user(&admin, "adm", "administrativo").await; // gestiona Finanzas
    let doc = s.user(&admin, "doc", "docente").await; // usa Finanzas, no gestiona
    let est = s.user(&admin, "est", "estudiante").await; // sin acceso

    let proyecto = json!({
        "id": "x", "created_at": "2026-01-01T00:00:00Z", "title": "Fondef Demo", "scope": "own", "status": "active",
        "end_date": "2030-01-01", "participants": [], "milestones": []
    });
    let pid = s.post("/api/portfolio", &admin, proyecto).await.json()["id"].as_str().unwrap().to_string();

    // Permisos: el estudiante no entra; el docente ve pero no gestiona
    assert_eq!(s.get("/api/finance/execution", &est).await.status, 403);
    assert_eq!(s.get(&format!("/api/finance/execution/{}", pid), &doc).await.status, 200);
    let item = json!({ "project_id": pid, "category": "equipos", "name": "Cortadora laser", "approved": 1000, "notes": "" });
    assert_eq!(s.post("/api/finance/budget/items", &doc, item.clone()).await.status, 403);
    assert_eq!(s.post("/api/finance/budget/items", &est, item.clone()).await.status, 403);
    assert_eq!(
        s.put(&format!("/api/finance/projects/{}/config", pid), &doc, json!({ "responsible": "doc" })).await.status,
        403
    );

    // Validaciones
    let mut malo = item.clone();
    malo["category"] = json!("lujos");
    assert_eq!(s.post("/api/finance/budget/items", &adm, malo).await.status, 400);
    let mut malo = item.clone();
    malo["approved"] = json!(-5);
    assert_eq!(s.post("/api/finance/budget/items", &adm, malo).await.status, 400);
    let mut malo = item.clone();
    malo["project_id"] = json!("nope");
    assert_eq!(s.post("/api/finance/budget/items", &adm, malo).await.status, 404);

    // Configuracion del proyecto
    let cfg = format!("/api/finance/projects/{}/config", pid);
    assert_eq!(s.get(&cfg, &doc).await.json()["alert_pct"], 80.0);
    assert_eq!(s.put(&cfg, &adm, json!({ "responsible": "fantasma" })).await.status, 400);
    assert_eq!(s.put(&cfg, &adm, json!({ "responsible": "doc", "alert_pct": 150 })).await.status, 400);
    let r = s.put(&cfg, &adm, json!({ "responsible": "doc", "alert_pct": 70 })).await;
    assert_eq!(r.status, 200, "{}", r.text);

    // Crear el item y cambiar su monto: exige motivo y deja antes/despues
    let r = s.post("/api/finance/budget/items", &adm, item.clone()).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let iid = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.post("/api/finance/budget/items", &adm, item.clone()).await.status, 409, "nombre repetido");
    let url = format!("/api/finance/budget/items/{}", iid);
    let mut cambio = json!({ "category": "equipos", "name": "Cortadora laser", "approved": 2000, "notes": "" });
    assert_eq!(s.put(&url, &adm, cambio.clone()).await.status, 400, "sin motivo");
    cambio["reason"] = json!("Ampliacion aprobada en acta 3");
    assert_eq!(s.put(&url, &adm, cambio.clone()).await.status, 200);
    let det = s.get(&format!("{}/detail", url), &doc).await.json();
    assert_eq!(det["changes"].as_array().unwrap().len(), 1);
    assert_eq!((det["changes"][0]["old_amount"].as_f64(), det["changes"][0]["new_amount"].as_f64()), (Some(1000.0), Some(2000.0)));
    assert!(det["changes"][0]["ledger_seq"].is_i64());
    let led = s.get("/api/ledger", &admin).await.text;
    assert!(led.contains("monto_modificado") && led.contains("Ampliacion aprobada"), "{}", led);

    // Ejecucion: un compromiso (compra aprobada) y un gasto
    let (p2, i2) = (pid.clone(), iid.clone());
    crate::db::db_op(&s.state.db, move |conn| {
        crate::finance::set_commitment(conn, "compra", "c1", &p2, Some(&i2), "Compra laser", 1500.0, 0.0).map_err(|e| e.to_string())
    })
    .await
    .unwrap();
    let ex = s.get(&format!("/api/finance/execution/{}", pid), &doc).await.json();
    let it = &ex["items"][0];
    assert_eq!((it["committed"].as_f64(), it["spent"].as_f64(), it["available"].as_f64()), (Some(1500.0), Some(0.0), Some(500.0)));
    assert_eq!(it["pct"].as_f64(), Some(75.0));
    assert_eq!(it["level"], "alerta", "75% cruza el 70% del proyecto");
    assert_eq!(ex["totals"]["approved"].as_f64(), Some(2000.0));
    let resumen = s.get("/api/finance/execution", &doc).await.json();
    assert_eq!(resumen[0]["level"], "alerta");

    // Umbrales: el aviso queda marcado una sola vez
    crate::handlers::finance::budget::run_alerts(&s.state).await;
    crate::handlers::finance::budget::run_alerts(&s.state).await;
    let marcas: i64 = crate::db::db_op(&s.state.db, |c| {
        c.query_row("SELECT COUNT(*) FROM fin_budget_alerts", [], |r| r.get(0)).map_err(|e| e.to_string())
    })
    .await
    .unwrap();
    assert_eq!(marcas, 1);

    // No se baja el aprobado bajo lo comprometido, ni se borra un item con compromisos
    let mut bajo = cambio.clone();
    bajo["approved"] = json!(1000);
    assert_eq!(s.put(&url, &adm, bajo).await.status, 409);
    assert_eq!(s.delete(&url, &adm).await.status, 409);
    assert_eq!(s.delete(&url, &doc).await.status, 403);

    // Un item sin movimientos si se borra
    let r = s.post("/api/finance/budget/items", &adm, json!({ "project_id": pid, "category": "otro", "name": "Varios", "approved": 100 })).await;
    let vid = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.delete(&format!("/api/finance/budget/items/{}", vid), &adm).await.status, 204);

    // ---- Bienes por proyecto ----
    let cat = s.post("/api/inventory/categories", &admin, json!({ "id": "", "name": "Equipos" })).await.json()["id"].as_str().unwrap().to_string();
    let bien = json!({ "id": "", "category_id": cat, "name": "Cortadora", "unit": "unidad", "quantity": 2, "cost": 500,
        "project_id": pid, "inventory_number": "UACH-001", "responsible": "Ana" });
    let r = s.post("/api/inventory/items", &admin, bien.clone()).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["inventory_number"], "UACH-001");
    assert_eq!(s.post("/api/inventory/items", &admin, bien.clone()).await.status, 409, "numero repetido");
    let mut huerfano = bien.clone();
    huerfano["inventory_number"] = json!("UACH-002");
    huerfano["project_id"] = json!("nope");
    assert_eq!(s.post("/api/inventory/items", &admin, huerfano).await.status, 400);
    let a = s.get(&format!("/api/finance/assets?project_id={}", pid), &doc).await.json();
    assert_eq!(a["items"].as_array().unwrap().len(), 1);
    assert_eq!(a["total_cost"].as_f64(), Some(1000.0));

    // ---- Caja chica con arqueo ----
    let fondo = json!({ "project_id": pid, "name": "Caja del lab", "responsible": "doc", "arqueo_days": 30 });
    assert_eq!(s.post("/api/finance/petty/funds", &doc, fondo.clone()).await.status, 403);
    let r = s.post("/api/finance/petty/funds", &adm, fondo.clone()).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let fid = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.post("/api/finance/petty/funds", &adm, fondo).await.status, 409, "un fondo por proyecto");
    let moves = format!("/api/finance/petty/funds/{}/moves", fid);

    // Reponer el fondo: solo quien gestiona
    let ing = json!({ "kind": "ingreso", "amount": 50000, "concept": "Fondo inicial" });
    assert_eq!(s.post(&moves, &doc, ing.clone()).await.status, 403);
    assert_eq!(s.post(&moves, &est, ing.clone()).await.status, 403);
    assert_eq!(s.post(&moves, &adm, ing).await.status, 201);

    // Egreso: respaldo e item obligatorios; lo registra el responsable
    let egr = |monto: i64| json!({ "kind": "egreso", "amount": monto, "concept": "Pegamento", "budget_item_id": iid, "file": marca(""), "file_name": "boleta.png" });
    let mut sin = egr(10000);
    sin.as_object_mut().unwrap().remove("file");
    assert_eq!(s.post(&moves, &doc, sin).await.status, 400);
    assert_eq!(s.post(&moves, &doc, egr(80000)).await.status, 409, "saldo insuficiente");
    let r = s.post(&moves, &doc, egr(10000)).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let mid = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(r.json()["file"]["sha256"].as_str().map(str::len), Some(64));
    let ex = s.get(&format!("/api/finance/execution/{}", pid), &doc).await.json();
    assert_eq!(ex["items"][0]["spent"].as_f64(), Some(10000.0), "el egreso cuenta como gastado");

    // Arqueo: la diferencia se explica y no lo hace quien custodia
    let cnt = format!("/api/finance/petty/funds/{}/counts", fid);
    assert_eq!(s.post(&cnt, &doc, json!({ "counted": 40000 })).await.status, 403);
    assert_eq!(s.post(&cnt, &adm, json!({ "counted": 38000 })).await.status, 400, "diferencia sin explicar");
    let r = s.post(&cnt, &adm, json!({ "counted": 38000, "notes": "Faltan dos boletas" })).await;
    assert_eq!(r.status, 201, "{}", r.text);
    assert_eq!((r.json()["expected"].as_f64(), r.json()["difference"].as_f64()), (Some(40000.0), Some(-2000.0)));
    assert_eq!(r.json()["counted_by"], "adm");
    let det = s.get(&format!("/api/finance/petty/funds/{}", fid), &doc).await.json();
    assert_eq!(det["fund"]["balance"].as_f64(), Some(40000.0));
    assert_eq!(det["counts"].as_array().unwrap().len(), 1);

    // Anular con motivo: el gasto deja de contar
    let anular = format!("/api/finance/petty/moves/{}/annul", mid);
    assert_eq!(s.post(&anular, &doc, json!({ "reason": "x" })).await.status, 403);
    assert_eq!(s.post(&anular, &adm, json!({ "reason": " " })).await.status, 400);
    assert_eq!(s.post(&anular, &adm, json!({ "reason": "Boleta duplicada" })).await.status, 200);
    assert_eq!(s.post(&anular, &adm, json!({ "reason": "otra vez" })).await.status, 409);
    let ex = s.get(&format!("/api/finance/execution/{}", pid), &doc).await.json();
    assert_eq!(ex["items"][0]["spent"].as_f64(), Some(0.0));

    // Ingresos por servicios
    let ing = json!({ "project_id": pid, "amount": 120000, "origin": "Corte laser a terceros", "destination": "Cuenta del proyecto" });
    assert_eq!(s.post("/api/finance/incomes", &doc, ing.clone()).await.status, 403);
    let r = s.post("/api/finance/incomes", &adm, ing).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let inc = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(s.get(&format!("/api/finance/incomes?project_id={}", pid), &doc).await.json().as_array().unwrap().len(), 1);
    assert_eq!(s.post(&format!("/api/finance/incomes/{}/annul", inc), &adm, json!({ "reason": "Error de digitacion" })).await.status, 200);

    // El arqueo pendiente sale en Plazos
    let plazos = s.get("/api/deadlines", &adm).await.text;
    assert!(plazos.contains("arqueo") && plazos.contains("Arqueo de caja chica"), "{}", plazos);

    // Proyecto cerrado: solo lectura; reabrir exige motivo
    assert_eq!(s.put(&cfg, &adm, json!({ "closed": true })).await.status, 200);
    assert_eq!(s.post("/api/finance/budget/items", &adm, json!({ "project_id": pid, "category": "otro", "name": "Tarde", "approved": 1 })).await.status, 409);
    assert_eq!(s.post(&moves, &adm, json!({ "kind": "ingreso", "amount": 1, "concept": "x" })).await.status, 409);
    assert_eq!(s.post("/api/finance/incomes", &adm, json!({ "project_id": pid, "amount": 5, "origin": "x" })).await.status, 409);
    assert_eq!(s.put(&cfg, &adm, json!({ "closed": false })).await.status, 400);
    assert_eq!(s.put(&cfg, &adm, json!({ "closed": false, "reason": "Falto un gasto" })).await.status, 200);

    // Todo quedo en la bitacora y la cadena sigue integra
    let led = s.get("/api/ledger", &admin).await.text;
    for k in ["arqueo", "egreso", "ingreso_servicio", "proyecto_cerrado", "item_eliminado"] {
        assert!(led.contains(k), "falta {} en la bitacora", k);
    }
    assert_eq!(s.get("/api/ledger/verify", &admin).await.json()["ok"], true);
}
