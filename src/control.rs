use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Serialize, Clone)]
pub struct WorkerInfo {
    pub command: String,
    pub thread_id: String,
    pub busy: bool,
}

#[derive(Serialize, Clone)]
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
