mod control;
mod errors;
mod handlers;
use control::{new_state, WorkerInfo, Task,save_jobs,load_jobs,Job};
use handlers::handle_command;

use std::collections::HashMap;
use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use errors::{error400, error404, error500, error503_json, res200, res200_json, ResponseMeta};

use chrono::Utc;
use serde_json::json;
use urlencoding::decode;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
const MAX_REQUEST_SIZE: usize = 16 * 1024;

fn next_request_id() -> String {
    let id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("req-{}", id)
}

fn resolve_bind_addr() -> String {
    let mut bind = std::env::var("P01_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--bind" {
            if let Some(value) = args.next() {
                bind = value;
            }
        } else if let Some(value) = arg.strip_prefix("--bind=") {
            bind = value.to_string();
        }
    }
    bind
}

fn main() -> io::Result<()> {
    let loaded_jobs = load_jobs();
    // Estado compartido
    let state = new_state();
    {
        let mut st = state.lock().unwrap();
        let mut last_id = 0u64;
        for job in loaded_jobs {
            // Intentar convertir el ID (string) a número
            if let Ok(num_id) = job.id.parse::<u64>() {
                if num_id > last_id {
                    last_id = num_id;
                }
            }
            st.jobs.insert(job.id.clone(), job);
        }
        // Guardar el siguiente ID disponible
        st.id_job_counter = (last_id + 1) as usize;
    }

    ctrlc::set_handler({
        let state = state.clone();
        move || {
            let st = state.lock().unwrap();
            let jobs: Vec<Job> = st.jobs.values().cloned().collect();
            save_jobs(&jobs);
            println!("Jobs guardados antes del cierre.");
             std::process::exit(0);
        }
    }).expect("Error configurando Ctrl-C handler");

    let bind_addr = resolve_bind_addr();
    let listener = TcpListener::bind(&bind_addr)?;
    println!("Servidor iniciado en http://{}", bind_addr);

    // Lista de comandos imlementados
    let commands = vec![
        "/fibonacci",
        "/createfile",
        "/deletefile",
        "/status",
        "/reverse",
        "/toupper",
        "/random",
        "/timestamp",
        "/hash",
        "/simulate",
        "/sleep",
        "/loadtest",
        "/help",
        "/isprime",
        "/factor",
        "/pi",
        "/mandelbrot",
        "/matrixmul",
        "/sortfile",
        "/wordcount",
        "/grep",
        "/compress",
        "/hashfile",
        "/metrics",
    ];
    //Cantidad de hilos por comando
    let workers_for_command = {
        let st = state.lock().unwrap();
        st.workers_for_command
    };

    //Almacena los workers de casa comando  fibonacci-> [work1,work2,...]
    //let mut pool_of_workers_for_command: HashMap<&str, Vec<Sender<Task>>> = HashMap::new();

    //let mut counters: HashMap<&str, usize> = HashMap::new();

    for &cmd in &commands {
        {
            let mut st = state.lock().unwrap();
            st.ensure_command(cmd);
        }
        let mut senders = Vec::with_capacity(workers_for_command);

        //Para cada comando crea la siguientes acciones workers_for_command veces
        for _ in 0..workers_for_command {
            //Crea un canal para cada worker
            let (tx, rx) = mpsc::channel::<Task>();
            let state_clone = state.clone();
            let cmd_string = cmd.to_string();
            thread::spawn(move || {
                // Se almacena el nuevo worker
                
                let tid = thread::current().id();
                let worker_label = format!("{}:{:?}", std::process::id(), tid);
                {
                    let mut st = state_clone.lock().unwrap();
                    st.workers.push(WorkerInfo {
                        command: cmd_string.clone(),
                        thread_id: format!("{:?}", tid),
                        busy: false,
                    });
                }

                //El for pasa escuchando si entra una nueva tarea al canal del worker
                for task in rx {
                    println!(
                        "Worker {:?} recibió tarea: {}",
                        tid, task.path_and_args
                    );
                    
                    //El worker se pone como ocupado
                    {
                        let mut st = state_clone.lock().unwrap();
                        if let Some(w) = st
                            .workers
                            .iter_mut()
                            .find(|w| w.thread_id == format!("{:?}", tid))
                        {
                            w.busy = true;
                        }
                    }

                    // Separa path y query de la solicitud original
                    let (path, qmap) = {
                        let mut it = task.path_and_args.splitn(2, '?');
                        let p = it.next().unwrap_or("/");
                        let q = it.next().unwrap_or("");
                        (p.to_string(), parse_query(q))
                    };

                    let mut meta = ResponseMeta::new(task.request_id.clone(), worker_label.clone());
                    if task.suppress_body {
                        meta = meta.for_head();
                    }
                    let mut skip_execution = false;

                    if !task.job_id.is_empty() {
                        {
                            let mut st = state_clone.lock().unwrap();
                            if let Some(job) = st.jobs.get_mut(&task.job_id) {
                                if job.status == "cancelled" {
                                    skip_execution = true;
                                } else {
                                    job.status = "running".to_string();
                                }
                            }
                        }
                        meta = meta.with_job(state_clone.clone(), task.job_id.clone());
                    }

                    if !skip_execution {
                        let task_timeout_ms = {
                            let st = state_clone.lock().unwrap();
                            st.task_timeout_ms
                        };
                        let deadline = Instant::now() + Duration::from_millis(task_timeout_ms);
                        let handled =
                            handle_command(&path, &qmap, &task.stream, &meta, &state_clone, deadline);

                        if !handled {
                            let message = format!(
                                "Ejecutando {} en el worker {:?}",
                                &task.path_and_args,
                                tid
                            );
                            res200(task.stream.try_clone().unwrap(), &message, &meta);
                        }
                    }

                    let elapsed = task.dispatched_at.elapsed().as_millis() as u128;
                    {
                        let mut st = state_clone.lock().unwrap();
                        st.record_completion(&path, elapsed);
                    }

                    // ------------------------------------------------------------------

                    //El worker se pone como disponible
                    {
                        let mut st = state_clone.lock().unwrap();
                        if let Some(w) = st
                            .workers
                            .iter_mut()
                            .find(|w| w.thread_id == format!("{:?}", tid))
                        {
                            w.busy = false;
                        }
                    }
                } // <-- cierre del bucle 'worker
            });
            senders.push(tx);
        }
        {
            let mut st = state.lock().unwrap();
            st.pool_of_workers_for_command.insert(cmd.to_string(), senders);
            st.counters.insert(cmd.to_string(), 0);
        }

        
    }

    // Bucle que espera conexiones
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let request_id = next_request_id();
                let mut main_meta =
                    ResponseMeta::new(request_id.clone(), format!("{}:main", std::process::id()));

                let request_buffer = match read_http_request(&mut stream) {
                    Ok(buf) => buf,
                    Err(e) => {
                        eprintln!("Error leyendo request: {}", e);
                        error400(stream, "Bad request", &main_meta);
                        continue;
                    }
                };

                println!("Nuevo cliente conectado: {:?}", stream.peer_addr()?);

                let header_end = header_length(&request_buffer);
                let request = match String::from_utf8(request_buffer[..header_end].to_vec()) {
                    Ok(s) => s,
                    Err(_) => {
                        error400(stream, "Invalid UTF-8 in request", &main_meta);
                        continue;
                    }
                };

                let request_first_line = request.lines().next().unwrap_or("");
                let components: Vec<&str> = request_first_line.split_whitespace().collect();
                if components.len() < 2 {
                    error400(stream, "Bad request", &main_meta);
                    continue;
                }

                let suppress_body = match components[0] {
                    method if method.eq_ignore_ascii_case("GET") => false,
                    method if method.eq_ignore_ascii_case("HEAD") => true,
                    _ => {
                        error400(stream, "Unsupported HTTP method", &main_meta);
                        continue;
                    }
                };

                if suppress_body {
                    main_meta = main_meta.for_head();
                }

                let path_and_args = components[1];
                let mut path_split = path_and_args.splitn(2, '?');
                let path = path_split.next().unwrap_or("");
                let query_str = path_split.next().unwrap_or("");

                // Actualizar contador global
                {
                    let mut st = state.lock().unwrap();
                    st.total_connections += 1;
                }

                //  /status responde JSON aquí ---
                if path == "/status" {
                    let st = state.lock().unwrap();
                    let uptime = (Utc::now() - st.start_time).num_seconds();

                    let body = json!({
                        "uptime_seconds": uptime,
                        "total_connections": st.total_connections,
                        "pid": st.pid,
                        "queues": st.queues_snapshot(),
                        "latency_ms": st.latency_snapshot(),
                        "workers": st.workers.iter().map(|w| {
                            json!({"command": w.command, "thread_id": w.thread_id, "busy": w.busy})
                        }).collect::<Vec<_>>()
                    })
                    .to_string();

                    res200_json(stream, &body, &main_meta);
                    continue; // importante: no encolar esta solicitud
                }

                if path.starts_with("/jobs/") {
                    let qmap = parse_query(query_str);
                    let task_timeout_ms = {
                        let st = state.lock().unwrap();
                        st.task_timeout_ms
                    };
                    let deadline = Instant::now() + Duration::from_millis(task_timeout_ms);
                    if !handle_command(path, &qmap, &stream, &main_meta, &state, deadline) {
                        error404(stream, path_and_args, &main_meta);
                    }
                    continue;
                }

                let backpressure = {
                    let st = state.lock().unwrap();
                    st.command_stats.get(path).and_then(|stats| {
                        if stats.in_flight >= st.max_in_flight_per_command {
                            Some((st.retry_after_ms, stats.in_flight, st.max_in_flight_per_command))
                        } else {
                            None
                        }
                    })
                };

                if let Some((retry_after_ms, current, limit)) = backpressure {
                    let body = json!({
                        "error": "backpressure",
                        "message": format!("{} saturated: {} in-flight (limit {})", path, current, limit),
                        "retry_after_ms": retry_after_ms
                    })
                    .to_string();
                    let retry_seconds = ((retry_after_ms + 999) / 1000).max(1);
                    let meta_retry = main_meta
                        .clone()
                        .with_header("Retry-After", retry_seconds.to_string());
                    error503_json(stream, &body, &meta_retry);
                    continue;
                }

                let (tx, path_known) = {
                    let mut st = state.lock().unwrap();
                    if let Some(senders_vec) = st.pool_of_workers_for_command.get(path) {
                        let senders = senders_vec.clone();
                        if senders.is_empty() {
                            (None, true)
                        } else if let Some(counter) = st.counters.get_mut(path) {
                            let idx = *counter;
                            println!(
                                "Current worker index for command {}: {}",
                                path, idx
                            );
                            *counter = (idx + 1) % senders.len();
                            println!(
                                "Dispatching to worker index {} for command {}",
                                idx, path
                            );
                            (Some(senders[idx].clone()), true)
                        } else {
                            (None, true)
                        }
                    } else {
                        (None, false)
                    }
                };

                if let Some(tx) = tx {
                    
                    //Clona el socket y valida si funciona
                    
                    match stream.try_clone() {
                        Ok(stream_clone) => {
                            //Crea la tarea para mandar
                            let task = Task {
                                path_and_args: path_and_args.to_string(),
                                stream: stream_clone,
                                request_id: request_id.clone(),
                                dispatched_at: Instant::now(),
                                state: "queued".to_string(),
                                job_id: "".to_string(),
                                suppress_body,
                            };
                            //Envia la tarea al worker
                            
                            if tx.send(task).is_err() {
                                error500(stream, "Error despachando tarea", &main_meta);
                            } else {
                                let mut st = state.lock().unwrap();
                                st.record_dispatch(path);
                            }
                        }
                        Err(e) => {
                            error500(
                                stream,
                                &format!("No se pudo clonar el socket: {}", e),
                                &main_meta,
                            );
                        }
                    }
                } else {
                    if path_known {
                        error500(stream, "No workers available", &main_meta);
                    } else {
                        error404(stream, path_and_args, &main_meta);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error en la conexión: {}", e);
            }
        }
    }

    Ok(())
}

fn parse_query(qs: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for pair in qs.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k0 = it.next().unwrap_or("");
        let v0 = it.next().unwrap_or("");
        let k_fixed = k0.replace('+', " ");
        let v_fixed = v0.replace('+', " ");
        let k = decode(&k_fixed).map(|c| c.into_owned()).unwrap_or(k_fixed);
        let v = decode(&v_fixed).map(|c| c.into_owned()).unwrap_or(v_fixed);
        m.insert(k, v);
    }
    m
}

fn read_http_request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if headers_complete(&buffer) {
            break;
        }
        if buffer.len() >= MAX_REQUEST_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Request header too large",
            ));
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
    }

    if buffer.is_empty() {
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Empty request",
        ))
    } else if headers_complete(&buffer) {
        Ok(buffer)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Request header too large",
        ))
    }
}

fn headers_complete(buffer: &[u8]) -> bool {
    buffer.windows(4).any(|w| w == b"\r\n\r\n")
}

fn header_length(buffer: &[u8]) -> usize {
    buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|idx| idx + 4)
        .unwrap_or(buffer.len())
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::{Duration, Instant};
    use std::io::{Write, Read};
    use std::env;

    // Helper: create a connected pair (client, server) using a short-lived listener.
    fn connect_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind failed");
        let addr = listener.local_addr().unwrap();
        // spawn acceptor
        let accept_thread = thread::spawn(move || {
            let (socket, _) = listener.accept().expect("accept failed");
            socket
        });
        // connect client
        let client = TcpStream::connect(addr).expect("connect failed");
        let server = accept_thread.join().expect("accept thread panicked");
        (client, server)
    }

    #[test]
    fn test_next_request_id_increments_and_format() {
        let id1 = next_request_id();
        let id2 = next_request_id();
        assert!(id1.starts_with("req-"));
        assert!(id2.starts_with("req-"));

        // extract numeric suffix and ensure increasing (best-effort)
        let n1 = id1.trim_start_matches("req-").parse::<u64>().ok();
        let n2 = id2.trim_start_matches("req-").parse::<u64>().ok();
        if let (Some(a), Some(b)) = (n1, n2) {
            assert!(b > a);
        }
    }


    #[test]
    fn test_parse_query_basic_and_encoded() {
        let q = "a=1&b=hello+world&c=%7Bjson%7D";
        let m = parse_query(q);
        assert_eq!(m.get("a").map(String::as_str), Some("1"));
        assert_eq!(m.get("b").map(String::as_str), Some("hello world"));
        assert_eq!(m.get("c").map(String::as_str), Some("{json}"));

        // empty and trailing ampersand
        let q2 = "x=1&&y=2&";
        let m2 = parse_query(q2);
        assert_eq!(m2.get("x").map(String::as_str), Some("1"));
        assert_eq!(m2.get("y").map(String::as_str), Some("2"));
    }

    #[test]
    fn test_headers_complete_and_header_length() {
        let h = b"GET / HTTP/1.1\r\nHost: local\r\n\r\nBody";
        assert!(headers_complete(h));
        let len = header_length(h);
        // header_length should point to end of headers (index after \r\n\r\n)
        assert_eq!(len,  (b"GET / HTTP/1.1\r\nHost: local\r\n\r\n".len()));
        // Without terminating sequence
        let h2 = b"GET / HTTP/1.1\r\nHost: local\r\n";
        assert!(!headers_complete(h2));
        let len2 = header_length(h2);
        assert_eq!(len2, h2.len());
    }

    #[test]
    fn test_read_http_request_success_with_headers() {
        let (mut client, mut server) = connect_pair();
        // Write a simple request with headers terminator
        let req = b"GET /foo?x=1 HTTP/1.1\r\nHost: test\r\n\r\n";
        client.write_all(req).expect("write failed");
        // Ensure server side is ready to read
        server.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let buf = read_http_request(&mut server).expect("should read request");
        // header_length should be <= buf.len()
        let hlen = header_length(&buf);
        assert!(hlen <= buf.len());
        // The header content should contain GET line
        let header_text = String::from_utf8_lossy(&buf[..hlen]);
        assert!(header_text.contains("GET /foo?x=1"));
    }

    #[test]
    fn test_read_http_request_empty_stream_returns_eof_error() {
        let (mut client, mut server) = connect_pair();
        // Close client write side so server sees EOF
        client.shutdown(std::net::Shutdown::Both).ok();
        server.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let res = read_http_request(&mut server);
        assert!(res.is_err());
        let err = res.err().unwrap();
        // Expect UnexpectedEof
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn test_read_http_request_large_header_error() {
        let (mut client, mut server) = connect_pair();
        // Build a payload larger than MAX_REQUEST_SIZE without \r\n\r\n
        let large = vec![b'a'; MAX_REQUEST_SIZE + 10];
        client.write_all(&large).expect("write failed");
        // Keep connection open; server should return InvalidData due to size limit
        server.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let res = read_http_request(&mut server);
        assert!(res.is_err());
        let err = res.err().unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_header_length_when_multiple_windows() {
        // Ensure header_length picks first occurrence
        let many = b"HEAD / A\r\nX: 1\r\n\r\nGET / B\r\n\r\n";
        assert_eq!(header_length(many), (b"HEAD / A\r\nX: 1\r\n\r\n".len()));
    }

    // Small utility test: ensure parse_query handles '+' and percent-decoding edgecases
    #[test]
    fn test_parse_query_plus_and_percent_edgecases() {
        let q = "+k+=+%21+%2B+";
        // Interpreted as " k " -> " ! + " maybe strange but ensure no panic and decodes
        let m = parse_query(q);
        // since key becomes " k " and value becomes " ! + " depending on splitting, check map non-panicking
        assert!(!m.is_empty());
    }
}
