mod control;
mod errors;
use control::{new_state, WorkerInfo};
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Sender};
use std::io;
use std::collections::HashMap;
use std::thread;
use errors::{error400,error404,error409,error429,error500,error503,res200};
//Estrucuta para cada worker
struct Task {
    path_and_args: String,
    stream: TcpStream,
}
fn main() -> io::Result<()> {
    // Estado compartido
    let state = new_state();

    let listener = TcpListener::bind("127.0.0.1:8080")?;
    println!("Servidor iniciado en http://127.0.0.1:8080");

    // Lista de comandos imlementados
    let commands = vec![
        "/fibonacci", "/createfile", "/deletefile", "/status", "/reverse", "/toupper",
        "/random", "/timestamp", "/hash", "/simulate", "/sleep", "/loadtest", "/help",

        "/isprime","/factor","/pi","/mandelbrot","/matrixmul",
        
        "/sortfile","/wordcount","/grep","/compress","/hashfile",
        
        "/metrics"
    ];
    //Cantidad de hilos por comando
    let workers_for_command = 2;

    //Almacena los workers de casa comando  fibonacci-> [work1,work2,...]
    let mut pool_of_workers_for_command: HashMap<&str, Vec<Sender<Task>>> = HashMap::new();

    let mut counters: HashMap<&str, usize> = HashMap::new();


    for &cmd in &commands {
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
                {
                    let mut st = state_clone.lock().unwrap();
                    st.workers.push(WorkerInfo {
                        command: cmd_string.clone(),
                        thread_id: format!("{:?}", tid),
                        busy: false,
                    });
                }
                //El for pasa escuchando si entra una nueva tarea al canal del worker
                for mut task in rx {
                    //El worker se pone como ocupado
                    {
                        let mut st = state_clone.lock().unwrap();
                        if let Some(w) = st.workers.iter_mut().find(|w| w.thread_id == format!("{:?}", tid)) {
                            w.busy = true;
                        }
                    }
                    // Se realiza el proceso
                    let message = format!("Ejecutando {} en el worker {:?}", &task.path_and_args, tid);
                    // Enviar respuesta al cliente
                    res200(task.stream.try_clone().unwrap(), &message);
                    
    
                    //El worker se pone como disponible
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
        pool_of_workers_for_command.insert(cmd, senders);
        counters.insert(cmd, 0);

    }
    // Bucle que espera conexiones
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                // Datos de la solucitud
                let mut data = [0; 1024];
                // Recorre la solicitud y la guarda en el data
                let n = match stream.read(&mut data) {
                    Ok(n) if n > 0 => n,
                    _ => { error400(stream,"Bad request"); continue; }
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
                    error400(stream,"Bad request");
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

                // Verifica el comando existe en el pool de workers por comando
                if let Some(senders) = pool_of_workers_for_command.get(path) {
                    //Obtiene el indice el worker que sigue para asignar
                    let idx = counters.get_mut(path).unwrap();
                    //Obtiene el canal del worker para mandar la tarea
                    let tx = &senders[*idx];
                    //Incrementa el indice del siquiente worker
                    *idx = (*idx + 1) % workers_for_command;
                    //Clona el socket y valida si funciona
                    match stream.try_clone() {
                        Ok(stream_clone) => {
                            //Crea la tarea para mandar
                            let task = Task {
                                path_and_args: path_and_args.to_string(),
                                stream: stream_clone,
                            };
                            //Envia la tarea al worker
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

                
            }
            Err(e) => {
                eprintln!("Error en la conexión: {}", e);
            }
        }
    }

    Ok(())
}


