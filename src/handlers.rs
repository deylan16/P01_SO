
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chrono::Utc;
use flate2::Compression;
use flate2::write::GzEncoder;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng, thread_rng};
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use xz2::write::XzEncoder;

use crate::control::{Job, SharedState,Task};
use crate::errors::{ResponseMeta,error404, error400, error500, res200_json, cancel_res200_json};

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
    job_id: &str,
) -> bool {
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
                    job_id,state
                );
            } else {
                error400(stream_clone(stream), "missing 'text' parameter", meta, job_id,state);
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
                    job_id,state
                );
            } else {
                error400(stream_clone(stream), "missing 'text' parameter", meta, job_id,state);
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
                    respond_json(stream, meta, json!({"num": n, "value": value}), job_id,state);
                }
                Some(_) => error400(stream_clone(stream), "num exceeds safe range (<=93)", meta, job_id,state),
                None => error400(stream_clone(stream), "invalid or missing 'num'", meta, job_id,state),
            }
            true
        }
        "/createfile" => {
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(n) if !n.is_empty() => n,
                _ => {
                    error400(stream_clone(stream), "missing 'name' parameter", meta, job_id,state);
                    return true;
                }
            };
            let content = qmap.get("content").map(String::as_str).unwrap_or("");
            let repeat = qmap
                .get("repeat")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);
            if repeat == 0 || repeat > MAX_REPEAT_WRITES {
                error400(
                    stream_clone(stream),
                    "repeat must be between 1 and 10000",
                    meta,
                    job_id,state
                );
                return true;
            }
            let mut file = match File::create(name) {
                Ok(f) => f,
                Err(err) => {
                    error500(
                        stream_clone(stream),
                        &format!("unable to create {}: {}", name, err),
                        meta,
                        job_id,state
                    );
                    return true;
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
                    error500(stream_clone(stream), "failed to write file", meta, job_id,state);
                    return true;
                }
                written += bytes.len();
            }
            respond_json(
                stream,
                meta,
                json!({"file": name, "bytes_written": written, "repeat": repeat}),
                job_id,state
            );
            true
        }
        "/deletefile" => {
            match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(name) => match fs::remove_file(name) {
                    Ok(_) => respond_json(stream, meta, json!({"file": name, "deleted": true}), job_id,state),
                    Err(err) => error500(
                        stream_clone(stream),
                        &format!("unable to delete {}: {}", name, err),
                        meta,
                        job_id,state
                    ),
                },
                None => error400(stream_clone(stream), "missing 'name' parameter", meta, job_id,state),
            }
            true
        }
        "/random" => {
            let count = qmap
                .get("count")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);
            if count == 0 || count > MAX_RANDOM_COUNT {
                error400(
                    stream_clone(stream),
                    "count must be between 1 and 1024",
                    meta,
                    job_id,state
                );
                return true;
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
                error400(stream_clone(stream), "min must be <= max", meta, job_id,state);
                return true;
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
                }),job_id,state
            );
            true
        }
        "/hash" => {
            if let Some(text) = qmap.get("text") {
                let digest = sha256_hex(text.as_bytes());
                respond_json(
                    stream,
                    meta,
                    json!({"text": text, "algorithm": "sha256", "digest": digest}), job_id,state
                );
            } else {
                error400(stream_clone(stream), "missing 'text' parameter", meta, job_id,state);
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
                "GET /compress?name=..&codec=gzip|xz",
                "GET /hashfile?name=..&algo=sha256",
                "GET /metrics",
            ];
            respond_json(stream, meta, json!({"commands": commands}), job_id,state);
            true
        }
        "/timestamp" => {
            let now = Utc::now();
            respond_json(
                stream,
                meta,
                json!({"iso8601": now.to_rfc3339(), "epoch_ms": now.timestamp_millis()}), job_id,state
            );
            true
        }
        "/sleep" => {

            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            let seconds = qmap
                .get("seconds")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            sleep(Duration::from_secs(seconds));
            
            respond_json(stream, meta, json!({"slept_seconds": seconds}), job_id,state);
            {
                let mut st = state.lock().unwrap();
                st.jobs.get_mut(job_id).map(|job| {
                    job.status = "done".to_string();
                    job.result = json!({"slept_seconds": seconds}); // ejemplo
                });
            }
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
                counter = counter.wrapping_add(1);
            }
            respond_json(
                stream,
                meta,
                json!({"task": task, "seconds": seconds, "iterations": counter}),
                job_id,state
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
                let until = Instant::now() + Duration::from_millis(sleep_ms);
                let mut noisy = 0u64;
                while Instant::now() < until {
                    noisy = noisy.wrapping_add(1);
                }
                std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
            }
            let elapsed = start.elapsed().as_millis();
            respond_json(
                stream,
                meta,
                json!({"tasks": tasks, "sleep_ms": sleep_ms, "elapsed_ms": elapsed}),
                job_id,state
            );
            true
        }
        "/isprime" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            match qmap.get("n").and_then(|s| s.parse::<u128>().ok()) {
                Some(n) => {
                    let start = Instant::now();
                    let is_prime = check_prime(n,job_id,state);
                    let elapsed = start.elapsed().as_millis();
                    let job_status = {
                        let st = state.lock().unwrap();
                        st.jobs.get(job_id).map(|job| job.status.clone())
                    };
                    if let Some(status) = job_status {
                        if status == "canceled" {
                            return true; 
                        }
                    }
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "n": n,
                            "is_prime": is_prime,
                            "method": "trial-division",
                            "elapsed_ms": elapsed
                        }),
                        job_id,state
                    );
                }
                None => error400(stream_clone(stream), "invalid or missing 'n'", meta, job_id,state),
            }
            true
        }
        "/factor" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            match qmap.get("n").and_then(|s| s.parse::<u128>().ok()) {
                Some(mut n) => {
                    let start = Instant::now();
                    let factors = factorize(n,job_id,state);
                    let elapsed = start.elapsed().as_millis();
                    let job_status = {
                        let st = state.lock().unwrap();
                        st.jobs.get(job_id).map(|job| job.status.clone())
                    };
                    if let Some(status) = job_status {
                        if status == "canceled" {
                            return true; 
                        }
                    }
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "n": n,
                            "factors": factors,
                            "elapsed_ms": elapsed
                        }),
                        job_id,state
                    );
                }
                None => error400(stream_clone(stream), "invalid or missing 'n'", meta, job_id,state),
            }
            true
        }
        "/pi" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            let digits = qmap
                .get("digits")
                .or_else(|| qmap.get("iters"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(10);
            if digits == 0 || digits > MAX_PI_DIGITS {
                error400(
                    stream_clone(stream),
                    &format!("digits must be between 1 and {}", MAX_PI_DIGITS),
                    meta,
                    job_id,state
                );
                return true;
            }
            let start = Instant::now();
            let pi = compute_pi_digits(digits,job_id,state);
            let elapsed = start.elapsed().as_millis();
            let job_status = {
                        let st = state.lock().unwrap();
                        st.jobs.get(job_id).map(|job| job.status.clone())
                    };
                    if let Some(status) = job_status {
                        if status == "canceled" {
                            return true; 
                        }
                    }
            respond_json(
                stream,
                meta,
                json!({"digits": digits, "pi": pi, "elapsed_ms": elapsed}),
                job_id,state
            );
            true
        }
        "/mandelbrot" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
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
                error400(stream_clone(stream), "invalid size or max_iter", meta, job_id,state);
                return true;
            }
            let (iters, elapsed, pgm_written) =
                mandelbrot(width, height, max_iter, qmap.get("file"),job_id,state);
            let job_status = {
                        let st = state.lock().unwrap();
                        st.jobs.get(job_id).map(|job| job.status.clone())
                    };
            if let Some(status) = job_status {
                        if status == "canceled" {
                            return true; 
                        }
                    }
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
                job_id,state
            );
            true
        }
        "/matrixmul" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            let size = qmap
                .get("size")
                .or_else(|| qmap.get("n"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(100);
            if size == 0 || size > 600 {
                error400(stream_clone(stream), "size must be between 1 and 600", meta, job_id,state);
                return true;
            }
            let seed = qmap
                .get("seed")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(42);
            let start = Instant::now();
            let hash = matrix_multiply_hash(size, seed,job_id,state);
            let elapsed = start.elapsed().as_millis();
            let job_status = {
                        let st = state.lock().unwrap();
                        st.jobs.get(job_id).map(|job| job.status.clone())
                    };
            if let Some(status) = job_status {
                        if status == "canceled" {
                            return true; 
                        }
                    }
            respond_json(
                stream,
                meta,
                json!({
                    "size": size,
                    "seed": seed,
                    "result_sha256": hash,
                    "elapsed_ms": elapsed
                }),
                job_id,state
            );
            true
        }
        "/sortfile" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(path) if !path.is_empty() => path,
                _ => {
                    error400(stream_clone(stream), "missing 'name' parameter", meta, job_id,state);
                    return true;
                }
            };
            let algo = qmap.get("algo").map(String::as_str).unwrap_or("quick");
            if !matches!(algo, "quick" | "merge") {
                error400(stream_clone(stream), "algo must be quick or merge", meta, job_id,state);
                return true;
            }
            let start = Instant::now();
            match sort_file(name, algo,job_id,state) {
                Ok((sorted_path, count)) => {
                    let elapsed = start.elapsed().as_millis();
                    let job_status = {
                        let st = state.lock().unwrap();
                        st.jobs.get(job_id).map(|job| job.status.clone())
                    };
                    if let Some(status) = job_status {
                        if status == "canceled" {
                            return true; 
                        }
                    }
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "file": name,
                            "algo": algo,
                            "sorted_file": sorted_path,
                            "items": count,
                            "elapsed_ms": elapsed
                        }),
                        job_id,state
                    );
                }
                Err(err) => error500(stream_clone(stream), &err, meta, job_id,state),
            }
            true
        }
        "/wordcount" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(name) => match word_count(name,job_id,state) {
                    Ok(stats) => {
                        let job_status = {
                            let st = state.lock().unwrap();
                            st.jobs.get(job_id).map(|job| job.status.clone())
                        };
                        if let Some(status) = job_status {
                            if status == "canceled" {
                                return true; 
                            }
                        }
                        respond_json(stream, meta, stats, job_id,state)},
                    Err(err) => error500(stream_clone(stream), &err, meta, job_id,state),
                },
                None => error400(stream_clone(stream), "missing 'name' parameter", meta, job_id,state),
            }
            true
        }
        "/grep" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(path) => path,
                None => {
                    error400(stream_clone(stream), "missing 'name' parameter", meta, job_id,state);
                    return true;
                }
            };
            let pattern = match qmap.get("pattern") {
                Some(pat) => pat,
                None => {
                    error400(stream_clone(stream), "missing 'pattern'", meta, job_id,state);
                    return true;
                }
            };
            let regex = match Regex::new(pattern) {
                Ok(r) => r,
                Err(err) => {
                    error400(
                        stream_clone(stream),
                        &format!("invalid regex: {}", err),
                        meta,
                        job_id,state
                    );
                    return true;
                }
            };
            match grep_file(name, &regex,job_id,state) {
                Ok(value) => {
                    let job_status = {
                        let st = state.lock().unwrap();
                        st.jobs.get(job_id).map(|job| job.status.clone())
                    };
                    if let Some(status) = job_status {
                        if status == "canceled" {
                            return true; 
                        }
                    }
                    respond_json(stream, meta, value, job_id,state)},
                Err(err) => error500(stream_clone(stream), &err, meta, job_id,state),
            }
            true
        }
        "/compress" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(path) => path,
                None => {
                    error400(stream_clone(stream), "missing 'name' parameter", meta, job_id,state);
                    return true;
                }
            };
            let codec = qmap.get("codec").map(String::as_str).unwrap_or("gzip");
            if !matches!(codec, "gzip" | "xz") {
                error400(stream_clone(stream), "codec must be gzip or xz", meta, job_id,state);
                return true;
            }
            match compress_file(name, codec,job_id,state) {
                Ok(value) => {
                    let job_status = {
                        let st = state.lock().unwrap();
                        st.jobs.get(job_id).map(|job| job.status.clone())
                    };
                    if let Some(status) = job_status {
                        if status == "canceled" {
                            return true; 
                        }
                    }
                    respond_json(stream, meta, value, job_id,state)},
                Err(err) => error500(stream_clone(stream), &err, meta, job_id,state),
            }
            true
        }
        "/hashfile" => {
            { let mut st = state.lock().unwrap();
              st.jobs.get_mut(job_id).map(|job| job.status = "running".to_string());}
            let name = match qmap.get("name").or_else(|| qmap.get("path")) {
                Some(path) => path,
                None => {
                    error400(stream_clone(stream), "missing 'name' parameter", meta, job_id,state);
                    return true;
                }
            };
            let algo = qmap.get("algo").map(String::as_str).unwrap_or("sha256");
            if algo != "sha256" {
                error400(stream_clone(stream), "unsupported algo (only sha256)", meta, job_id,state);
                return true;
            }
            match hash_file(name,job_id,state) {
                Ok(hash) => {
                    let job_status = {
                        let st = state.lock().unwrap();
                        st.jobs.get(job_id).map(|job| job.status.clone())
                    };
                    if let Some(status) = job_status {
                        if status == "canceled" {
                            return true; 
                        }
                    }
                    respond_json(
                    stream,
                    meta,
                    json!({"file": name, "algorithm": "sha256", "digest": hash}),
                    job_id,state
                )},
                Err(err) => error500(stream_clone(stream), &err, meta, job_id,state),
            }
            true
        }
        "/jobs/submit" => {
            println!("{:#?}", qmap);
            let workers_for_command = {
                let st = state.lock().unwrap();
                st.workers_for_command
                
            };
            let id_job_counter = {
                let mut st = state.lock().unwrap(); // mutable porque vamos a incrementar
                let id = st.id_job_counter;          // guardamos el valor actual
                st.id_job_counter += 1;              // incrementamos
                id                                   // esto se devuelve del bloque
            };
            {let mut st = state.lock().unwrap();
            
            
            st.jobs.insert(id_job_counter.to_string(), Job {
                id: id_job_counter.to_string(),
                status: "queued".to_string(),
                error_message: "".to_string(),
                result: Value::Null,
                progress: 0,
                eta_ms: 0,
                /*path_and_args: qmap_to_string(qmap),
                stream: stream_clone(stream),
                request_id: meta.request_id.clone(),
                dispatched_at: Instant::now(),
                state: "queued".to_string(),*/
            });}
            let new_task = qmap.get("task").cloned().unwrap_or_default();
            let new_path = format!("/{}", new_task);
            let senders = {
                    let mut st = state.lock().unwrap();

                    // Obtengo senders primero, como copia de referencia
                    let senders = st.pool_of_workers_for_command.get(&new_path).cloned(); // clonado o con Arc
                    senders
                };
                
                if let Some(senders) = senders {
                    //Obtiene el indice el worker que sigue para asignar
                    
                    
                    let tx = {
                        let mut st = state.lock().unwrap();
                        let idx  = st.counters.get_mut(&new_path).unwrap();
                        
                        
                        //Obtiene el canal del worker para mandar la tarea
                        let tx = &senders[*idx];
                        //Incrementa el indice del siquiente worker
                        *idx = (*idx + 1) % workers_for_command;
                        tx
                    };
                    //Clona el socket y valida si funciona
                    
                    match stream.try_clone() {
                        Ok(stream_clone) => {
                            //Crea la tarea para mandar
                            let task = Task {
                                path_and_args: qmap_to_string(&qmap),
                                stream: stream_clone,
                                request_id: meta.request_id.clone(),
                                dispatched_at: Instant::now(),
                                state: "queued".to_string(),
                                job_id: id_job_counter.to_string(),
                            };
                            //Envia la tarea al worker
                            
                            if tx.send(task).is_err() {
                                error500(stream.try_clone().unwrap(), "Error despachando tarea", meta, "", state);
                            } else {
                                let mut st = state.lock().unwrap();
                                st.record_dispatch(path);
                            }
                        }
                        Err(e) => {
                            error500(
                                stream_clone(stream),
                                &format!("No se pudo clonar el socket: {}", e),
                                meta, "", state
                            );
                        }
                    }
                } else {
                    error404(stream_clone(stream), &qmap_to_string(&qmap), meta, "", state);
                }
            let result = {
                json!({
                    "job_id": id_job_counter.to_string(),
                    "status": "queued".to_string(),

                })
            };
            respond_json(stream, meta, result, job_id,state);
            true
        }
        "/jobs/status" => {
            
            

            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get(qmap.get("id").unwrap_or(&"".to_string())) {
                
                let result = {
                    json!({
                        "job_id": job.id.to_string(),
                        "status": job.status.to_string(),
                        "progress": job.progress,
                        "eta_ms": job.eta_ms

                    })
                };
                
                respond_json(stream, meta, result, job_id,state);
            }
            else {
                error400(stream_clone(stream), "missing 'ID' parameter", meta, job_id,state);
            }
            true
        }
        "/jobs/result" => {
            println!("{:#?}", qmap);
            let job_id_param = qmap.get("id").unwrap_or(&"".to_string()).clone();
            let job_opt = {
                let st = state.lock().unwrap();
                st.jobs.get(&job_id_param).cloned() // ← aquí clonás el Job, no la referencia
            };
            if let Some(job) = job_opt {
                if job.status == "done" {
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "job_id": job.id.to_string(),
                            "status": job.status.to_string(),
                            "result": job.result.clone()
                        }),
                        job_id,
                        state,
                    );
                } else if job.status == "error" {
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "job_id": job.id.to_string(),
                            "status": job.status.to_string(),
                            "error_message": job.error_message.to_string()
                        }),
                        job_id,
                        state,
                    );
                }
            } else {
                error400(stream_clone(stream), "missing 'ID' parameter", meta, job_id, state);
            }           
            true
        }
        "/jobs/cancel" => {
            println!("{:#?}", qmap);
            let job_id_param = qmap.get("id").unwrap_or(&"".to_string()).clone();

            let job_opt = {
                let st = state.lock().unwrap();
                st.jobs.get(&job_id_param).cloned() // ← aquí clonás el Job, no la referencia
            };
            if let Some(job) = job_opt {
                if job.status == "done" || job.status == "error" {
                    respond_json(
                        stream,
                        meta,
                        json!({
                            "job_id": job.id.to_string(),
                            "status": job.status.to_string(),
                            "message": "cannot cancel a completed job"
                        }),
                        job_id,
                        state,
                    );
                } else {
                    
                    
                    
                    cancel_respond_json(
                        stream,
                        meta,
                        json!({
                            "job_id": job_id_param,
                            "status": "canceled".to_string(),
                            "message": "job canceled"
                        }),
                        
                        &job_id_param,
                        state,
                    );
                }
            } else {
                error400(stream_clone(stream), "missing 'ID' parameter", meta, job_id, state);
            }
            
            true
        }
        "/metrics" => {
            let snapshot = {
                let st = state.lock().unwrap();
                json!({
                    "queues": st.queues_snapshot(),
                    "workers": summarize_workers(&st.workers),
                    "latency_ms": st.latency_snapshot()
                })
            };
            respond_json(stream, meta, snapshot, job_id,state);
            true
        }
        _ => false,
    }
}

fn respond_json(stream: &TcpStream, meta: &ResponseMeta, value: Value, job_id: &str,state: &SharedState) {
    let body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    res200_json(stream_clone(stream), &body, meta, job_id, state);
}
fn cancel_respond_json(stream: &TcpStream, meta: &ResponseMeta, value: Value, job_id: &str,state: &SharedState) {
    let body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    cancel_res200_json(stream_clone(stream), &body, meta, job_id, state);
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
fn qmap_to_string(qmap: &HashMap<String, String>) -> String {
    // 1️⃣ Tomar el valor de la clave "task"
    let task = qmap.get("task").cloned().unwrap_or_default();

    // 2️⃣ Filtrar las demás claves y construir key=value
    let params: Vec<String> = qmap.iter()
        .filter(|(k, _)| k != &"task")  // excluye "task"
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();

    // 3️⃣ Unir todo con comas
    if params.is_empty() {
        format!("{}", task)
    } else {
        format!("/{}?{}", task, params.join(","))
    }
}


fn check_prime(n: u128, job_id: &str,state: &SharedState ) -> bool {

    if n < 2 {
        return false;
    }
    if n % 2 == 0 {
        return n == 2;
    }
    let mut d = 3u128;

    while d * d <= n {
        {
            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get_mut(job_id) {
                if job.status == "canceled" {
                    
                    
                    return false; // Termina la comprobación si se ha cancelado
                }
            }
            
        }
        if n % d == 0 {
            return false;
        }
        d += 2;
    }
    true
}

fn factorize(mut n: u128, job_id: &str,state: &SharedState) -> Vec<(u128, u32)> {
    if n < 2 {
        return vec![(n, 1)];
    }
    let mut res = Vec::new();
    let mut d = 2;
    while (d as u128) * (d as u128) <= n {
        {
            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get_mut(job_id) {
                if job.status == "canceled" {
                    
                    
                    return Vec::new(); // coi

                }
            }
            
        }
        let mut cnt = 0;
        while n % d as u128 == 0 {
            n /= d as u128;
            cnt += 1;
        }
        if cnt > 0 {
            res.push((d as u128, cnt));
        }
        d += if d == 2 { 1 } else { 2 };
    }
    if n > 1 {
        res.push((n, 1));
    }
    res
}

fn compute_pi_digits(digits: usize, job_id: &str,state: &SharedState) -> String {
    if digits == 0 {
        return "3".to_string();
    }
    let mut pi = String::from("3.");
    let len = digits * 10 / 3 + 2;
    let mut array = vec![2u32; len];
    let mut nines = 0usize;
    let mut predigit = 0u32;

    for _ in 0..digits {
        {
            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get_mut(job_id) {
                if job.status == "canceled" {
                    
                    
                    return "Cancelado".to_string(); // Termina la comprobación si se ha cancelado
                }
            }
            
        }
        let mut carry = 0u32;
        for j in (0..len).rev() {
            let denominator = 2 * j as u32 + 1;
            let num = array[j] * 10 + carry;
            array[j] = num % denominator;
            carry = (num / denominator) * j as u32;
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
    pi
}

fn mandelbrot(
    width: usize,
    height: usize,
    max_iter: u32,
    file: Option<&String>,
     job_id: &str,state: &SharedState
) -> (Vec<Vec<u32>>, u128, Option<String>) {
    let start = Instant::now();
    let mut grid = vec![vec![0u32; width]; height];
    for y in 0..height {
        for x in 0..width {
            {
                let mut st = state.lock().unwrap();
                if let Some(job) = st.jobs.get_mut(job_id) {
                    if job.status == "canceled" {
                        
                        
                        return (Vec::new(), 0, Some("cancelado".to_string()));

                    }
                }
                
            }
            let cx = (x as f64 / width as f64) * 3.5 - 2.5;
            let cy = (y as f64 / height as f64) * 2.0 - 1.0;
            let mut zx = 0.0f64;
            let mut zy = 0.0f64;
            let mut iter = 0u32;
            while zx * zx + zy * zy <= 4.0 && iter < max_iter {
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
        if let Ok(mut f) = File::create(name) {
            let _ = writeln!(f, "P2\n{} {}\n255", width, height);
            for row in &grid {
                for value in row {
                    let scaled = (*value as u64 * 255 / max_iter.max(1) as u64) as u32;
                    let _ = write!(f, "{} ", scaled);
                }
                let _ = writeln!(f);
            }
            pgm = Some(name.clone());
        }
    }
    (grid, elapsed, pgm)
}

fn matrix_multiply_hash(size: usize, seed: u64, job_id: &str,state: &SharedState) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut a = vec![0f64; size * size];
    let mut b = vec![0f64; size * size];
    for v in &mut a {
        *v = rng.gen_range(0.0..1.0);
    }
    for v in &mut b {
        *v = rng.gen_range(0.0..1.0);
    }
    let mut c = vec![0f64; size * size];
    for i in 0..size {
        {
            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get_mut(job_id) {
                if job.status == "canceled" {
                    
                    
                    return "Cancelado".to_string(); // Termina la comprobación si se ha cancelado
                }
            }
            
        }
        for k in 0..size {
            let aik = a[i * size + k];
            for j in 0..size {
                c[i * size + j] += aik * b[k * size + j];
            }
        }
    }
    let mut hasher = Sha256::new();
    for value in c {
        hasher.update(value.to_le_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn sort_file(path: &str, algo: &str, job_id: &str,state: &SharedState) -> Result<(String, usize), String> {
    let file = File::open(path).map_err(|e| format!("unable to open {}: {}", path, e))?;
    let reader = BufReader::new(file);
    let mut values: Vec<i64> = reader
        .lines()
        .map(|line| {
            line.and_then(|l| {
                l.trim()
                    .parse::<i64>()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })
        })
        .collect::<Result<_, _>>()
        .map_err(|e| format!("unable to parse integers: {}", e))?;
    if values.len() > MAX_SORT_ITEMS {
        return Err(format!(
            "file too large (>{} items) for in-memory sort",
            MAX_SORT_ITEMS
        ));
    }
    match algo {
        "merge" => merge_sort(&mut values),
        _ => quick_sort(&mut values),
    }
    let sorted_path = format!("{}.sorted", path);
    let mut out = File::create(&sorted_path)
        .map_err(|e| format!("unable to create {}: {}", sorted_path, e))?;
    for (idx, value) in values.iter().enumerate() {
        {
            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get_mut(job_id) {
                if job.status == "canceled" {
                    
                    
                    return Ok(("cancelado".to_string(), 0)); // ejemplo: 0 para tamaño nulo

                }
            }
            
        }
        if idx > 0 {
            writeln!(out).map_err(|e| format!("write error: {}", e))?;
        }
        write!(out, "{}", value).map_err(|e| format!("write error: {}", e))?;
    }
    Ok((sorted_path, values.len()))
}

fn merge_sort(values: &mut [i64]) {
    if values.len() <= 1 {
        return;
    }
    let mid = values.len() / 2;
    merge_sort(&mut values[..mid]);
    merge_sort(&mut values[mid..]);
    let mut merged = values.to_vec();
    merge(&values[..mid], &values[mid..], &mut merged);
    values.copy_from_slice(&merged);
}

fn merge(left: &[i64], right: &[i64], out: &mut [i64]) {
    let mut i = 0;
    let mut j = 0;
    let mut k = 0;
    while i < left.len() && j < right.len() {
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
        out[k] = left[i];
        i += 1;
        k += 1;
    }
    while j < right.len() {
        out[k] = right[j];
        j += 1;
        k += 1;
    }
}

fn quick_sort(values: &mut [i64]) {
    if values.len() <= 1 {
        return;
    }
    let pivot_index = partition(values);
    let (left, right) = values.split_at_mut(pivot_index);
    quick_sort(left);
    quick_sort(&mut right[1..]);
}

fn partition(values: &mut [i64]) -> usize {
    let len = values.len();
    let pivot_index = len - 1;
    let pivot = values[pivot_index];
    let mut i = 0;
    for j in 0..pivot_index {
        if values[j] <= pivot {
            values.swap(i, j);
            i += 1;
        }
    }
    values.swap(i, pivot_index);
    i
}

fn word_count(path: &str, job_id: &str,state: &SharedState) -> Result<Value, String> {
    let file = File::open(path).map_err(|e| format!("unable to open {}: {}", path, e))?;
    let mut reader = BufReader::new(file);
    let mut bytes = 0usize;
    let mut lines = 0usize;
    let mut words = 0usize;
    let mut buf = String::new();
    loop {
        {
            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get_mut(job_id) {
                if job.status == "canceled" {
                    
                    
                    return Ok(json!({
                            "state": "canceled"
                        }))
                }
            }
            
        }
        buf.clear();
        let n = reader
            .read_line(&mut buf)
            .map_err(|e| format!("read error: {}", e))?;
        if n == 0 {
            break;
        }
        bytes += n;
        lines += 1;
        words += buf.split_whitespace().count();
    }
    Ok(json!({
        "file": path,
        "bytes": bytes,
        "lines": lines,
        "words": words
    }))
}

fn grep_file(path: &str, regex: &Regex, job_id: &str,state: &SharedState) -> Result<Value, String> {
    let file = File::open(path).map_err(|e| format!("unable to open {}: {}", path, e))?;
    let reader = BufReader::new(file);
    let mut matches = 0usize;
    let mut first_lines = Vec::new();
    for line in reader.lines() {
        {
            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get_mut(job_id) {
                if job.status == "canceled" {
                    
                    
                    return Ok(json!({
                            "state": "canceled"
                        }))
                }
            }
            
        }
        let line = line.map_err(|e| format!("read error: {}", e))?;
        if regex.is_match(&line) {
            matches += 1;
            if first_lines.len() < 10 {
                first_lines.push(line);
            }
        }
    }
    Ok(json!({
        "file": path,
        "pattern": regex.as_str(),
        "matches": matches,
        "sample": first_lines
    }))
}

fn compress_file(path: &str, codec: &str, job_id: &str,state: &SharedState) -> Result<Value, String> {
    let mut input = File::open(path).map_err(|e| format!("unable to open {}: {}", path, e))?;
    let mut contents = Vec::new();
    input
        .read_to_end(&mut contents)
        .map_err(|e| format!("read error: {}", e))?;
    let output_path = match codec {
        "xz" => format!("{}.xz", path),
        _ => format!("{}.gz", path),
    };
    let output_file = File::create(&output_path)
        .map_err(|e| format!("unable to create {}: {}", output_path, e))?;
    match codec {
        "xz" => {
            let mut encoder = XzEncoder::new(output_file, 6);
            encoder
                .write_all(&contents)
                .map_err(|e| format!("xz write error: {}", e))?;
            encoder
                .finish()
                .map_err(|e| format!("xz finish error: {}", e))?;
        }
        _ => {
            let mut encoder = GzEncoder::new(output_file, Compression::default());
            encoder
                .write_all(&contents)
                .map_err(|e| format!("gzip write error: {}", e))?;
            encoder
                .finish()
                .map_err(|e| format!("gzip finish error: {}", e))?;
        }
    }
    Ok(json!({
        "file": path,
        "codec": codec,
        "output": output_path,
        "bytes_in": contents.len(),
        "bytes_out": fs::metadata(&output_path)
            .map(|m| m.len())
            .unwrap_or(0)
    }))
}

fn hash_file(path: &str, job_id: &str,state: &SharedState) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("unable to open {}: {}", path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        {
            let mut st = state.lock().unwrap();
            if let Some(job) = st.jobs.get_mut(job_id) {
                if job.status == "canceled" {
                    
                    
                    return Ok("cancelado".to_string()); 
                }
            }
            
        }
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("read error: {}", e))?;
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
