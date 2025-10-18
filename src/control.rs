use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

/// Información de cada worker thread
pub struct WorkerInfo {
    pub command: String,
    pub thread_id: String,
    pub busy: bool,
}

/// Estado compartido del servidor
pub struct ServerState {
    pub start_time: DateTime<Utc>,
    pub total_connections: usize,
    pub pid: u32,
    pub workers: Vec<WorkerInfo>,
}

pub type SharedState = Arc<Mutex<ServerState>>;

pub fn new_state() -> SharedState {
    Arc::new(Mutex::new(ServerState {
        start_time: Utc::now(),
        total_connections: 0,
        pid: std::process::id(),
        workers: Vec::new(),
    }))
}