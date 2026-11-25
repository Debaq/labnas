//! Avisos de LabNAS (los mismos que salen en la web) como notificaciones del sistema.
//!
//! Un hilo espera avisos nuevos en `/api/live/notices` (long-poll) con el token del
//! computador; LabNAS solo le manda los de esa persona, los de todos y, si es admin, los
//! de administracion. Aca se filtran segun lo que la persona eligio en el widget.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::http::{self, Url};

/// Mas que la espera del servidor (~18 s)
const WAIT: Duration = Duration::from_secs(30);
/// Si llega el aviso personal de una pieza, el general de la impresora se calla un rato
const SAME_PRINT: Duration = Duration::from_secs(90);

/// Con que servidor hablar y que avisos mostrar (lo actualiza el hilo principal de red)
#[derive(Clone, Default)]
pub struct Link {
    pub base: Option<Url>,
    pub token: Option<String>,
    pub personal: bool,
    pub lab: bool,
    pub admin: bool,
}

pub type Shared = Arc<Mutex<Link>>;

pub fn spawn(shared: Shared) {
    std::thread::spawn(move || run(&shared));
}

fn run(shared: &Shared) {
    let mut since = 0u64;
    let mut key = String::new();
    let mut mine_3d: Option<Instant> = None;
    loop {
        let link = shared.lock().map(|l| l.clone()).unwrap_or_default();
        let (Some(base), Some(token)) = (link.base.clone(), link.token.clone()) else {
            since = 0;
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        // otro servidor u otra cuenta: empezar de nuevo (sin avisos viejos)
        let k = format!("{}|{token}", base.base());
        if k != key {
            key = k;
            since = 0;
        }
        let path = format!("/api/live/notices?since={since}");
        match http::request_timeout(&base, "GET", &path, Some(&token), None, WAIT) {
            Ok(r) if r.status == 200 => {
                let Some(v) = r.json() else {
                    std::thread::sleep(Duration::from_secs(10));
                    continue;
                };
                let first = since == 0;
                since = v["next"].as_u64().unwrap_or(since);
                if first {
                    continue;
                }
                for ev in v["events"].as_array().into_iter().flatten() {
                    if ev["kind"] != "notify" {
                        continue;
                    }
                    if let Some((title, body)) = pick(&ev["data"], &link, &mut mine_3d, Instant::now()) {
                        crate::media::notify(crate::media::Kind::LabNas, title, body);
                    }
                }
            }
            // LabNAS anterior a los avisos en la bandeja
            Ok(r) if r.status == 404 => std::thread::sleep(Duration::from_secs(300)),
            _ => std::thread::sleep(Duration::from_secs(10)),
        }
    }
}

/// Decide si un aviso se muestra: (titulo, texto)
fn pick(d: &Value, prefs: &Link, mine_3d: &mut Option<Instant>, now: Instant) -> Option<(String, String)> {
    let title = d["title"].as_str().filter(|t| !t.is_empty())?.to_string();
    let body = d["body"].as_str().unwrap_or("").to_string();
    let module = d["module"].as_str();
    let show = match d["scope"].as_str()? {
        // alerta de seguridad de la cuenta (entrada por internet): siempre
        "personal" if module.is_none() && d["level"] == "warning" => true,
        "personal" => {
            if module == Some("printers3d") {
                *mine_3d = Some(now);
            }
            prefs.personal
        }
        "lab" => {
            // la impresora termino la pieza de esta persona: ya llego el aviso personal
            let just_mine = module == Some("printers3d") && mine_3d.is_some_and(|t| now.duration_since(t) < SAME_PRINT);
            prefs.lab && !(just_mine && prefs.personal)
        }
        "admin" => prefs.admin,
        _ => false,
    };
    show.then_some((title, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prefs(personal: bool, lab: bool, admin: bool) -> Link {
        Link { personal, lab, admin, ..Link::default() }
    }

    #[test]
    fn que_avisos_se_muestran() {
        let now = Instant::now();
        let mut mine = None;
        let mio = json!({ "title": "Tu pieza está lista", "body": "«x»", "scope": "personal", "module": "printers3d", "level": "success" });
        let general = json!({ "title": "Impresion terminada en P1", "scope": "lab", "module": "printers3d", "level": "success" });
        let sensor = json!({ "title": "Alerta sensor", "scope": "lab", "module": "sensors", "level": "warning" });
        let admin = json!({ "title": "Usuario pendiente", "scope": "admin", "level": "info" });
        let entrada = json!({ "title": "Nuevo inicio de sesión por internet", "scope": "personal", "module": null, "level": "warning" });

        // por defecto: personales y administracion, no los del laboratorio
        let p = prefs(true, false, true);
        assert!(pick(&mio, &p, &mut mine, now).is_some());
        assert!(pick(&general, &p, &mut mine, now).is_none());
        assert!(pick(&admin, &p, &mut mine, now).is_some());

        // con los del laboratorio: el general de la propia pieza no se repite...
        let p = prefs(true, true, true);
        assert!(pick(&general, &p, &mut mine, now).is_none());
        assert!(pick(&sensor, &p, &mut mine, now).is_some());
        // ...pero el de otra impresion mas tarde si
        assert!(pick(&general, &p, &mut mine, now + SAME_PRINT * 2).is_some());

        // todo apagado: solo la alerta de seguridad
        let p = prefs(false, false, false);
        assert!(pick(&mio, &p, &mut mine, now).is_none());
        assert!(pick(&admin, &p, &mut mine, now).is_none());
        assert_eq!(pick(&entrada, &p, &mut mine, now).map(|t| t.0).as_deref(), Some("Nuevo inicio de sesión por internet"));
        // sin titulo o sin alcance (servidor raro): nada
        assert!(pick(&json!({ "title": "", "scope": "admin" }), &prefs(true, true, true), &mut mine, now).is_none());
        assert!(pick(&json!({ "title": "x" }), &prefs(true, true, true), &mut mine, now).is_none());
    }
}
