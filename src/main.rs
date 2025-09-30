
mod state;
mod router;
mod handlers;

use std::net::{TcpListener, TcpStream};
use state::{new_state, WorkerInfo};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Sender};
use std::thread;
use router::route;
use std::io;
use std::collections::HashMap;

struct Task {
    path_and_args: String,
    stream: TcpStream,
}

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    println!("Servidor iniciado en http://127.0.0.1:8080");

    let state = new_state();

    let commands = vec![
         "/help",
    ];

    let thread_pool = 4;
    let mut pools: HashMap<&str, Vec<Sender<Task>>> = HashMap::new();
    let mut counters: HashMap<&str, usize> = HashMap::new();

    for &cmd in &commands {
        let mut senders = Vec::with_capacity(thread_pool);
        for _ in 0..thread_pool {
            let (tx, rx) = mpsc::channel::<Task>();
            let state_clone = state.clone();
            let cmd_string = cmd.to_string();
    
            thread::spawn(move || {
                // 1) Registrar este worker
                let tid = thread::current().id();
                {
                    let mut st = state_clone.lock().unwrap();
                    st.workers.push(WorkerInfo {
                        command: cmd_string.clone(),
                        thread_id: format!("{:?}", tid),
                        busy: false,
                    });
                }
    
                for mut task in rx {
                    // Marcar busy = true
                    {
                        let mut st = state_clone.lock().unwrap();
                        if let Some(w) = st.workers.iter_mut().find(|w| w.thread_id == format!("{:?}", tid)) {
                            w.busy = true;
                        }
                    }
                    // Procesar
                    let response = route(&task.path_and_args, state_clone.clone());
                    let _ = task.stream.write_all(response.as_bytes());
    
                    // Marcar busy = false
                    {
                        let mut st = state_clone.lock().unwrap();
                        if let Some(w) = st.workers.iter_mut().find(|w| w.thread_id == format!("{:?}", tid)) {
                            w.busy = false;
                        }
                    }
                }
            });
    
            senders.push(tx);
        }
        pools.insert(cmd, senders);
        counters.insert(cmd, 0);
    }


    // Bucle que espera conexiones
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                // Datos de la solucitud
                let mut data = [0; 1024];

                let n = match stream.read(&mut data) {
                    Ok(n) if n > 0 => n,
                    _ => { error400(stream); continue; }
                };

                println!("Nuevo cliente conectado: {:?}", stream.peer_addr()?);

                let request = String::from_utf8_lossy(&data[..n]);
                let request_first_line = request.lines().next().unwrap_or("");
                let components: Vec<&str> = request_first_line.split_whitespace().collect();
                if components.len() < 2 {
                    error400(stream);
                    continue;
                }
                let path_and_args = components[1];
                let path = path_and_args.splitn(2, '?').next().unwrap_or("");

                // Actualizar contador global
                {
                    let mut st = state.lock().unwrap();
                    st.total_connections += 1;
                }

                // Despachar a pool o responder 404
                if let Some(senders) = pools.get(path) {
                    let idx = counters.get_mut(path).unwrap();
                    let tx = &senders[*idx];
                    *idx = (*idx + 1) % thread_pool;
                    match stream.try_clone() {
                        Ok(stream_clone) => {
                            let task = Task {
                                path_and_args: path_and_args.to_string(),
                                stream: stream_clone,
                            };
                            if tx.send(task).is_err() {
                                error500(stream, "Error despachando tarea");
                            }
                        }
                        Err(e) => {
                            error500(stream, &format!("No se pudo clonar el socket: {}", e));
                        }
                    }
                } else {
                    error404(stream, path_and_args);
                }



                // acá podrías leer/escribir al cliente
            }
            Err(e) => {
                eprintln!("Error en la conexión: {}", e);
            }
        }
    }

    Ok(())
}

fn error400(mut stream: TcpStream) {
    let resp = "HTTP/1.0 400 Bad Request\r\n\r\nBad Request";
    let _ = stream.write(resp.as_bytes());
}
fn error500(mut stream: TcpStream, msg: &str) {
    let resp = format!(
        "HTTP/1.0 500 Internal Server Error\r\n\r\n{}",
        msg
    );
    let _ = stream.write_all(resp.as_bytes());
}
fn error404(mut stream: TcpStream, route: &str) {
    let resp = format!(
        "HTTP/1.0 404 Not Found\r\n\r\nRuta no implementada: {}",
        route
    );
    let _ = stream.write_all(resp.as_bytes());
}