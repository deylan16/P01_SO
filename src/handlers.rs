
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Component, Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use chrono::Utc;
use std::cell::Cell;
use flate2::Compression;
use flate2::write::GzEncoder;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng, thread_rng};
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::control::{Job, SharedState,Task};
use crate::errors::{ResponseMeta,error404, error400, error409, error500, error503_json, res200_json};

const MAX_RANDOM_COUNT: u64 = 1024;
const MAX_PI_DIGITS: usize = 1000;
const MAX_SORT_ITEMS: usize = 5_000_000;
const MAX_REPEAT_WRITES: u64 = 10_000;

pub fn handle_command(
    path: &str,
    qmap: &HashMap<String, String>,
    stream: &TcpStream,
    meta: &ResponseMeta,
    state: &SharedState,
    deadline: Instant,
) -> bool {
    let guard = TimeoutGuard::new(deadline, stream, meta);
    match path {
        "/reverse" => {
            if let Some(text) = qmap.get("text") {
                let reversed: String = text.chars().rev().collect();
                respond_json(
                    stream,
                    meta,
                    json!({
                        "input": text,
                        "reversed": reversed,
                        "length": text.chars().count()
                    }),
                );
            } else {
                return error400(stream_clone(stream), "missing 'text' parameter", meta);
                
            }
            true
        }
        "/toupper" => {
            if let Some(text) = qmap.get("text") {
                respond_json(
                    stream,
                    meta,
                    json!({
                        "input": text,
                        "upper": text.to_uppercase(),
                        "length": text.chars().count()
                    }),
                );
            } else {
                return error400(stream_clone(stream), "missing 'text' parameter", meta);
                
            }
            true
        }
        "/fibonacci" => {
            match qmap
                .get("num")
                .or_else(|| qmap.get("n"))
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(n) if n <= 93 => {
                    let value = fibonacci(n);
                    respond_json(stream, meta, json!({"num": n, "value": value}));
                }
                Some(_) => return error400(stream_clone(stream), "num exceeds safe range (<=93)", meta),
                None => return error400(stream_clone(stream), "invalid or missing 'num'", meta),
            }
            true
        }
        "/createfile" => {
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(n) if !n.is_empty() => n,
                _ => {
                    return error400(stream_clone(stream), "missing 'name' parameter", meta);
                    
                }
            };
            let path = match sanitize_path(name) {
                Ok(p) => p,
                Err(msg) => {
                    return error400(stream_clone(stream), &msg, meta);
                    
                }
            };
            let path_display = path.display().to_string();
            let content = qmap.get("content").map(String::as_str).unwrap_or("");
            let repeat = qmap
                .get("repeat")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);
            if repeat == 0 || repeat > MAX_REPEAT_WRITES {
                return error400(
                    stream_clone(stream),
                    "repeat must be between 1 and 10000",
                    meta,
                );
                
            }
            let mut file = match File::create(&path) {
                Ok(f) => f,
                Err(err) => {
                    return error500(
                        stream_clone(stream),
                        &format!("unable to create {}: {}", path_display, err),
                        meta,
                    );
                    
                }
            };
            let mut written: usize = 0;
            for (idx, chunk) in std::iter::repeat(content).take(repeat as usize).enumerate() {
                if idx > 0 {
                    let _ = file.write_all(b"\n");
                    written += 1;
                }
                let bytes = chunk.as_bytes();
                if file.write_all(bytes).is_err() {
                    return error500(stream_clone(stream), "failed to write file", meta);
                    
                }
                written += bytes.len();
            }
            respond_json(
                stream,
                meta,
                json!({"file": path_display, "bytes_written": written, "repeat": repeat}),
            );
            true
        }
        "/deletefile" => {
            match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(name) => {
                    let path = match sanitize_path(name) {
                        Ok(p) => p,
                        Err(msg) => {
                            return error400(stream_clone(stream), &msg, meta);
                            
                        }
                    };
                    let display = path.display().to_string();
                    match fs::remove_file(&path) {
                        Ok(_) => respond_json(stream, meta, json!({"file": display, "deleted": true})),
                        Err(err) => return error500(
                            stream_clone(stream),
                            &format!("unable to delete {}: {}", display, err),
                            meta,
                        ),
                    }
                }
                None => return error400(stream_clone(stream), "missing 'name' parameter", meta),
            }
            true
        }
        "/random" => {
            let count = qmap
                .get("count")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);
            if count == 0 || count > MAX_RANDOM_COUNT {
                return error400(
                    stream_clone(stream),
                    "count must be between 1 and 1024",
                    meta,
                );
                
            }
            let min = qmap
                .get("min")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let max = qmap
                .get("max")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(100);
            if min > max {
                return error400(stream_clone(stream), "min must be <= max", meta);
                
            }
            let mut rng = thread_rng();
            let values: Vec<i64> = (0..count).map(|_| rng.gen_range(min..=max)).collect();
            respond_json(
                stream,
                meta,
                json!({
                    "count": count,
                    "min": min,
                    "max": max,
                    "values": values
                }),
            );
            true
        }
        "/hash" => {
            if let Some(text) = qmap.get("text") {
                let digest = sha256_hex(text.as_bytes());
                respond_json(
                    stream,
                    meta,
                    json!({"text": text, "algorithm": "sha256", "digest": digest}),
                );
            } else {
                return error400(stream_clone(stream), "missing 'text' parameter", meta);
            }
            true
        }
        "/help" => {
            let commands = vec![
                "GET /status",
                "GET /reverse?text=...",
                "GET /toupper?text=...",
                "GET /fibonacci?num=...",
                "GET /random?count=..&min=..&max=..",
                "GET /hash?text=...",
                "GET /timestamp",
                "GET /sleep?seconds=...",
                "GET /simulate?seconds=..&task=..",
                "GET /createfile?name=..&content=..&repeat=..",
                "GET /deletefile?name=..",
                "GET /isprime?n=..",
                "GET /factor?n=..",
                "GET /pi?digits=..",
                "GET /mandelbrot?width=..&height=..&max_iter=..",
                "GET /matrixmul?size=..&seed=..",
                "GET /sortfile?name=..&algo=merge|quick",
                "GET /wordcount?name=..",
                "GET /grep?name=..&pattern=..",
                "GET /compress?name=..&codec=gzip",
                "GET /hashfile?name=..&algo=sha256",
                "GET /metrics",
            ];
            respond_json(stream, meta, json!({"commands": commands}));
            true
        }
        "/timestamp" => {
            let now = Utc::now();
            respond_json(
                stream,
                meta,
                json!({"iso8601": now.to_rfc3339(), "epoch_ms": now.timestamp_millis()}),
            );
            true
        }
        "/sleep" => {
            let seconds = qmap
                .get("seconds")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            if guard.expired() {
                return true;
            }
            let desired = Duration::from_secs(seconds);
            let Some(remaining) = guard.remaining() else {
                guard.trigger();
                return true;
            };
            if desired > remaining {
                guard.trigger();
                return true;
            }
            sleep(desired);
            respond_json(stream, meta, json!({"slept_seconds": seconds}));
            true
        }
        "/simulate" => {
            let seconds = qmap
                .get("seconds")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let task = qmap
                .get("task")
                .cloned()
                .unwrap_or_else(|| "default".to_string());
            let until = Instant::now() + Duration::from_secs(seconds);
            let mut counter = 0u64;
            while Instant::now() < until {
                if guard.expired() {
                    return true;
                }
                counter = counter.wrapping_add(1);
            }
            respond_json(
                stream,
                meta,
                json!({"task": task, "seconds": seconds, "iterations": counter}),
            );
            true
        }
        "/loadtest" => {
            let tasks = qmap
                .get("tasks")
                .or_else(|| qmap.get("jobs"))
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(10);
            let sleep_ms = qmap
                .get("sleep")
                .or_else(|| qmap.get("ms"))
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(50);
            let start = Instant::now();
            for _ in 0..tasks {
                if guard.expired() {
                    return true;
                }
                let until = Instant::now() + Duration::from_millis(sleep_ms);
                let mut noisy = 0u64;
                while Instant::now() < until {
                    if guard.expired() {
                        return true;
                    }
                    noisy = noisy.wrapping_add(1);
                }
                std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
            }
            let elapsed = start.elapsed().as_millis();
            respond_json(
                stream,
                meta,
                json!({"tasks": tasks, "sleep_ms": sleep_ms, "elapsed_ms": elapsed}),
            );
            true
        }
        "/isprime" => {
            match qmap.get("n").and_then(|s| s.parse::<u128>().ok()) {
                Some(n) => {
                    let start = Instant::now();
                    let is_prime = match check_prime(n, &guard) {
                        Ok(val) => val,
                        Err(_) => return true,
                    };
                    let elapsed = start.elapsed().as_millis();
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "n": n,
                            "is_prime": is_prime,
                            "method": "trial-division",
                            "elapsed_ms": elapsed
                        }),
                    );
                }
                None => return error400(stream_clone(stream), "invalid or missing 'n'", meta),
            }
            true
        }
        "/factor" => {
            match qmap.get("n").and_then(|s| s.parse::<u128>().ok()) {
                Some(n) => {
                    let start = Instant::now();
                    let factors = match factorize(n, &guard) {
                        Ok(v) => v,
                        Err(_) => return true,
                    };
                    let elapsed = start.elapsed().as_millis();
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "n": n,
                            "factors": factors,
                            "elapsed_ms": elapsed
                        }),
                    );
                }
                None => return error400(stream_clone(stream), "invalid or missing 'n'", meta),
            }
            true
        }
        "/pi" => {
            let digits = qmap
                .get("digits")
                .or_else(|| qmap.get("iters"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(10);
            if digits == 0 || digits > MAX_PI_DIGITS {
                return error400(
                    stream_clone(stream),
                    &format!("digits must be between 1 and {}", MAX_PI_DIGITS),
                    meta,
                );
            }
            let start = Instant::now();
            let pi = match compute_pi_digits(digits, &guard) {
                Ok(pi) => pi,
                Err(_) => return true,
            };
            let elapsed = start.elapsed().as_millis();
            respond_json(
                stream,
                meta,
                json!({"digits": digits, "pi": pi, "elapsed_ms": elapsed}),
            );
            true
        }
        "/mandelbrot" => {
            let width = qmap
                .get("width")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(80);
            let height = qmap
                .get("height")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(24);
            let max_iter = qmap
                .get("max_iter")
                .or_else(|| qmap.get("iter"))
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(50);
            if width == 0 || height == 0 || width > 1000 || height > 1000 || max_iter == 0 {
                return error400(stream_clone(stream), "invalid size or max_iter", meta);
                
            }
            let sanitized_output = if let Some(target) = qmap.get("file") {
                match sanitize_path(target) {
                    Ok(path) => Some(path),
                    Err(msg) => {
                        return error400(stream_clone(stream), &msg, meta);
                        
                    }
                }
            } else {
                None
            };
            let (iters, elapsed, pgm_written) = match mandelbrot(
                width,
                height,
                max_iter,
                sanitized_output.as_deref(),
                &guard,
            ) {
                Ok(v) => v,
                Err(_) => return true,
            };
            respond_json(
                stream,
                meta,
                json!({
                    "width": width,
                    "height": height,
                    "max_iter": max_iter,
                    "elapsed_ms": elapsed,
                    "file": pgm_written,
                    "iterations": iters
                }),
            );
            true
        }
        "/matrixmul" => {
            let size = qmap
                .get("size")
                .or_else(|| qmap.get("n"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(100);
            if size == 0 || size > 600 {
                return error400(stream_clone(stream), "size must be between 1 and 600", meta);
                
            }
            let seed = qmap
                .get("seed")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(42);
            let start = Instant::now();
            let hash = match matrix_multiply_hash(size, seed, &guard) {
                Ok(hash) => hash,
                Err(_) => return true,
            };
            let elapsed = start.elapsed().as_millis();
            respond_json(
                stream,
                meta,
                json!({
                    "size": size,
                    "seed": seed,
                    "result_sha256": hash,
                    "elapsed_ms": elapsed
                }),
            );
            true
        }
        "/sortfile" => {
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(path) if !path.is_empty() => path,
                _ => {
                    return error400(stream_clone(stream), "missing 'name' parameter", meta);
                    
                }
            };
            let path = match sanitize_path(name) {
                Ok(p) => p,
                Err(msg) => {
                    return error400(stream_clone(stream), &msg, meta);
                    
                }
            };
            let algo = qmap.get("algo").map(String::as_str).unwrap_or("quick");
            if !matches!(algo, "quick" | "merge") {
                return error400(stream_clone(stream), "algo must be quick or merge", meta);
                
            }
            let start = Instant::now();
            match sort_file(&path, algo, &guard) {
                Ok((sorted_path, count)) => {
                    let elapsed = start.elapsed().as_millis();
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "file": path.display().to_string(),
                            "algo": algo,
                            "sorted_file": sorted_path,
                            "items": count,
                            "elapsed_ms": elapsed
                        }),
                    );
                }
                Err(HandlerError::Timeout) => return true,
                Err(HandlerError::Message(err)) => {
                    return error500(stream_clone(stream), &err, meta)
                }
            }
            true
        }
        "/wordcount" => {
            match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(name) => {
                    let path = match sanitize_path(name) {
                        Ok(p) => p,
                        Err(msg) => {
                            return error400(stream_clone(stream), &msg, meta);
                            
                        }
                    };
                    match word_count(&path, &guard) {
                        Ok(stats) => respond_json(stream, meta, stats),
                        Err(HandlerError::Timeout) => return true,
                        Err(HandlerError::Message(err)) => {
                            return error500(stream_clone(stream), &err, meta)
                        }
                    }
                }
                None => return error400(stream_clone(stream), "missing 'name' parameter", meta),
            }
            true
        }
        "/grep" => {
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(path) => path,
                None => {
                    return error400(stream_clone(stream), "missing 'name' parameter", meta);
                    
                }
            };
            let path = match sanitize_path(name) {
                Ok(p) => p,
                Err(msg) => {
                    return error400(stream_clone(stream), &msg, meta);
                    
                }
            };
            let pattern = match qmap.get("pattern") {
                Some(pat) => pat,
                None => {
                    return error400(stream_clone(stream), "missing 'pattern'", meta);
                    
                }
            };
            let regex = match Regex::new(pattern) {
                Ok(r) => r,
                Err(err) => {
                    return error400(
                        stream_clone(stream),
                        &format!("invalid regex: {}", err),
                        meta,
                    );
                    
                }
            };
            match grep_file(&path, &regex, &guard) {
                Ok(value) => respond_json(stream, meta, value),
                Err(HandlerError::Timeout) => return true,
                Err(HandlerError::Message(err)) => {
                    return error500(stream_clone(stream), &err, meta)
                }
            }
            true
        }
        "/compress" => {
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(path) => path,
                None => {
                    return error400(stream_clone(stream), "missing 'name' parameter", meta);
                    
                }
            };
            let path = match sanitize_path(name) {
                Ok(p) => p,
                Err(msg) => {
                    return error400(stream_clone(stream), &msg, meta);
                    
                }
            };
            let codec = qmap.get("codec").map(String::as_str).unwrap_or("gzip");
            if codec != "gzip" {
                return error400(stream_clone(stream), "codec must be gzip (xz not supported)", meta);
                
            }
            match compress_file(&path, codec, &guard) {
                Ok(value) => respond_json(stream, meta, value),
                Err(HandlerError::Timeout) => return true,
                Err(HandlerError::Message(err)) => {
                    return error500(stream_clone(stream), &err, meta)
                }
            }
            true
        }
        "/hashfile" => {
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(path) => path,
                None => {
                    return error400(stream_clone(stream), "missing 'name' parameter", meta);
                    
                }
            };
            let path = match sanitize_path(name) {
                Ok(p) => p,
                Err(msg) => {
                    return error400(stream_clone(stream), &msg, meta);
                    
                }
            };
            let algo = qmap.get("algo").map(String::as_str).unwrap_or("sha256");
            if algo != "sha256" {
                return error400(stream_clone(stream), "unsupported algo (only sha256)", meta);
                
            }
            match hash_file(&path, &guard) {
                Ok(hash) => respond_json(
                    stream,
                    meta,
                    json!({"file": path.display().to_string(), "algorithm": "sha256", "digest": hash}),
                ),
                Err(HandlerError::Timeout) => return true,
                Err(HandlerError::Message(err)) => {
                    return error500(stream_clone(stream), &err, meta)
                }
            }
            true
        }
        "/jobs/submit" => {
            println!("{:#?}", qmap);
            let task_value = match qmap.get("task").map(|s| s.trim()).filter(|s| !s.is_empty()) {
                Some(task) => task,
                None => {
                    return error400(stream_clone(stream), "missing 'task' parameter", meta);
                    
                }
            };
            let target_path = normalize_task_path(task_value);

            let job_id = {
                let mut st = state.lock().unwrap();
                if !st.pool_of_workers_for_command.contains_key(&target_path) {
                    return error404(stream_clone(stream), &target_path, meta);
                    
                }
                let id = st.id_job_counter;
                st.id_job_counter += 1;
                let job_id = id.to_string();
                st.jobs.insert(
                    job_id.clone(),
                    Job {
                        id: job_id.clone(),
                        status: "queued".to_string(),
                        error_message: String::new(),
                        result: Value::Null,
                        progress: 0,
                        eta_ms: 0,
                        created_at: Utc::now(),
                        started_at: None,
                        completed_at: None,
                        task_type: target_path.clone(),
                        task_params: json!(qmap),
                    },
                );
                job_id
            };

            let tx = {
                let mut st = state.lock().unwrap();
                let senders = st
                    .pool_of_workers_for_command
                    .get(&target_path)
                    .cloned()
                    .unwrap_or_default();
                if senders.is_empty() {
                    None
                } else {
                    let idx = st
                        .counters
                        .get_mut(&target_path)
                        .expect("missing counter for command");
                    let sender = senders[*idx].clone();
                    *idx = (*idx + 1) % senders.len();
                    Some(sender)
                }
            };

            let Some(tx) = tx else {
                mark_job_failed(state, &job_id, "No hay workers disponibles");
                return error500(stream_clone(stream), "No hay workers disponibles", meta);
                
            };

            let path_and_args = build_task_invocation(&target_path, &qmap);

            match stream.try_clone() {
                Ok(job_stream) => {
                    let task = Task {
                        path_and_args,
                        stream: job_stream,
                        request_id: meta.request_id.clone(),
                        dispatched_at: Instant::now(),
                        state: "queued".to_string(),
                        job_id: job_id.clone(),
                        suppress_body: false,
                    };

                    if tx.send(task).is_err() {
                        mark_job_failed(state, &job_id, "No se pudo encolar la tarea");
                        return error500(stream_clone(stream), "Error despachando tarea", meta);
                        
                    }

                    {
                        let mut st = state.lock().unwrap();
                        st.record_dispatch(&target_path);
                    }
                }
                Err(e) => {
                    mark_job_failed(state, &job_id, "No se pudo clonar el socket");
                    return error500(
                        stream_clone(stream),
                        &format!("No se pudo clonar el socket: {}", e),
                        meta,
                    );
                    
                }
            }

            let result = json!({
                "job_id": job_id,
                "status": "queued",
            });
            respond_json(stream, meta, result);
            true
        }
        "/jobs/status" => {
            let job_id = match extract_job_id(qmap) {
                Ok(id) => id,
                Err(msg) => {
                    return error400(stream_clone(stream), &msg, meta);
                   
                }
            };

            let st = state.lock().unwrap();
            if let Some(job) = st.jobs.get(&job_id) {
                let result = json!({
                    "job_id": job.id,
                    "status": job.status,
                    "progress": job.progress,
                    "eta_ms": job.eta_ms

                });
                respond_json(stream, meta, result);
            } else {
                return error404(stream_clone(stream), "job not found", meta);
            }
            true
        }
        "/jobs/result" => {
            let job_id = match extract_job_id(qmap) {
                Ok(id) => id,
                Err(msg) => {
                    return error400(stream_clone(stream), &msg, meta);
                    
                }
            };

            let st = state.lock().unwrap();
            if let Some(job) = st.jobs.get(&job_id) {
                if job.status == "done" {
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "job_id": job.id,
                            "result": job.result,
                        }),
                    );
                } else if job.status == "failed" {
                    return error500(
                        stream_clone(stream),
                        &format!("job {} failed: {}", job.id, job.error_message),
                        meta,
                    );
                } else {
                    return error409(stream_clone(stream), "job not finished", meta);
                }
            } else {
                return error404(stream_clone(stream), "job not found", meta);
            }
            true
        }
        "/jobs/cancel" => {
            let job_id = match extract_job_id(qmap) {
                Ok(id) => id,
                Err(msg) => {
                    return error400(stream_clone(stream), &msg, meta);
                    
                }
            };

            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get_mut(&job_id) {
                match job.status.as_str() {
                    "queued" => {
                        job.status = "cancelled".to_string();
                        job.error_message = "job cancelled".to_string();
                        job.result = Value::Null;
                        respond_json(
                            stream,
                            meta,
                            json!({
                                "job_id": job.id,
                                "status": job.status,
                            }),
                        );
                    }
                    "cancelled" => {
                        respond_json(
                            stream,
                            meta,
                            json!({
                                "job_id": job.id,
                                "status": job.status,
                            }),
                        );
                    }
                    _ => {
                        return error409(stream_clone(stream), &json!({"status": "not_cancelable"}).to_string(), meta);
                    }
                }
            } else {
                return error404(stream_clone(stream), "job not found", meta);
            }
            true
        }
        "/metrics" => {
            let snapshot = {
                let st = state.lock().unwrap();
                let uptime = (Utc::now() - st.start_time).num_seconds();
                json!({
                    "uptime_seconds": uptime,
                    "total_connections": st.total_connections,
                    "pid": st.pid,
                    "queues": st.queues_snapshot(),
                    "workers": summarize_workers(&st.workers),
                    "latency_ms": st.latency_snapshot(),
                    "config": {
                        "workers_per_command": st.workers_for_command,
                        "max_in_flight_per_command": st.max_in_flight_per_command,
                        "retry_after_ms": st.retry_after_ms,
                        "task_timeout_ms": st.task_timeout_ms
                    },
                    "jobs": {
                        "total": st.jobs.len(),
                        "by_status": {
                            "queued": st.jobs.values().filter(|j| j.status == "queued").count(),
                            "running": st.jobs.values().filter(|j| j.status == "running").count(),
                            "done": st.jobs.values().filter(|j| j.status == "done").count(),
                            "failed": st.jobs.values().filter(|j| j.status == "failed").count(),
                            "cancelled": st.jobs.values().filter(|j| j.status == "cancelled").count()
                        }
                    }
                })
            };
            respond_json(stream, meta, snapshot);
            true
        }
        _ => false,
    }
}

fn respond_json(stream: &TcpStream, meta: &ResponseMeta, value: Value) {
    let body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    res200_json(stream_clone(stream), &body, meta);
}

fn stream_clone(stream: &TcpStream) -> TcpStream {
    stream.try_clone().unwrap()
}

fn fibonacci(n: u64) -> u128 {
    let (mut a, mut b) = (0u128, 1u128);
    for _ in 0..n {
        let t = a + b;
        a = b;
        b = t;
    }
    a
}
fn build_task_invocation(task_path: &str, qmap: &HashMap<String, String>) -> String {
    let mut params: Vec<String> = qmap
        .iter()
        .filter(|(k, _)| k.as_str() != "task")
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    params.sort();

    if params.is_empty() {
        task_path.to_string()
    } else {
        format!("{}?{}", task_path, params.join("&"))
    }
}

fn normalize_task_path(task: &str) -> String {
    if task.starts_with('/') {
        task.to_string()
    } else {
        format!("/{}", task)
    }
}

fn extract_job_id(qmap: &HashMap<String, String>) -> Result<String, String> {
    match qmap.get("id").map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(id) => Ok(id.to_string()),
        None => Err("missing 'id' parameter".to_string()),
    }
}

fn mark_job_failed(state: &SharedState, job_id: &str, message: &str) {
    let mut st = state.lock().unwrap();
    if let Some(job) = st.jobs.get_mut(job_id) {
        job.status = "failed".to_string();
        job.error_message = message.to_string();
        job.result = Value::Null;
        job.completed_at = Some(Utc::now());
    }
}


fn check_prime(n: u128, guard: &TimeoutGuard) -> Result<bool, ()> {
    if n < 2 {
        return Ok(false);
    }
    if n % 2 == 0 {
        return Ok(n == 2);
    }
    let mut d = 3u128;
    while d * d <= n {
        if guard.expired() {
            return Err(());
        }
        if n % d == 0 {
            return Ok(false);
        }
        d += 2;
    }
    Ok(true)
}

fn factorize(mut n: u128, guard: &TimeoutGuard) -> Result<Vec<(u128, u32)>, ()> {
    if n < 2 {
        return Ok(vec![(n, 1)]);
    }
    let mut res = Vec::new();
    let mut d = 2;
    while (d as u128) * (d as u128) <= n {
        if guard.expired() {
            return Err(());
        }
        let mut cnt = 0;
        while n % d as u128 == 0 {
            n /= d as u128;
            cnt += 1;
            if guard.expired() {
                return Err(());
            }
        }
        if cnt > 0 {
            res.push((d as u128, cnt));
        }
        d += if d == 2 { 1 } else { 2 };
    }
    if n > 1 {
        res.push((n, 1));
    }
    Ok(res)
}

fn compute_pi_digits(digits: usize, guard: &TimeoutGuard) -> Result<String, ()> {
    if digits == 0 {
        return Ok("3".to_string());
    }
    let mut pi = String::from("3.");
    let len = digits * 10 / 3 + 2;
    let mut array = vec![2u32; len];
    let mut nines = 0usize;
    let mut predigit = 0u32;

    for _ in 0..digits {
        if guard.expired() {
            return Err(());
        }
        let mut carry = 0u32;
        for j in (0..len).rev() {
            let denominator = 2 * j as u32 + 1;
            let num = array[j] * 10 + carry;
            array[j] = num % denominator;
            carry = (num / denominator) * j as u32;
            if guard.expired() {
                return Err(());
            }
        }
        array[0] = carry % 10;
        let digit = carry / 10;
        if digit == 9 {
            nines += 1;
        } else if digit == 10 {
            pi.push_str(&(predigit + 1).to_string());
            for _ in 0..nines {
                pi.push('0');
            }
            predigit = 0;
            nines = 0;
        } else {
            pi.push_str(&predigit.to_string());
            predigit = digit;
            for _ in 0..nines {
                pi.push('9');
            }
            nines = 0;
        }
    }
    pi.push_str(&predigit.to_string());
    Ok(pi)
}

fn mandelbrot(
    width: usize,
    height: usize,
    max_iter: u32,
    file: Option<&Path>,
    guard: &TimeoutGuard,
) -> Result<(Vec<Vec<u32>>, u128, Option<String>), ()> {
    let start = Instant::now();
    let mut grid = vec![vec![0u32; width]; height];
    for y in 0..height {
        if guard.expired() {
            return Err(());
        }
        for x in 0..width {
            if guard.expired() {
                return Err(());
            }
            let cx = (x as f64 / width as f64) * 3.5 - 2.5;
            let cy = (y as f64 / height as f64) * 2.0 - 1.0;
            let mut zx = 0.0f64;
            let mut zy = 0.0f64;
            let mut iter = 0u32;
            while zx * zx + zy * zy <= 4.0 && iter < max_iter {
                if guard.expired() {
                    return Err(());
                }
                let temp = zx * zx - zy * zy + cx;
                zy = 2.0 * zx * zy + cy;
                zx = temp;
                iter += 1;
            }
            grid[y][x] = iter;
        }
    }
    let elapsed = start.elapsed().as_millis() as u128;
    let mut pgm = None;
    if let Some(name) = file {
        if guard.expired() {
            return Err(());
        }
        if let Ok(mut f) = File::create(name) {
            let _ = writeln!(f, "P2\n{} {}\n255", width, height);
            for row in &grid {
                for value in row {
                    if guard.expired() {
                        return Err(());
                    }
                    let scaled = (*value as u64 * 255 / max_iter.max(1) as u64) as u32;
                    let _ = write!(f, "{} ", scaled);
                }
                let _ = writeln!(f);
            }
            pgm = Some(name.display().to_string());
        }
    }
    Ok((grid, elapsed, pgm))
}

fn matrix_multiply_hash(size: usize, seed: u64, guard: &TimeoutGuard) -> Result<String, ()> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut a = vec![0f64; size * size];
    let mut b = vec![0f64; size * size];
    for v in &mut a {
        if guard.expired() {
            return Err(());
        }
        *v = rng.gen_range(0.0..1.0);
    }
    for v in &mut b {
        if guard.expired() {
            return Err(());
        }
        *v = rng.gen_range(0.0..1.0);
    }
    let mut c = vec![0f64; size * size];
    for i in 0..size {
        for k in 0..size {
            let aik = a[i * size + k];
            for j in 0..size {
                if guard.expired() {
                    return Err(());
                }
                c[i * size + j] += aik * b[k * size + j];
            }
        }
    }
    let mut hasher = Sha256::new();
    for value in c {
        if guard.expired() {
            return Err(());
        }
        hasher.update(value.to_le_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sort_file(path: &Path, algo: &str, guard: &TimeoutGuard) -> Result<(String, usize), HandlerError> {
    guard.ensure()?;
    let file = File::open(path)
        .map_err(|e| HandlerError::msg(format!("unable to open {}: {}", path.display(), e)))?;
    let reader = BufReader::new(file);
    let mut values: Vec<i64> = Vec::new();
    for line in reader.lines() {
        guard.ensure()?;
        let line = line
            .map_err(|e| HandlerError::msg(format!("read error: {}", e)))?;
        let value = line.trim().parse::<i64>().map_err(|e| {
            HandlerError::msg(format!("unable to parse integers: {}", e))
        })?;
        values.push(value);
        if values.len() > MAX_SORT_ITEMS {
            return Err(HandlerError::msg(format!(
                "file too large (>{} items) for in-memory sort",
                MAX_SORT_ITEMS
            )));
        }
    }

    match algo {
        "merge" => merge_sort(&mut values, guard)?,
        _ => quick_sort(&mut values, guard)?,
    }

    let sorted_path = format!("{}.sorted", path.display());
    let mut out = File::create(&sorted_path)
        .map_err(|e| HandlerError::msg(format!("unable to create {}: {}", sorted_path, e)))?;
    for (idx, value) in values.iter().enumerate() {
        guard.ensure()?;
        if idx > 0 {
            writeln!(out)
                .map_err(|e| HandlerError::msg(format!("write error: {}", e)))?;
        }
        write!(out, "{}", value)
            .map_err(|e| HandlerError::msg(format!("write error: {}", e)))?;
    }
    Ok((sorted_path, values.len()))
}

fn merge_sort(values: &mut [i64], guard: &TimeoutGuard) -> Result<(), HandlerError> {
    if values.len() <= 1 {
        return Ok(());
    }
    guard.ensure()?;
    let mid = values.len() / 2;
    merge_sort(&mut values[..mid], guard)?;
    merge_sort(&mut values[mid..], guard)?;
    guard.ensure()?;
    let mut merged = values.to_vec();
    merge(&values[..mid], &values[mid..], &mut merged, guard)?;
    values.copy_from_slice(&merged);
    Ok(())
}

fn merge(
    left: &[i64],
    right: &[i64],
    out: &mut [i64],
    guard: &TimeoutGuard,
) -> Result<(), HandlerError> {
    let mut i = 0;
    let mut j = 0;
    let mut k = 0;
    while i < left.len() && j < right.len() {
        guard.ensure()?;
        if left[i] <= right[j] {
            out[k] = left[i];
            i += 1;
        } else {
            out[k] = right[j];
            j += 1;
        }
        k += 1;
    }
    while i < left.len() {
        guard.ensure()?;
        out[k] = left[i];
        i += 1;
        k += 1;
    }
    while j < right.len() {
        guard.ensure()?;
        out[k] = right[j];
        j += 1;
        k += 1;
    }
    Ok(())
}

fn quick_sort(values: &mut [i64], guard: &TimeoutGuard) -> Result<(), HandlerError> {
    if values.len() <= 1 {
        return Ok(());
    }
    guard.ensure()?;
    let pivot_index = partition(values, guard)?;
    let (left, right) = values.split_at_mut(pivot_index);
    quick_sort(left, guard)?;
    quick_sort(&mut right[1..], guard)?;
    Ok(())
}

fn partition(values: &mut [i64], guard: &TimeoutGuard) -> Result<usize, HandlerError> {
    let len = values.len();
    let pivot_index = len - 1;
    let pivot = values[pivot_index];
    let mut i = 0;
    for j in 0..pivot_index {
        guard.ensure()?;
        if values[j] <= pivot {
            values.swap(i, j);
            i += 1;
        }
    }
    values.swap(i, pivot_index);
    Ok(i)
}

fn word_count(path: &Path, guard: &TimeoutGuard) -> Result<Value, HandlerError> {
    guard.ensure()?;
    let file = File::open(path)
        .map_err(|e| HandlerError::msg(format!("unable to open {}: {}", path.display(), e)))?;
    let mut reader = BufReader::new(file);
    let mut bytes = 0usize;
    let mut lines = 0usize;
    let mut words = 0usize;
    let mut buf = String::new();
    loop {
        guard.ensure()?;
        buf.clear();
        let n = reader
            .read_line(&mut buf)
            .map_err(|e| HandlerError::msg(format!("read error: {}", e)))?;
        if n == 0 {
            break;
        }
        bytes += n;
        lines += 1;
        words += buf.split_whitespace().count();
    }
    Ok(json!({
        "file": path.display().to_string(),
        "bytes": bytes,
        "lines": lines,
        "words": words
    }))
}

fn grep_file(path: &Path, regex: &Regex, guard: &TimeoutGuard) -> Result<Value, HandlerError> {
    guard.ensure()?;
    let file = File::open(path)
        .map_err(|e| HandlerError::msg(format!("unable to open {}: {}", path.display(), e)))?;
    let reader = BufReader::new(file);
    let mut matches = 0usize;
    let mut first_lines = Vec::new();
    for line in reader.lines() {
        guard.ensure()?;
        let line = line.map_err(|e| HandlerError::msg(format!("read error: {}", e)))?;
        if regex.is_match(&line) {
            matches += 1;
            if first_lines.len() < 10 {
                first_lines.push(line);
            }
        }
    }
    Ok(json!({
        "file": path.display().to_string(),
        "pattern": regex.as_str(),
        "matches": matches,
        "sample": first_lines
    }))
}

fn compress_file(path: &Path, codec: &str, guard: &TimeoutGuard) -> Result<Value, HandlerError> {
    guard.ensure()?;
    let mut input = File::open(path)
        .map_err(|e| HandlerError::msg(format!("unable to open {}: {}", path.display(), e)))?;
    let mut contents = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        guard.ensure()?;
        let n = input
            .read(&mut buf)
            .map_err(|e| HandlerError::msg(format!("read error: {}", e)))?;
        if n == 0 {
            break;
        }
        contents.extend_from_slice(&buf[..n]);
    }
    let base = path.display().to_string();
    let output_path = format!("{}.gz", base);
    let output_file = File::create(&output_path)
        .map_err(|e| HandlerError::msg(format!("unable to create {}: {}", output_path, e)))?;
    
    let mut encoder = GzEncoder::new(output_file, Compression::default());
    encoder
        .write_all(&contents)
        .map_err(|e| HandlerError::msg(format!("gzip write error: {}", e)))?;
    encoder
        .finish()
        .map_err(|e| HandlerError::msg(format!("gzip finish error: {}", e)))?;
    Ok(json!({
        "file": base,
        "codec": codec,
        "output": output_path,
        "bytes_in": contents.len(),
        "bytes_out": fs::metadata(&output_path)
            .map(|m| m.len())
            .unwrap_or(0)
    }))
}

fn hash_file(path: &Path, guard: &TimeoutGuard) -> Result<String, HandlerError> {
    guard.ensure()?;
    let mut file = File::open(path)
        .map_err(|e| HandlerError::msg(format!("unable to open {}: {}", path.display(), e)))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        guard.ensure()?;
        let n = file
            .read(&mut buf)
            .map_err(|e| HandlerError::msg(format!("read error: {}", e)))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn summarize_workers(workers: &[crate::control::WorkerInfo]) -> Value {
    let mut summary = HashMap::new();
    for worker in workers {
        let entry = summary
            .entry(worker.command.clone())
            .or_insert((0usize, 0usize));
        entry.0 += 1;
        if worker.busy {
            entry.1 += 1;
        }
    }
    let map: HashMap<_, _> = summary
        .into_iter()
        .map(|(command, (total, busy))| {
            (
                command,
                json!({
                    "total": total,
                    "busy": busy
                }),
            )
        })
        .collect();
    json!(map)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

enum HandlerError {
    Timeout,
    Message(String),
}

impl HandlerError {
    fn msg(msg: impl Into<String>) -> Self {
        HandlerError::Message(msg.into())
    }
}

struct TimeoutGuard<'a> {
    deadline: Instant,
    stream: &'a TcpStream,
    meta: &'a ResponseMeta,
    fired: Cell<bool>,
}

impl<'a> TimeoutGuard<'a> {
    fn new(deadline: Instant, stream: &'a TcpStream, meta: &'a ResponseMeta) -> Self {
        Self {
            deadline,
            stream,
            meta,
            fired: Cell::new(false),
        }
    }

    fn expired(&self) -> bool {
        if self.fired.get() {
            return true;
        }
        if Instant::now() > self.deadline {
            self.trigger();
            true
        } else {
            false
        }
    }

    fn ensure(&self) -> Result<(), HandlerError> {
        if self.expired() {
            Err(HandlerError::Timeout)
        } else {
            Ok(())
        }
    }

    fn remaining(&self) -> Option<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
    }

    fn trigger(&self) {
        if self.fired.replace(true) {
            return;
        }
        let body = json!({
            "error": "timeout",
            "message": "request exceeded maximum execution time"
        })
        .to_string();
        error503_json(stream_clone(self.stream), &body, self.meta);
    }
}

fn sanitize_path(raw: &str) -> Result<PathBuf, String> {
    if raw.trim().is_empty() {
        return Err("path cannot be empty".to_string());
    }
    let candidate = PathBuf::from(raw);
    if contains_parent_dir(&candidate) {
        return Err("path must not contain '..' segments".to_string());
    }
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        env::current_dir()
            .map_err(|e| format!("unable to resolve current dir: {}", e))?
            .join(candidate)
    };
    let allowed = allowed_roots();
    if allowed.iter().any(|root| absolute.starts_with(root)) {
        Ok(absolute)
    } else {
        Err("path outside allowed directories".to_string())
    }
}

fn contains_parent_dir(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn allowed_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = env::current_dir() {
        roots.push(cwd);
    }
    roots.push(env::temp_dir());
    if let Ok(extra) = env::var("P01_DATA_DIR") {
        let extra_path = PathBuf::from(extra);
        let absolute = if extra_path.is_absolute() {
            extra_path
        } else if let Ok(cwd) = env::current_dir() {
            cwd.join(extra_path)
        } else {
            extra_path
        };
        roots.push(absolute);
    }
    roots
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};
    use std::net::{TcpListener, TcpStream};
    use crate::control::{SharedState, Job};

    // ------------------------------
    // Helpers
    // ------------------------------

    fn dummy_stream() -> TcpStream {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || { let _ = listener.accept(); });
        TcpStream::connect(addr).unwrap()
    }

    fn dummy_state() -> SharedState {
        Arc::new(Mutex::new(crate::control::ServerState::default()))
    }

    fn dummy_meta() -> ResponseMeta {
        ResponseMeta::new("test".to_string(), "false".to_string()) // segundo campo como String
    }


    fn run_cmd(path: &str, params: HashMap<String, String>) -> bool {
        let s = dummy_stream();
        let state = dummy_state();
        let meta = dummy_meta();
        let result = handle_command(path, &params, &s, &meta, &state, Instant::now() + Duration::from_secs(5));
        println!("Command '{}' with params {:?} returned {}", path, params, result);
        result
    }

    // ------------------------------
    // 1. Funciones de texto
    // ------------------------------

    #[test]
    fn test_reverse() {
        let mut p = HashMap::new();
        p.insert("text".to_string(), "abc".to_string());
        assert!(run_cmd("/reverse", p.clone()));

        p.insert("text".to_string(), "".to_string());
        assert!(run_cmd("/reverse", p));

        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/reverse", p)); // sin parámetro → 400
    }

    #[test]
    fn test_toupper() {
        let mut p = HashMap::new();
        p.insert("text".to_string(), "abc".to_string());
        assert!(run_cmd("/toupper", p.clone()));

        p.insert("text".to_string(), "AbC123".to_string());
        assert!(run_cmd("/toupper", p));

        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/toupper", p));
    }

      // ------------------------------
    // 2. Funciones matemáticas
    // ------------------------------

    #[test]
    fn test_fibonacci() {
        for num in &[0,1,10,93] {
            let mut p = HashMap::new();
            p.insert("num".to_string(), num.to_string());
            assert!(run_cmd("/fibonacci", p));
        }

        let mut p = HashMap::new();
        p.insert("num".to_string(), "94".to_string());
        assert!(!run_cmd("/fibonacci", p));

        let mut p = HashMap::new();
        p.insert("num".to_string(), "abc".to_string());
        assert!(!run_cmd("/fibonacci", p));

        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/fibonacci", p));
    }

    #[test]
    fn test_random() {
        let mut p = HashMap::new();
        p.insert("count".to_string(), "0".to_string());
        assert!(!run_cmd("/random", p.clone()));

        p.insert("count".to_string(), "1".to_string());
        assert!(run_cmd("/random", p.clone()));

        // min > max
        p.insert("min".to_string(), "10".to_string());
        p.insert("max".to_string(), "0".to_string());
        assert!(!run_cmd("/random", p));

        // parámetros inválidos
        let mut p = HashMap::new();
        p.insert("count".to_string(), "abc".to_string());
        assert!(run_cmd("/random", p));
    }

    #[test]
    fn test_isprime() {
        for n in &[2,3,13] {
            let mut p = HashMap::new();
            p.insert("n".to_string(), n.to_string());
            assert!(run_cmd("/isprime", p));
        }
        for n in &[0,1,4,15] {
            let mut p = HashMap::new();
            p.insert("n".to_string(), n.to_string());
            assert!(run_cmd("/isprime", p));
        }
        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/isprime", p));
    }

    #[test]
    fn test_factor() {
        for n in &[6,13] {
            let mut p = HashMap::new();
            p.insert("n".to_string(), n.to_string());
            assert!(run_cmd("/factor", p));
        }
        let mut p = HashMap::new();
        p.insert("n".to_string(), (2_u64.pow(10) * 3_u64.pow(5)).to_string());
        assert!(run_cmd("/factor", p));

        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/factor", p));
    }

    #[test]
    fn test_pi() {
        for d in &[1,10,crate::handlers::MAX_PI_DIGITS] {
            let mut p = HashMap::new();
            p.insert("digits".to_string(), d.to_string());
            assert!(run_cmd("/pi", p));
        }
        let mut p = HashMap::new();
        p.insert("digits".to_string(), "0".to_string());
        assert!(!run_cmd("/pi", p));

        let mut p = HashMap::new();
        p.insert("digits".to_string(), (crate::handlers::MAX_PI_DIGITS+1).to_string());
        assert!(!run_cmd("/pi", p));
    }

    // ------------------------------
    // 3. Archivos
    // ------------------------------

    #[test]
    fn test_create_delete_sort_wordcount_grep_compress_hashfile() {
        let filename = "testfile.txt";

        // createfile
        let mut p = HashMap::new();
        p.insert("name".to_string(), filename.to_string());
        p.insert("content".to_string(), "a b c".to_string());
        assert!(run_cmd("/createfile", p.clone()));

        // createfile sin name
        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/createfile", p));

        // sortfile
        let mut p = HashMap::new();
        p.insert("name".to_string(), filename.to_string());
        p.insert("algo".to_string(), "quick".to_string());
        assert!(run_cmd("/sortfile", p.clone()));
        p.insert("algo".to_string(), "merge".to_string());
        assert!(run_cmd("/sortfile", p));

        // sortfile algo inválido
        let mut p = HashMap::new();
        p.insert("name".to_string(), filename.to_string());
        p.insert("algo".to_string(), "invalid".to_string());
        assert!(!run_cmd("/sortfile", p));

        // wordcount
        let mut p = HashMap::new();
        p.insert("name".to_string(), filename.to_string());
        assert!(run_cmd("/wordcount", p));

        // grep
        let mut p = HashMap::new();
        p.insert("name".to_string(), filename.to_string());
        p.insert("pattern".to_string(), "a".to_string());
        assert!(run_cmd("/grep", p.clone()));
        p.insert("pattern".to_string(), "z".to_string());
        assert!(run_cmd("/grep", p));

        // grep inválido
        let mut p = HashMap::new();
        p.insert("name".to_string(), filename.to_string());
        p.insert("pattern".to_string(), "[".to_string()); // regex inválido
        assert!(!run_cmd("/grep", p));

        // compress
        let mut p = HashMap::new();
        p.insert("name".to_string(), filename.to_string());
        p.insert("codec".to_string(), "gzip".to_string());
        assert!(run_cmd("/compress", p.clone()));
        p.insert("codec".to_string(), "xz".to_string());
        assert!(run_cmd("/compress", p.clone()));
        p.insert("codec".to_string(), "invalid".to_string());
        assert!(!run_cmd("/compress", p));

        // hashfile
        let mut p = HashMap::new();
        p.insert("name".to_string(), filename.to_string());
        p.insert("algo".to_string(), "sha256".to_string());
        assert!(run_cmd("/hashfile", p.clone()));
        p.insert("algo".to_string(), "invalid".to_string());
        assert!(!run_cmd("/hashfile", p));
    }

    // ------------------------------
    // 4. Simulación y tiempo
    // ------------------------------

    #[test]
    fn test_timestamp_sleep_simulate_loadtest() {
        let p: HashMap<String,String> = HashMap::new();
        assert!(run_cmd("/timestamp", p.clone()));

        let mut p = HashMap::new();
        p.insert("seconds".to_string(), "0".to_string());
        assert!(run_cmd("/sleep", p.clone()));

        let mut p = HashMap::new();
        p.insert("seconds".to_string(), "1".to_string());
        assert!(run_cmd("/simulate", p.clone()));

        let mut p = HashMap::new();
        p.insert("tasks".to_string(), "0".to_string());
        p.insert("sleep".to_string(), "0".to_string());
        assert!(run_cmd("/loadtest", p.clone()));
    }

    // ------------------------------
    // 5. Cálculos complejos
    // ------------------------------

    #[test]
    fn test_mandelbrot_matrixmul() {
        let mut p = HashMap::new();
        p.insert("width".to_string(), "10".to_string());
        p.insert("height".to_string(), "10".to_string());
        p.insert("max_iter".to_string(), "10".to_string());
        assert!(run_cmd("/mandelbrot", p.clone()));

        let mut p = HashMap::new();
        p.insert("size".to_string(), "1".to_string());
        assert!(run_cmd("/matrixmul", p.clone()));
    }

    // ------------------------------
    // 6. Hash y otros
    // ------------------------------

    #[test]
    fn test_hash() {
        let mut p = HashMap::new();
        p.insert("text".to_string(), "abc".to_string());
        assert!(run_cmd("/hash", p.clone()));

        p.insert("text".to_string(), "".to_string());
        assert!(run_cmd("/hash", p.clone()));

        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/hash", p));
    }

    // ------------------------------
    // 7. Jobs
    // ------------------------------

    #[test]
    fn test_jobs_endpoints() {
        let state = dummy_state();
        let s = dummy_stream();
        let meta = dummy_meta();

        // submit job válido
        let mut p = HashMap::new();
        p.insert("task".to_string(), "reverse".to_string());
        handle_command("/jobs/submit", &p, &s, &meta, &state, Instant::now()+Duration::from_secs(5));

        // submit sin task
        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/jobs/submit", p));

        // status sin id
        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/jobs/status", p));

        // result sin id
        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/jobs/result", p));

        // cancel sin id
        let p: HashMap<String,String> = HashMap::new();
        assert!(!run_cmd("/jobs/cancel", p));
    }

    // ------------------------------
    // 8. Auxiliares
    // ------------------------------

    #[test]
    fn test_help_metrics() {
        let p: HashMap<String,String> = HashMap::new();
        assert!(run_cmd("/help", p.clone()));
        assert!(run_cmd("/metrics", p));
    }

}
