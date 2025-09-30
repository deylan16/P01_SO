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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn new_state_defaults() {
        let st = new_state();
        let guard = st.lock().unwrap();
        assert_eq!(guard.total_connections, 0);
        assert_eq!(guard.pid, std::process::id());
        let now = Utc::now();
        // start_time no puede ser mayor que ahora
        assert!(guard.start_time <= now);
    }
}
