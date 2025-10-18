use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};


pub fn error400(mut stream: TcpStream, explain:&str) {
    let resp = format!(
        "HTTP/1.0 400 Bad Request\r\n\r\n{}",
        explain
    );
    let _ = stream.write(resp.as_bytes());
}
pub fn error404(mut stream: TcpStream, explain:&str) {
    let resp = format!(
        "HTTP/1.0 404 Not Found\r\n\r\n{}",
        explain
    );
    let _ = stream.write(resp.as_bytes());
}
pub fn error409(mut stream: TcpStream, explain:&str) {
    let resp = format!(
        "HTTP/1.0 409 Conflict\r\n\r\n{}",
        explain
    );
    let _ = stream.write(resp.as_bytes());
}
pub fn error429(mut stream: TcpStream, explain:&str) {
    let resp = format!(
        "HTTP/1.0 429 Too Many Request\r\n\r\n{}",
        explain
    );
    let _ = stream.write(resp.as_bytes());
}
pub fn error500(mut stream: TcpStream, explain:&str) {
    let resp = format!(
        "HTTP/1.0 500 Internal Server Error\r\n\r\n{}",
        explain
    );
    let _ = stream.write(resp.as_bytes());
}
pub fn error503(mut stream: TcpStream, explain:&str) {
    let resp = format!(
        "HTTP/1.0 503 Service Unavailable\r\n\r\n{}",
        explain
    );
    let _ = stream.write(resp.as_bytes());
}
pub fn res200(mut stream: TcpStream, explain:&str) {
    let resp = format!(
        "HTTP/1.0 200 Good Conection\r\n\r\n\r\n\r\n{}",
        explain
    );
    let _ = stream.write(resp.as_bytes());
}