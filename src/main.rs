mod control;
mod errors;
mod handlers;
use control::{WorkerInfo, new_state, Task};
use handlers::handle_command;

use std::collections::HashMap;
use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Instant;

use errors::{ResponseMeta, error400, error404, error500, res200, res200_json};

use chrono::Utc;
use serde_json::json;
use urlencoding::decode;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> String {
    let id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("req-{}", id)
}



fn main() -> io::Result<()> {
    // Estado compartido
    let state = new_state();



    let listener = TcpListener::bind("127.0.0.1:8080")?;
    println!("Servidor iniciado en http://127.0.0.1:8080");

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
        "/jobs/submit",
        "/jobs/status",
        "/jobs/result",
        "/jobs/cancel",
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

                // --- Helper local: parsea "a=b&c=d" haciendo también + -> ' ' ---
                let parse_query = |qs: &str| -> std::collections::HashMap<String, String> {
                    let mut m = std::collections::HashMap::new();
                    for pair in qs.split('&') {
                        if pair.is_empty() {
                            continue;
                        }
                        let mut it = pair.splitn(2, '=');

                        // valores crudos
                        let k0 = it.next().unwrap_or("");
                        let v0 = it.next().unwrap_or("");

                        // '+' => espacio (form-urlencoded)
                        let k_fixed = k0.replace('+', " ");
                        let v_fixed = v0.replace('+', " ");

                        // decodifica %xx sin violar el borrow checker
                        let k = match decode(&k_fixed) {
                            Ok(cow) => cow.into_owned(),
                            Err(_) => k_fixed,
                        };
                        let v = match decode(&v_fixed) {
                            Ok(cow) => cow.into_owned(),
                            Err(_) => v_fixed,
                        };

                        m.insert(k, v);
                    }
                    m
                };

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

                    let meta = ResponseMeta::new(task.request_id.clone(), worker_label.clone());
                    println!(
                        "Worker {:?} manejando comando: {}",
                        tid, task.path_and_args
                    );
                    let handled = handle_command(&path, &qmap, &task.stream, &meta, &state_clone, &task.job_id);

                    if !handled {
                        let message =
                            format!("Ejecutando {} en el worker {:?}", &task.path_and_args, tid);
                        res200(task.stream.try_clone().unwrap(), &message, &meta);
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
                let main_meta =
                    ResponseMeta::new(request_id.clone(), format!("{}:main", std::process::id()));
                // Datos de la solucitud
                let mut data = [0; 1024];
                // Recorre la solicitud y la guarda en el data
                let n = match stream.read(&mut data) {
                    Ok(n) if n > 0 => n,
                    _ => {
                        error400(stream, "Bad request", &main_meta);
                        continue;
                    }
                };
                println!("Nuevo cliente conectado: {:?}", stream.peer_addr()?);
                //Convierte la solicitud en string
                let request = String::from_utf8_lossy(&data[..n]);
                //Lee la primera linea de la solicitud que es donde se encuentran los datos
                let request_first_line = request.lines().next().unwrap_or("");
                //Separa la primera linea para obtener cada dato en una lista
                let components: Vec<&str> = request_first_line.split_whitespace().collect();
                //Se verifica que tenga los datos requeridos
                if components.len() < 2 {
                    error400(stream, "Bad request", &main_meta);
                    continue;
                }
                //Almacena la ruta del cliente solicitada
                let path_and_args = components[1];
                let path = path_and_args.splitn(2, '?').next().unwrap_or("");

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

                

                // Verifica el comando existe en el pool de workers por comando
                let senders = {
                    let mut st = state.lock().unwrap();

                    // Obtengo senders primero, como copia de referencia
                    let senders = st.pool_of_workers_for_command.get(path).cloned(); // clonado o con Arc
                    senders
                };
                if let Some(senders) = senders {
                    //Obtiene el indice el worker que sigue para asignar
                    
                    
                    let tx = {
                        let mut st = state.lock().unwrap();
                        let idx  = st.counters.get_mut(path).unwrap();
                        
                        
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
                                path_and_args: path_and_args.to_string(),
                                stream: stream_clone,
                                request_id: request_id.clone(),
                                dispatched_at: Instant::now(),
                                state: "queued".to_string(),
                                job_id: "".to_string(),
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
                    error404(stream, path_and_args, &main_meta);
                }
            }
            Err(e) => {
                eprintln!("Error en la conexión: {}", e);
            }
        }
    }

    Ok(())
}
