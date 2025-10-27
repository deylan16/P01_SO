use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::collections::{HashMap, VecDeque};
use std::net::TcpStream;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::fs;
use std::path::{Path,PathBuf};
const MAX_LATENCY_SAMPLES: usize = 128;
use std::time::Instant;
use serde_json::Value;
#[derive(Serialize, Clone)]
pub struct WorkerInfo {
    pub command: String,
    pub thread_id: String,
    pub busy: bool,
}


//Estrucuta para cada worker
pub struct Task {
    pub path_and_args: String,
    pub stream: TcpStream,
    pub request_id: String,
    pub dispatched_at: Instant,
    pub state: String,
    pub job_id: String,
    pub suppress_body: bool,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct Job {
    pub id: String,
    pub status: String,
    pub error_message: String,
    pub result: Value,
    pub progress: u8,
    pub eta_ms: u64,

}

pub fn jobs_file_path() -> PathBuf {
    let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    path.push("jobs_journal.json");
    path
}

pub fn save_jobs(jobs: &Vec<Job>) {
    if let Ok(json) = serde_json::to_string_pretty(jobs) {
        let path = jobs_file_path();
        let _ = fs::write(&path, json);
    }
}
pub fn load_jobs() -> Vec<Job> {
    let path = jobs_file_path();
    if Path::new(&path).exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(jobs) = serde_json::from_str::<Vec<Job>>(&data) {
                return jobs;
            }
        }
    }
    Vec::new()
}

pub struct ServerState {
    pub start_time: DateTime<Utc>,
    pub total_connections: usize,
    pub pid: u32,
    pub workers: Vec<WorkerInfo>,
    pub command_stats: HashMap<String, CommandStats>,
    pub jobs: HashMap<String, Job>,
    pub id_job_counter: usize,
    pub pool_of_workers_for_command:HashMap<String, Vec<Sender<Task>>>,
    pub counters: HashMap<String, usize>,
    pub workers_for_command: usize,
    pub max_in_flight_per_command: usize,
    pub retry_after_ms: u64,
    pub task_timeout_ms: u64,
}

pub type SharedState = Arc<Mutex<ServerState>>;

pub fn new_state() -> SharedState {
    let workers_for_command = env_usize("P01_WORKERS_PER_COMMAND", 2).max(1);
    let max_in_flight_per_command = env_usize("P01_MAX_INFLIGHT", 32).max(1);
    let retry_after_ms = env_u64("P01_RETRY_AFTER_MS", 250);
    let task_timeout_ms = env_u64("P01_TASK_TIMEOUT_MS", 60_000).max(1);

    Arc::new(Mutex::new(ServerState {
        start_time: Utc::now(),
        total_connections: 0,
        pid: std::process::id(),
        workers: Vec::new(),
        command_stats: HashMap::new(),
        jobs: HashMap::new(),
        id_job_counter: 0,
        pool_of_workers_for_command: HashMap::new(),
        counters: HashMap::new(),
        workers_for_command,
        max_in_flight_per_command,
        retry_after_ms,
        task_timeout_ms,
    }))
}

fn env_usize(var: &str, default: usize) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_u64(var: &str, default: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

#[derive(Clone)]
pub struct CommandStats {
    pub command: String,
    pub in_flight: usize,
    pub total_requests: u64,
    latencies_ms: VecDeque<u128>,
}

impl CommandStats {
    pub fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
            in_flight: 0,
            total_requests: 0,
            latencies_ms: VecDeque::with_capacity(MAX_LATENCY_SAMPLES),
        }
    }

    pub fn record_start(&mut self) {
        self.in_flight += 1;
        self.total_requests += 1;
    }

    pub fn record_finish(&mut self, elapsed_ms: u128) {
        self.in_flight = self.in_flight.saturating_sub(1);
        if self.latencies_ms.len() == MAX_LATENCY_SAMPLES {
            self.latencies_ms.pop_front();
        }
        self.latencies_ms.push_back(elapsed_ms);
    }

    pub fn latency_snapshot(&self) -> LatencySnapshot {
        let mut samples: Vec<u128> = self.latencies_ms.iter().copied().collect();
        samples.sort_unstable();
        let percentile = |p: f64| -> Option<u128> {
            if samples.is_empty() {
                return None;
            }
            let rank = (p * (samples.len() as f64 - 1.0)).round() as usize;
            samples.get(rank).copied()
        };
        LatencySnapshot {
            count: samples.len() as u64,
            p50: percentile(0.50),
            p95: percentile(0.95),
            p99: percentile(0.99),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct LatencySnapshot {
    pub count: u64,
    pub p50: Option<u128>,
    pub p95: Option<u128>,
    pub p99: Option<u128>,
}

impl ServerState {
    pub fn ensure_command(&mut self, command: &str) {
        self.command_stats
            .entry(command.to_string())
            .or_insert_with(|| CommandStats::new(command));
    }

    pub fn record_dispatch(&mut self, command: &str) {
        self.ensure_command(command);
        if let Some(stats) = self.command_stats.get_mut(command) {
            stats.record_start();
        }
    }

    pub fn record_completion(&mut self, command: &str, elapsed_ms: u128) {
        if let Some(stats) = self.command_stats.get_mut(command) {
            stats.record_finish(elapsed_ms);
        }
    }

    pub fn queues_snapshot(&self) -> HashMap<String, usize> {
        self.command_stats
            .iter()
            .map(|(cmd, stats)| (cmd.clone(), stats.in_flight))
            .collect()
    }

    pub fn latency_snapshot(&self) -> HashMap<String, LatencySnapshot> {
        self.command_stats
            .iter()
            .map(|(cmd, stats)| (cmd.clone(), stats.latency_snapshot()))
            .collect()
    }
}
