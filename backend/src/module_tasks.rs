//! Tareas de fondo de cada modulo (revision de correo, monitor 3D, sensores,
//! musica, escaneo de red, servidor IPP): se inician y se detienen junto con su modulo, sin
//! reiniciar LabNAS.
//!
//! En los tests de integracion el gestor queda inactivo (nadie llama a
//! `start_enabled`), asi encender un modulo no lanza tareas reales.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::task::{AbortHandle, JoinHandle};

use crate::handlers;
use crate::state::AppState;

#[derive(Clone, Default)]
pub struct ModuleTasks {
    active: Arc<AtomicBool>,
    handles: Arc<StdMutex<HashMap<String, Vec<AbortHandle>>>>,
}

#[cfg(test)]
impl ModuleTasks {
    /// Modulos con tareas corriendo (ordenados)
    pub fn running(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.handles.lock().map(|h| h.keys().cloned().collect()).unwrap_or_default();
        ids.sort();
        ids
    }
}

fn spawn_for(state: &AppState, id: &str) -> Vec<JoinHandle<()>> {
    let s = || state.clone();
    match id {
        "email" => vec![tokio::spawn(handlers::email::email_check_loop(s()))],
        "printers3d" => vec![tokio::spawn(handlers::printers3d::printer_monitor_loop(s()))],
        "music" => vec![
            tokio::spawn(handlers::music::ensure_x_access()),
            tokio::spawn(handlers::music::music_monitor_loop(s())),
            tokio::spawn(handlers::music::music_events_loop(s())),
        ],
        "sensors" => vec![
            tokio::spawn(handlers::sensors::serial_listener_loop(s())),
            tokio::spawn(handlers::sensors::sensor_monitor_loop(s())),
        ],
        "network" => vec![tokio::spawn(handlers::network::network_scan_loop(s()))],
        // Impresoras de red (IPP): el puerto se abre y cierra con el modulo
        "printing" => vec![tokio::spawn(handlers::printing::ipp_server_loop(s()))],
        // Camaras: cerrar grabaciones que un corte dejo abiertas
        // y vigilar con las reglas de deteccion con IA
        "cameras" => vec![
            tokio::spawn(handlers::cameras::startup_cleanup(s())),
            tokio::spawn(handlers::cameras::detect::detection_loop(s())),
        ],
        // Maquinas: recordatorio por Telegram antes de cada reserva
        "machines" => vec![tokio::spawn(handlers::machines::reminder_loop(s()))],
        "deadlines" => vec![tokio::spawn(handlers::deadlines::alert_loop(s()))],
        _ => Vec::new(),
    }
}

/// Al arrancar el servidor: activa el gestor e inicia las tareas de los modulos encendidos
pub async fn start_enabled(state: &AppState) {
    state.module_tasks.active.store(true, Ordering::SeqCst);
    let mods = state.enabled_modules.lock().await.clone();
    for id in mods {
        start(state, &id);
    }
}

/// Inicia las tareas de un modulo (si no estaban corriendo)
pub fn start(state: &AppState, id: &str) {
    let mt = &state.module_tasks;
    if !mt.active.load(Ordering::SeqCst) {
        return;
    }
    let Ok(mut handles) = mt.handles.lock() else { return };
    if handles.contains_key(id) {
        return;
    }
    let spawned = spawn_for(state, id);
    if !spawned.is_empty() {
        handles.insert(id.to_string(), spawned.iter().map(JoinHandle::abort_handle).collect());
    }
}

/// Detiene las tareas de un modulo y deja su estado coherente
pub async fn stop(state: &AppState, id: &str) {
    // Las camaras no dependen de una tarea propia: se cierran siempre
    if id == "cameras" {
        handlers::cameras::shutdown(state);
    }
    let taken = state.module_tasks.handles.lock().ok().and_then(|mut h| h.remove(id));
    let Some(taken) = taken else { return };
    for h in taken {
        h.abort();
    }
    match id {
        "sensors" => {
            // El puerto serie se cierra al abortar la tarea que lo tenia abierto
            let mut s = state.sensors.lock().await;
            s.receiver_connected = false;
            s.receiver_error = Some("Modulo desactivado".to_string());
        }
        "music" => {
            // Sin el monitor nadie avanza la cola: detener la reproduccion
            let _ = handlers::music::stop(axum::extract::State(state.clone())).await;
        }
        _ => {}
    }
}

#[cfg(test)]
pub mod testing {
    /// Activa el gestor como en produccion (para probar el encendido en vivo)
    pub fn activate(state: &crate::state::AppState) {
        state.module_tasks.active.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
