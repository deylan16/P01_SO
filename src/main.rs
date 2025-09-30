use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};

use std::io;

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    println!("Servidor iniciado en http://127.0.0.1:8080");

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