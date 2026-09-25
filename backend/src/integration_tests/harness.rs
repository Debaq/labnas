//! Servidor LabNAS completo en proceso, con home y base propios, en un puerto libre.

use futures_util::StreamExt;
use reqwest::Method;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, MutexGuard};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::state::AppState;

/// El home de prueba es global (config::TEST_HOME): un servidor a la vez
static SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct Server {
    pub base: String,
    pub ws_base: String,
    /// Directorio temporal de la prueba (fuera de las raices)
    pub root: PathBuf,
    /// Home del servidor (unica raiz por defecto, ademas de /media, /mnt...)
    pub home: PathBuf,
    pub state: AppState,
    client: reqwest::Client,
    guard: Option<MutexGuard<'static, ()>>,
    /// En un reinicio el directorio se conserva
    keep_dir: bool,
}

pub struct Resp {
    pub status: u16,
    pub text: String,
    pub headers: reqwest::header::HeaderMap,
}

impl Resp {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or(Value::Null)
    }
    pub fn header(&self, name: &str) -> String {
        self.headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("").to_string()
    }
}

impl Server {
    pub async fn start() -> Self {
        let root = std::env::temp_dir().join(format!("labnas-it-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("home")).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        Self::boot(root, SERIAL.lock().await).await
    }

    /// Apaga y vuelve a arrancar sobre el mismo home (misma base de datos)
    pub async fn restart(mut self) -> Self {
        self.state.shutdown.cancel();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let guard = self.guard.take().expect("guard");
        let root = self.root.clone();
        self.keep_dir = true;
        drop(self);
        Self::boot(root, guard).await
    }

    async fn boot(root: PathBuf, guard: MutexGuard<'static, ()>) -> Self {
        let home = root.join("home");
        *crate::config::TEST_HOME.write().unwrap() = Some(home.to_string_lossy().to_string());

        let pool = crate::db::init_db();
        let state = crate::new_state(pool);
        let app = crate::build_api(&state, 50);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let shutdown = state.shutdown.clone();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { shutdown.cancelled().await })
                .await
                .ok();
        });
        tokio::spawn(crate::handlers::music::music_events_loop(state.clone()));

        Server {
            base: format!("http://127.0.0.1:{}", port),
            ws_base: format!("ws://127.0.0.1:{}", port),
            root,
            home,
            state,
            client: reqwest::Client::new(),
            guard: Some(guard),
            keep_dir: false,
        }
    }

    pub fn path(&self, rel: &str) -> String {
        self.home.join(rel).to_string_lossy().to_string()
    }

    pub async fn req(&self, method: Method, path: &str, token: Option<&str>, body: Option<Value>) -> Resp {
        let mut r = self.client.request(method, format!("{}{}", self.base, path));
        if let Some(t) = token {
            r = r.bearer_auth(t);
        }
        if let Some(b) = body {
            r = r.json(&b);
        }
        Self::finish(r).await
    }

    pub async fn raw(&self, r: reqwest::RequestBuilder) -> Resp {
        Self::finish(r).await
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn finish(r: reqwest::RequestBuilder) -> Resp {
        let resp = r.send().await.expect("request fallo");
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let text = resp.text().await.unwrap_or_default();
        Resp { status, text, headers }
    }

    pub async fn get(&self, path: &str, token: &str) -> Resp {
        self.req(Method::GET, path, Some(token), None).await
    }
    pub async fn post(&self, path: &str, token: &str, body: Value) -> Resp {
        self.req(Method::POST, path, Some(token), Some(body)).await
    }
    pub async fn put(&self, path: &str, token: &str, body: Value) -> Resp {
        self.req(Method::PUT, path, Some(token), Some(body)).await
    }
    pub async fn delete(&self, path: &str, token: &str) -> Resp {
        self.req(Method::DELETE, path, Some(token), None).await
    }

    pub async fn register(&self, user: &str, pass: &str) -> Resp {
        self.req(Method::POST, "/api/auth/register", None, Some(json!({ "username": user, "password": pass }))).await
    }

    /// Primer usuario (admin); devuelve su token
    pub async fn admin(&self) -> String {
        let r = self.register("admin", "adminadmin").await;
        assert_eq!(r.json()["role"], "admin");
        r.json()["token"].as_str().unwrap().to_string()
    }

    /// Registra y aprueba un usuario con `role`; devuelve su token
    pub async fn user(&self, admin: &str, name: &str, role: &str) -> String {
        let r = self.register(name, &format!("{}-password", name)).await;
        let token = r.json()["token"].as_str().unwrap().to_string();
        let ok = self.post(&format!("/api/auth/users/{}/role", name), admin, json!({ "role": role })).await;
        assert_eq!(ok.status, 200);
        token
    }

    /// WebSocket del bus de eventos (con ticket)
    pub async fn live(&self, token: &str) -> Ws {
        let ticket = self.req(Method::POST, "/api/live/ticket", Some(token), None).await.json();
        let url = format!("{}/api/live?ticket={}", self.ws_base, ticket.as_str().unwrap());
        tokio_tungstenite::connect_async(url).await.expect("ws").0
    }

    /// Codigo HTTP con el que el servidor rechaza (o acepta: 101) un WebSocket
    pub async fn ws_status(&self, path_and_query: &str) -> u16 {
        match tokio_tungstenite::connect_async(format!("{}{}", self.ws_base, path_and_query)).await {
            Ok(_) => 101,
            Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => resp.status().as_u16(),
            Err(e) => panic!("ws: {}", e),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.state.shutdown.cancel();
        if !self.keep_dir {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// Eventos recibidos durante `ms` milisegundos
pub async fn events(ws: &mut Ws, ms: u64) -> Vec<Value> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Some(Ok(msg))) = tokio::time::timeout_at(deadline, ws.next()).await {
        if let Ok(text) = msg.to_text() {
            if let Ok(v) = serde_json::from_str::<Value>(text) {
                out.push(v);
            }
        }
    }
    out
}

pub fn kinds(evs: &[Value]) -> Vec<String> {
    evs.iter().filter_map(|e| e["kind"].as_str().map(String::from)).collect()
}

/// Espera hasta que `f` devuelva true (o falla tras `ms`)
pub async fn wait_until<F, Fut>(ms: u64, mut f: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while tokio::time::Instant::now() < deadline {
        if f().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("condicion no cumplida en {} ms", ms);
}
