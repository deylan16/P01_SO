mod errors;

use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};

use std::io;

use errors::{error400,error404,error409,error429,error500,error503,res200};

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    println!("Servidor iniciado en http://127.0.0.1:8080");

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
                res200(stream,"GOD conection");
            }
            Err(e) => {
                eprintln!("Error en la conexión: {}", e);
            }
        }
    }

    Ok(())
}


