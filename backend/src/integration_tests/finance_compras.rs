//! Finanzas: flujo de compras con funciones separadas, cotizaciones, factura electronica,
//! recepcion, conflicto de interes y firma con passkey (handlers::finance::purchases)

use base64::Engine;
use serde_json::{json, Value};

use super::harness::Server;
use crate::handlers::passkeys::testing::SoftKey;

// PNG minimo valido (solo importa la firma)
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
const ORIGIN: &str = "https://labnas.ejemplo.cl";
const RP: &str = "labnas.ejemplo.cl";

fn file(name: &str, bytes: &[u8]) -> Value {
    json!({ "name": name, "data": base64::engine::general_purpose::STANDARD.encode(bytes) })
}

fn pdf(n: &str) -> Value {
    file("cotizacion.pdf", format!("%PDF-1.4 cotizacion {}", n).as_bytes())
}

fn dte(folio: u32, total: u32) -> Value {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><EnvioDTE><SetDTE><DTE><Documento><Encabezado>\
         <IdDoc><TipoDTE>33</TipoDTE><Folio>{folio}</Folio><FchEmis>2026-11-20</FchEmis></IdDoc>\
         <Emisor><RUTEmisor>76123456-7</RUTEmisor><RznSoc>Ferreteria Andes SpA</RznSoc></Emisor>\
         <Receptor><RUTRecep>71234500-1</RUTRecep></Receptor>\
         <Totales><MntNeto>{}</MntNeto><IVA>{}</IVA><MntTotal>{total}</MntTotal></Totales>\
         </Encabezado></Documento></DTE></SetDTE></EnvioDTE>",
        total * 100 / 119,
        total - total * 100 / 119
    );
    file("factura.xml", xml.as_bytes())
}

struct Ctx {
    s: Server,
    admin: String,
    doc: String, // docente: usa Finanzas (pide)
    inv: String, // investigador: gestiona (aprueba, paga)
    adm: String, // administrativo: gestiona (recibe)
    pid: String,
    item: String,
}

async fn setup() -> Ctx {
    let s = Server::start().await;
    let admin = s.admin().await;
    let doc = s.user(&admin, "doc", "docente").await;
    let inv = s.user(&admin, "inv", "investigador").await;
    let adm = s.user(&admin, "adm", "administrativo").await;
    let proyecto = json!({
        "id": "x", "created_at": "2026-01-01T00:00:00Z", "title": "Fondef Demo", "scope": "own", "status": "active",
        "end_date": "2030-01-01", "participants": [], "milestones": []
    });
    let pid = s.post("/api/portfolio", &admin, proyecto).await.json()["id"].as_str().unwrap().to_string();
    let item = "item1".to_string();
    let (p, i) = (pid.clone(), item.clone());
    crate::db::db_op(&s.state.db, move |c| {
        c.execute(
            "INSERT INTO fin_budget_items (id, project_id, category, name, approved, created_by, created_at) VALUES (?1, ?2, 'equipos', 'Equipos', 2000000, 'admin', 'x')",
            rusqlite::params![i, p],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    })
    .await
    .unwrap();
    Ctx { s, admin, doc, inv, adm, pid, item }
}

async fn commitment(c: &Ctx, id: &str) -> (f64, f64) {
    let id = id.to_string();
    crate::db::db_op(&c.s.state.db, move |conn| {
        conn.query_row("SELECT committed, spent FROM fin_commitments WHERE source = 'compra' AND source_id = ?1", [&id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap()
}

async fn request(c: &Ctx, who: &str, title: &str, amount: u32) -> String {
    let r = c
        .s
        .post("/api/finance/purchases", who, json!({ "project_id": c.pid, "budget_item_id": c.item, "title": title, "justification": "Se necesita para el proyecto", "estimated_amount": amount }))
        .await;
    assert_eq!(r.status, 201, "{}", r.text);
    r.json()["purchase"]["id"].as_str().unwrap().to_string()
}

fn url(id: &str, tail: &str) -> String {
    format!("/api/finance/purchases/{}/{}", id, tail)
}

/// Compra chica (sin minimo de cotizaciones) hasta quedar recibida: doc pide, inv aprueba y adm recibe
async fn to_received(c: &Ctx, title: &str, amount: u32) -> String {
    let id = request(c, &c.doc, title, amount).await;
    assert_eq!(c.s.post(&url(&id, "quoting"), &c.doc, json!({})).await.status, 200);
    let ok = json!({ "quote_id": null, "reason": "", "conflict": { "has_conflict": false, "detail": "" } });
    assert_eq!(c.s.post(&url(&id, "approve"), &c.inv, ok).await.status, 200);
    assert_eq!(c.s.post(&url(&id, "order"), &c.inv, json!({})).await.status, 200);
    let r = c.s.post(&url(&id, "receive"), &c.adm, json!({ "conforming": true })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flujo_completo_con_funciones_separadas() {
    let c = setup().await;
    let s = &c.s;
    let est = s.user(&c.admin, "est", "estudiante").await;

    // Pedir: justificacion obligatoria, proyecto valido, sin acceso no entra
    let base = json!({ "project_id": c.pid, "budget_item_id": c.item, "title": "Cortadora", "justification": "Se necesita para cortar", "estimated_amount": 1500000 });
    assert_eq!(s.post("/api/finance/purchases", &est, base.clone()).await.status, 403);
    let mut malo = base.clone();
    malo["justification"] = json!("corta");
    assert_eq!(s.post("/api/finance/purchases", &c.doc, malo).await.status, 400);
    let mut malo = base.clone();
    malo["project_id"] = json!("nope");
    assert_eq!(s.post("/api/finance/purchases", &c.doc, malo).await.status, 400);
    let mut malo = base.clone();
    malo["estimated_amount"] = json!(0);
    assert_eq!(s.post("/api/finance/purchases", &c.doc, malo).await.status, 400);
    let r = s.post("/api/finance/purchases", &c.doc, base).await;
    assert_eq!(r.status, 201, "{}", r.text);
    let d = r.json();
    let id = d["purchase"]["id"].as_str().unwrap().to_string();
    assert_eq!(d["purchase"]["folio"], 1);
    assert_eq!(d["purchase"]["status"], "solicitud");
    assert_eq!(d["required_quotes"], 3, "sobre $1.000.000 se exigen 3 cotizaciones");
    assert!(d["steps"][0]["ledger_seq"].is_i64(), "cada paso queda en la bitacora");
    // Ver (sin gestionar) lista la compra; sin acceso, no
    assert_eq!(s.get("/api/finance/purchases", &est).await.status, 403);
    assert_eq!(s.get(&format!("/api/finance/purchases?project={}&status=solicitud", c.pid), &c.doc).await.json().as_array().unwrap().len(), 1);

    // No se saltan pasos
    let aprobar = |quote: Value, reason: &str, conflict: bool| {
        json!({ "quote_id": quote, "reason": reason, "conflict": { "has_conflict": conflict, "detail": if conflict { "Es mi cuñado" } else { "" } } })
    };
    assert_eq!(s.post(&url(&id, "approve"), &c.inv, aprobar(Value::Null, "", false)).await.status, 409, "aprobar sin cotizar");
    assert_eq!(s.post(&url(&id, "order"), &c.inv, json!({})).await.status, 409);
    assert_eq!(s.post(&url(&id, "receive"), &c.adm, json!({ "conforming": true })).await.status, 409);
    assert_eq!(s.post(&url(&id, "pay"), &c.inv, json!({})).await.status, 409);
    assert_eq!(s.post(&url(&id, "approve"), &c.doc, aprobar(Value::Null, "", false)).await.status, 403, "quien solo usa no aprueba");

    // Cotizaciones: solo quien pidio o gestiona; con archivo; la primera pasa a "cotizacion"
    let q = |supplier: &str, amount: u32, n: &str| json!({ "supplier": supplier, "supplier_rut": "76.123.456-7", "amount": amount, "notes": "", "file": pdf(n) });
    assert_eq!(s.post(&url(&id, "quotes"), &c.adm, q("A", 1400000, "0")).await.status, 200, "quien gestiona tambien puede");
    assert_eq!(s.post(&url(&id, "quotes"), &c.doc, json!({ "supplier": "B", "amount": 100 })).await.status, 422, "sin archivo");
    let mut sin_tipo = q("B", 1400000, "1");
    sin_tipo["file"] = file("x.bin", b"MZ\x90\x00");
    assert_eq!(s.post(&url(&id, "quotes"), &c.doc, sin_tipo).await.status, 400, "tipo de archivo no admitido");
    let d = s.get(&format!("/api/finance/purchases/{}", id), &c.doc).await.json();
    assert_eq!(d["purchase"]["status"], "cotizacion");
    // Con una sola cotizacion no se puede aprobar (minimo 3 sobre $1.000.000)
    let q1 = d["quotes"][0]["id"].as_str().unwrap().to_string();
    let r = s.post(&url(&id, "approve"), &c.inv, aprobar(json!(q1), "", false)).await;
    assert_eq!(r.status, 409, "{}", r.text);
    assert!(r.text.contains("3 cotizaciones"));
    // Cambiar el monto de la primera no se puede (no se edita): se agregan las otras
    for (sup, amt, n) in [("Barata SpA", 1200000u32, "2"), ("Cara Ltda", 1600000, "3")] {
        assert_eq!(s.post(&url(&id, "quotes"), &c.doc, q(sup, amt, n)).await.status, 200);
    }
    let d = s.get(&format!("/api/finance/purchases/{}", id), &c.doc).await.json();
    assert_eq!(d["quotes"].as_array().unwrap().len(), 3);
    let barata = d["quotes"][0].clone();
    let media = d["quotes"][1].clone();
    assert_eq!(barata["amount"], 1200000.0);
    assert_eq!(barata["cheapest"], true);
    // Hasta aprobar no hay compromiso
    assert!(crate::db::db_op(&s.state.db, {
        let id = id.clone();
        move |conn| conn.query_row("SELECT COUNT(*) FROM fin_commitments WHERE source_id = ?1", [&id], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())
    })
    .await
    .unwrap()
        == 0);

    // Aprobar: elegir una que no es la mas barata exige razon; hay que declarar conflicto
    let media_id = media["id"].as_str().unwrap();
    assert_eq!(s.post(&url(&id, "approve"), &c.inv, aprobar(json!(media_id), "", false)).await.status, 400, "sin razon");
    assert_eq!(s.post(&url(&id, "approve"), &c.inv, json!({ "quote_id": media_id, "reason": "Entrega inmediata" })).await.status, 400, "sin declaracion de conflicto");
    assert_eq!(s.post(&url(&id, "approve"), &c.inv, aprobar(json!("zzz"), "Entrega inmediata", false)).await.status, 400, "cotizacion ajena");
    // Con conflicto no aprueba, pero queda registrado
    let r = s.post(&url(&id, "approve"), &c.inv, aprobar(json!(media_id), "Entrega inmediata", true)).await;
    assert_eq!(r.status, 403, "{}", r.text);
    let d = s.get(&format!("/api/finance/purchases/{}", id), &c.doc).await.json();
    assert_eq!(d["purchase"]["status"], "cotizacion");
    assert_eq!(d["declarations"][0]["has_conflict"], true);
    assert!(d["declarations"][0]["ledger_seq"].is_i64());
    assert!(d["steps"].as_array().unwrap().iter().any(|st| st["step"] == "conflicto"));
    // Sin conflicto, con razon: aprueba, queda comprometido el monto de la cotizacion elegida
    let r = s.post(&url(&id, "approve"), &c.inv, aprobar(json!(media_id), "Entrega inmediata", false)).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let d = r.json();
    assert_eq!(d["purchase"]["status"], "aprobada");
    assert_eq!(d["purchase"]["approved_by"], "inv");
    assert_eq!(d["purchase"]["choice_reason"], "Entrega inmediata");
    assert_eq!(d["declarations"].as_array().unwrap().len(), 2);
    assert_eq!(commitment(&c, &id).await, (1400000.0, 0.0));
    assert_eq!(s.post(&url(&id, "approve"), &c.inv, aprobar(json!(media_id), "x", false)).await.status, 409, "ya aprobada");
    assert_eq!(s.post(&url(&id, "quotes"), &c.doc, q("Tarde", 1, "9")).await.status, 409, "ya no admite cotizaciones");

    // Orden (Gestionar) y recepcion con separacion de funciones
    assert_eq!(s.post(&url(&id, "order"), &c.doc, json!({})).await.status, 403);
    assert_eq!(s.post(&url(&id, "order"), &c.inv, json!({ "note": "OC-77" })).await.status, 200);
    let rec = json!({ "conforming": true, "notes": "Llego completa", "photo": file("foto.png", &base64::engine::general_purpose::STANDARD.decode(PNG).unwrap()), "inventory": { "name": "Cortadora laser", "quantity": 1 } });
    let r = s.post(&url(&id, "receive"), &c.doc, rec.clone()).await;
    assert_eq!(r.status, 403, "quien pidio no recibe: {}", r.text);
    assert!(r.text.contains("pidio"));
    let r = s.post(&url(&id, "receive"), &c.inv, rec.clone()).await;
    assert_eq!(r.status, 403, "quien aprobo no recibe: {}", r.text);
    assert!(r.text.contains("aprobo"));
    let r = s.post(&url(&id, "receive"), &c.adm, json!({ "conforming": false })).await;
    assert_eq!(r.status, 400, "no conforme exige observaciones");
    let r = s.post(&url(&id, "receive"), &c.adm, rec).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let d = r.json();
    assert_eq!(d["purchase"]["status"], "recibida");
    assert_eq!(d["purchase"]["received_by"], "adm");
    assert_eq!(d["receptions"][0]["conforming"], true);
    assert!(d["receptions"][0]["photo"]["sha256"].is_string());
    let (n_items, cost): (i64, f64) = crate::db::db_op(&s.state.db, |conn| {
        conn.query_row("SELECT COUNT(*), COALESCE(SUM(cost), 0) FROM inventory_items WHERE name = 'Cortadora laser' AND project_id IS NOT NULL", [], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())
    })
    .await
    .unwrap();
    assert_eq!((n_items, cost), (1, 1400000.0), "el bien nace en Inventario con el costo de la compra");

    // Factura electronica del SII (XML): datos exactos; no cuenta hasta que una persona confirma
    let r = s.post(&url(&id, "invoice"), &c.doc, json!({ "file": file("mala.xml", b"<?xml version=\"1.0\"?><hola/>") })).await;
    assert_eq!(r.status, 400);
    let r = s.post(&url(&id, "invoice"), &c.doc, json!({ "file": dte(4321, 1400000) })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let d = r.json();
    let docu = &d["documents"][0];
    assert_eq!(
        (docu["extracted_by"].as_str(), docu["folio"].as_str(), docu["issuer_rut"].as_str(), docu["issued_on"].as_str(), docu["total_amount"].as_f64()),
        (Some("sii_xml"), Some("4321"), Some("76123456-7"), Some("2026-11-20"), Some(1400000.0))
    );
    assert_eq!(docu["issuer_name"], "Ferreteria Andes SpA");
    assert!(docu["confirmed_by"].is_null());
    assert_eq!(d["purchase"]["status"], "recibida");
    let doc_id = docu["id"].as_str().unwrap().to_string();
    assert_eq!(s.post(&format!("/api/finance/purchases/{}/invoice/{}/confirm", id, doc_id), &c.doc, json!({})).await.status, 403);
    // Los datos del XML no se editan
    let r = s.post(&format!("/api/finance/purchases/{}/invoice/{}/confirm", id, doc_id), &c.inv, json!({ "total": 5, "folio": "9" })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let d = r.json();
    assert_eq!(d["purchase"]["status"], "facturada");
    assert_eq!(d["purchase"]["final_amount"], 1400000.0);
    assert_eq!(d["documents"][0]["folio"], "4321");
    assert_eq!(d["documents"][0]["confirmed_by"], "inv");
    assert_eq!(commitment(&c, &id).await, (1400000.0, 0.0));

    // Factura duplicada (mismo RUT emisor y folio) en otra compra: 409
    let otra = to_received(&c, "Tornillos", 50000).await;
    let r = s.post(&url(&otra, "invoice"), &c.doc, json!({ "file": dte(4321, 50000) })).await;
    assert_eq!(r.status, 409, "{}", r.text);
    // Una factura PDF a mano: se completa y una persona confirma
    let r = s.post(&url(&otra, "invoice"), &c.doc, json!({ "file": pdf("f"), "folio": "55", "issuer_rut": "77.000.111-2", "issuer_name": "Otro", "issued_on": "2026-11-21", "total": 50000 })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["documents"][0]["extracted_by"], "manual");
    let doc2 = r.json()["documents"][0]["id"].as_str().unwrap().to_string();
    let r = s.post(&format!("/api/finance/purchases/{}/invoice/{}/confirm", otra, doc2), &c.inv, json!({ "issued_on": "21/11/2026" })).await;
    assert_eq!(r.status, 400, "fecha invalida");
    assert_eq!(s.post(&format!("/api/finance/purchases/{}/invoice/{}/confirm", otra, doc2), &c.inv, json!({})).await.status, 200);

    // Pago: Gestionar; lo gastado pasa a ser el monto de la factura
    assert_eq!(s.post(&url(&id, "pay"), &c.doc, json!({})).await.status, 403);
    let r = s.post(&url(&id, "pay"), &c.inv, json!({ "reference": "Transferencia 1" })).await;
    assert_eq!(r.status, 200, "{}", r.text);
    assert_eq!(r.json()["purchase"]["status"], "pagada");
    assert_eq!(commitment(&c, &id).await, (0.0, 1400000.0));
    assert_eq!(s.post(&url(&id, "pay"), &c.inv, json!({})).await.status, 409, "no se paga dos veces");
    let r = s.post(&url(&id, "settle"), &c.adm, json!({})).await;
    assert_eq!(r.json()["purchase"]["status"], "rendida");

    // Todos los pasos quedaron en la bitacora, con numero y huella
    let d = s.get(&format!("/api/finance/purchases/{}", id), &c.doc).await.json();
    for st in d["steps"].as_array().unwrap() {
        assert!(st["ledger_seq"].is_i64() && st["ledger_hash"].is_string(), "paso sin bitacora: {}", st);
    }
    let led = s.get("/api/ledger", &c.admin).await.text;
    assert!(led.contains("compra #1") && led.contains("sha256"));

    // Rechazo y anulacion llevan los compromisos a cero
    let rech = request(&c, &c.adm, "Cosa cara", 30000).await;
    assert_eq!(s.post(&url(&rech, "reject"), &c.adm, json!({ "reason": "no corresponde" })).await.status, 403, "quien pidio no rechaza");
    assert_eq!(s.post(&url(&rech, "reject"), &c.doc, json!({ "reason": "no corresponde" })).await.status, 403);
    assert_eq!(s.post(&url(&rech, "reject"), &c.inv, json!({ "reason": "no" })).await.status, 400, "motivo muy corto");
    assert_eq!(s.post(&url(&rech, "reject"), &c.inv, json!({ "reason": "No corresponde al proyecto" })).await.status, 200);
    assert_eq!(commitment(&c, &rech).await, (0.0, 0.0));
    assert_eq!(s.post(&url(&rech, "cancel"), &c.adm, json!({ "reason": "ya rechazada" })).await.status, 409);
    // Mismo usuario pide y aprueba: 403 con mensaje claro
    let propia = request(&c, &c.adm, "Compra propia", 30000).await;
    assert_eq!(s.post(&url(&propia, "quoting"), &c.adm, json!({})).await.status, 200);
    let r = s.post(&url(&propia, "approve"), &c.adm, aprobar(Value::Null, "", false)).await;
    assert_eq!(r.status, 403);
    assert!(r.text.contains("quien pidio"), "{}", r.text);
    // Anular: motivo obligatorio; quien pidio solo antes de aprobar
    assert_eq!(s.post(&url(&propia, "cancel"), &c.adm, json!({})).await.status, 400);
    assert_eq!(s.post(&url(&propia, "cancel"), &c.adm, json!({ "reason": "Ya no se necesita" })).await.status, 200);
    let aprob = to_received(&c, "Cables", 20000).await;
    assert_eq!(commitment(&c, &aprob).await, (20000.0, 0.0), "sin cotizacion se compromete el monto estimado");
    assert_eq!(s.post(&url(&aprob, "cancel"), &c.doc, json!({ "reason": "Ya no se necesita" })).await.status, 403);
    assert_eq!(s.post(&url(&aprob, "cancel"), &c.inv, json!({ "reason": "Ya no se necesita" })).await.status, 200);
    assert_eq!(commitment(&c, &aprob).await, (0.0, 0.0));

    // Advertencia (sin bloquear) cuando supera lo disponible del item: 2.000.000 - 1.400.000 gastado - 50.000 comprometido
    let grande = request(&c, &c.doc, "Muy grande", 900000).await;
    let d = s.get(&format!("/api/finance/purchases/{}", grande), &c.doc).await.json();
    assert_eq!(d["warnings"].as_array().unwrap().len(), 1, "{}", d);
    assert_eq!(d["budget"]["available"].as_f64(), Some(550000.0));
    // Items de presupuesto para elegir
    let items = s.get(&format!("/api/finance/purchases/items?project={}", c.pid), &c.doc).await.json();
    assert_eq!(items[0]["id"], "item1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ajustes_y_minimo_de_cotizaciones() {
    let c = setup().await;
    let s = &c.s;
    let d = s.get("/api/finance/purchases/settings", &c.doc).await.json();
    assert_eq!((d["min_quotes"].as_i64(), d["quote_threshold"].as_f64(), d["require_passkey"].as_bool()), (Some(3), Some(1000000.0), Some(false)));
    let nuevo = json!({ "min_quotes": 2, "quote_threshold": 200000, "require_passkey": false });
    assert_eq!(s.put("/api/finance/purchases/settings", &c.doc, nuevo.clone()).await.status, 403);
    assert_eq!(s.put("/api/finance/purchases/settings", &c.inv, json!({ "min_quotes": 0, "quote_threshold": 1, "require_passkey": false })).await.status, 400);
    assert_eq!(s.put("/api/finance/purchases/settings", &c.inv, nuevo).await.status, 200);

    // Ahora 2 cotizaciones desde $200.000
    let id = request(&c, &c.doc, "Router", 250000).await;
    let q = |n: u32| json!({ "supplier": format!("P{}", n), "amount": 250000 + n, "file": pdf(&n.to_string()) });
    assert_eq!(s.post(&url(&id, "quotes"), &c.doc, q(1)).await.status, 200);
    let d = s.get(&format!("/api/finance/purchases/{}", id), &c.doc).await.json();
    let q1 = d["quotes"][0]["id"].as_str().unwrap().to_string();
    let ap = json!({ "quote_id": q1, "reason": "", "conflict": { "has_conflict": false, "detail": "" } });
    assert_eq!(s.post(&url(&id, "approve"), &c.inv, ap.clone()).await.status, 409);
    assert_eq!(s.post(&url(&id, "quotes"), &c.doc, q(2)).await.status, 200);
    assert_eq!(s.post(&url(&id, "approve"), &c.inv, ap).await.status, 200, "es la mas barata: no pide razon");
    assert_eq!(commitment(&c, &id).await, (250001.0, 0.0));
}

async fn post_origin(s: &Server, path: &str, token: &str, body: Value) -> (u16, Value) {
    let resp = s.client().post(format!("{}{}", s.base, path)).header("origin", ORIGIN).bearer_auth(token).json(&body).send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    (status, serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aprobacion_firmada_con_passkey() {
    let c = setup().await;
    let s = &c.s;
    let mut key = SoftKey::new();

    // inv registra su passkey
    let (st, opts) = post_origin(s, "/api/auth/passkey/register/options", &c.inv, json!({})).await;
    assert_eq!(st, 200);
    let (st, r) = post_origin(s, "/api/auth/passkey/register", &c.inv, key.create(RP, opts["challenge"].as_str().unwrap(), ORIGIN)).await;
    assert_eq!(st, 200, "{}", r);

    let nuevo = json!({ "min_quotes": 3, "quote_threshold": 1000000, "require_passkey": true });
    assert_eq!(s.put("/api/finance/purchases/settings", &c.inv, nuevo).await.status, 200);
    let ap = |extra: Value| {
        let mut b = json!({ "quote_id": null, "reason": "", "conflict": { "has_conflict": false, "detail": "" } });
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        b
    };

    let id = request(&c, &c.doc, "Fresadora", 80000).await;
    assert_eq!(s.post(&url(&id, "quoting"), &c.doc, json!({})).await.status, 200);
    // Sin firma: la exige el ajuste
    assert_eq!(s.post(&url(&id, "approve"), &c.inv, ap(json!({}))).await.status, 400);
    // Desafio ligado a esta compra y monto; sin passkey propia no hay opciones
    assert_eq!(post_origin(s, &url(&id, "passkey/options"), &c.adm, json!({})).await.0, 400);
    let (st, o1) = post_origin(s, &url(&id, "passkey/options"), &c.inv, json!({})).await;
    assert_eq!(st, 200, "{}", o1);
    assert_eq!(o1["allowCredentials"].as_array().unwrap().len(), 1);
    // Un desafio de otra compra no sirve para esta
    let otra = request(&c, &c.doc, "Otra", 80000).await;
    assert_eq!(s.post(&url(&otra, "quoting"), &c.doc, json!({})).await.status, 200);
    let (_, o2) = post_origin(s, &url(&otra, "passkey/options"), &c.inv, json!({})).await;
    let ajena = key.get(RP, o2["challenge"].as_str().unwrap(), ORIGIN);
    let r = s.post(&url(&id, "approve"), &c.inv, ap(json!({ "passkey": ajena }))).await;
    assert_eq!(r.status, 401, "{}", r.text);
    assert!(r.text.contains("otra compra"));
    // Firma valida
    let firma = key.get(RP, o1["challenge"].as_str().unwrap(), ORIGIN);
    let r = s.post(&url(&id, "approve"), &c.inv, ap(json!({ "passkey": firma.clone() }))).await;
    assert_eq!(r.status, 200, "{}", r.text);
    let aprob = r.json()["steps"].as_array().unwrap().iter().find(|st| st["step"] == "aprobada").unwrap().clone();
    assert_eq!(aprob["passkey"], true);
    // La misma firma no se reusa
    assert_eq!(s.post(&url(&otra, "approve"), &c.inv, ap(json!({ "passkey": firma }))).await.status, 401);
    // Aprobar sin passkey registrada de quien firma: la firma de otro usuario no vale
    let mut otra_key = SoftKey::new();
    let (_, o3) = post_origin(s, &url(&otra, "passkey/options"), &c.inv, json!({})).await;
    let falsa = otra_key.get(RP, o3["challenge"].as_str().unwrap(), ORIGIN);
    assert_eq!(s.post(&url(&otra, "approve"), &c.inv, ap(json!({ "passkey": falsa }))).await.status, 401);
}
