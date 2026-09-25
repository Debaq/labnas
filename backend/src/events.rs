//! Bus de eventos en tiempo real (WebSocket `/api/live`).
//!
//! Los modulos publican con `state.events.publish(...)` / `notify(...)`; cada
//! conexion recibe solo los eventos de su audiencia y de modulos activos.
//!
//! Los WebSocket del navegador no pueden mandar headers, asi que se abren con un
//! ticket de un solo uso (`POST /api/live/ticket`, 30 s) en vez del token de sesion.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

use crate::models::notifications::UserRole;
use crate::state::{AppState, SessionInfo};

const BUS_CAPACITY: usize = 256;
const TICKET_TTL: Duration = Duration::from_secs(30);
/// Cada cuanto una conexion abierta revisa que su sesion siga vigente
const SESSION_CHECK: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Audience {
    All,
    Admins,
    User(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub kind: &'static str,
    pub data: serde_json::Value,
    #[serde(skip)]
    pub audience: Audience,
    /// Solo se entrega si este modulo esta activo
    #[serde(skip)]
    pub module: Option<&'static str>,
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self { tx: broadcast::channel(BUS_CAPACITY).0 }
    }
}

impl EventBus {
    pub fn publish(&self, kind: &'static str, data: impl Serialize, audience: Audience, module: Option<&'static str>) {
        let data = serde_json::to_value(data).unwrap_or(serde_json::Value::Null);
        // Sin suscriptores send() falla; no es un error
        let _ = self.tx.send(Event { kind, data, audience, module });
    }

    /// ¿Alguna pestaña esta escuchando `kind`? Para no hacer trabajo que nadie va a ver
    /// (p.ej. consultar impresoras cada 5 s solo si alguien mira esa pagina).
    pub fn has_interest(&self, kind: &str) -> bool {
        INTEREST.lock().map(|m| m.get(kind).copied().unwrap_or(0) > 0).unwrap_or(false)
    }

    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

/// Notificacion para la UI (toast) y el visor de escritorio (notificacion nativa)
pub fn notify(state: &AppState, audience: Audience, module: Option<&'static str>, level: Level, title: &str, body: &str) {
    state.events.publish(
        "notify",
        serde_json::json!({ "level": level, "title": title, "body": body }),
        audience,
        module,
    );
}

// ─── Interes por tema ───
//
// El cliente manda {"sub": kind} / {"unsub": kind} segun que vistas tiene montadas.

static INTEREST: LazyLock<StdMutex<HashMap<String, usize>>> = LazyLock::new(|| StdMutex::new(HashMap::new()));

/// Temas de una conexion; al soltarse (desconexion) descuenta su interes
struct Interests(std::collections::HashSet<String>);

impl Interests {
    fn add(&mut self, kind: &str) {
        if kind.len() <= 64 && self.0.len() < 64 && self.0.insert(kind.to_string()) {
            if let Ok(mut m) = INTEREST.lock() {
                *m.entry(kind.to_string()).or_insert(0) += 1;
            }
        }
    }

    fn remove(&mut self, kind: &str) {
        if self.0.remove(kind) {
            if let Ok(mut m) = INTEREST.lock() {
                if let Some(n) = m.get_mut(kind) {
                    *n = n.saturating_sub(1);
                }
            }
        }
    }
}

impl Drop for Interests {
    fn drop(&mut self) {
        for kind in std::mem::take(&mut self.0) {
            if let Ok(mut m) = INTEREST.lock() {
                if let Some(n) = m.get_mut(&kind) {
                    *n = n.saturating_sub(1);
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct ClientMsg {
    sub: Option<String>,
    unsub: Option<String>,
}

// ─── Tickets de WebSocket ───

/// ticket -> (token de sesion, creado)
static TICKETS: LazyLock<StdMutex<HashMap<String, (String, Instant)>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

/// Token de sesion de la request (lo inserta el middleware)
#[derive(Debug, Clone)]
pub struct SessionToken(pub String);

/// POST /api/live/ticket — ticket de un solo uso para abrir un WebSocket
pub async fn create_ticket(Extension(token): Extension<SessionToken>) -> Result<Json<String>, (StatusCode, String)> {
    let ticket = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let mut tickets = TICKETS
        .lock()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tickets.retain(|_, (_, created)| created.elapsed() < TICKET_TTL);
    tickets.insert(ticket.clone(), (token.0, Instant::now()));
    Ok(Json(ticket))
}

/// Canjea (y consume) un ticket por el token de sesion
pub fn redeem_ticket(ticket: &str) -> Option<String> {
    let mut tickets = TICKETS.lock().ok()?;
    let (token, created) = tickets.remove(ticket)?;
    (created.elapsed() < TICKET_TTL).then_some(token)
}

// ─── Conexion ───

fn visible_to(ev: &Event, session: &SessionInfo) -> bool {
    match &ev.audience {
        Audience::All => true,
        Audience::Admins => session.role == UserRole::Admin,
        Audience::User(u) => *u == session.username,
    }
}

/// GET /api/live (WebSocket)
pub async fn events_ws(
    State(state): State<AppState>,
    Extension(session): Extension<SessionInfo>,
    Extension(token): Extension<SessionToken>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, session, token.0))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, session: SessionInfo, token: String) {
    let mut rx = state.events.subscribe();
    let mut interests = Interests(Default::default());
    let mut session_check = tokio::time::interval(SESSION_CHECK);
    session_check.tick().await;

    // Los pendientes solo reciben sus propios eventos (p.ej. "auth.changed" al ser aprobados)
    let pending = session.role == UserRole::Pendiente;

    loop {
        tokio::select! {
            ev = rx.recv() => {
                let ev = match ev {
                    Ok(ev) => ev,
                    // La pestaña quedo atras: avisar para que recargue lo que muestra
                    Err(broadcast::error::RecvError::Lagged(_)) => Event {
                        kind: "resync",
                        data: serde_json::Value::Null,
                        audience: Audience::All,
                        module: None,
                    },
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if !visible_to(&ev, &session) {
                    continue;
                }
                if pending && !matches!(ev.audience, Audience::User(_)) {
                    continue;
                }
                if let Some(m) = ev.module {
                    if !state.enabled_modules.lock().await.contains(m) {
                        continue;
                    }
                }
                let Ok(text) = serde_json::to_string(&ev) else { continue };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(m) = serde_json::from_str::<ClientMsg>(&text) {
                            if let Some(k) = m.sub { interests.add(&k); }
                            if let Some(k) = m.unsub { interests.remove(&k); }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {} // pings los responde axum
                }
            }
            _ = session_check.tick() => {
                let valid = state
                    .sessions
                    .lock()
                    .await
                    .get(&token)
                    .map(|s| !s.is_expired())
                    .unwrap_or(false);
                if !valid {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::notifications::UserPermissions;

    fn session(user: &str, role: UserRole) -> SessionInfo {
        SessionInfo { username: user.into(), role, permissions: UserPermissions::default(), created_at: 0 }
    }

    fn ev(audience: Audience) -> Event {
        Event { kind: "x", data: serde_json::Value::Null, audience, module: None }
    }

    #[test]
    fn audiencias() {
        let admin = session("ana", UserRole::Admin);
        let obs = session("bob", UserRole::Observador);
        assert!(visible_to(&ev(Audience::All), &obs));
        assert!(visible_to(&ev(Audience::Admins), &admin));
        assert!(!visible_to(&ev(Audience::Admins), &obs));
        assert!(visible_to(&ev(Audience::User("bob".into())), &obs));
        assert!(!visible_to(&ev(Audience::User("bob".into())), &admin));
    }

    #[test]
    fn interes_se_descuenta_al_desconectar() {
        let bus = EventBus::default();
        let mut a = Interests(Default::default());
        let mut b = Interests(Default::default());
        a.add("test.topic");
        a.add("test.topic"); // repetido en la misma conexion no suma
        b.add("test.topic");
        assert!(bus.has_interest("test.topic"));
        a.remove("test.topic");
        assert!(bus.has_interest("test.topic"));
        drop(b);
        assert!(!bus.has_interest("test.topic"));
        drop(a);
        assert!(!bus.has_interest("test.topic"));
    }

    #[test]
    fn ticket_de_un_solo_uso() {
        let t = "t-prueba".to_string();
        TICKETS.lock().unwrap().insert(t.clone(), ("sesion".into(), Instant::now()));
        assert_eq!(redeem_ticket(&t).as_deref(), Some("sesion"));
        assert_eq!(redeem_ticket(&t), None);
        TICKETS.lock().unwrap().insert("viejo".into(), ("s".into(), Instant::now() - TICKET_TTL * 2));
        assert_eq!(redeem_ticket("viejo"), None);
    }
}
