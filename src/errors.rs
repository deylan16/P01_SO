use std::net::TcpStream;
use std::io::Write;

/// Envía una respuesta HTTP con código, mensaje y cuerpo
fn send_response(mut stream: TcpStream, status_line: &str, body: &str) {
    let response = format!(
        "{status_line}\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\
        Content-Length: {}\r\n\
        Connection: close\r\n\
        \r\n\
        {}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

/// 400 - Bad Request
pub fn error400(stream: TcpStream, explain: &str) {
    send_response(stream, "HTTP/1.0 400 Bad Request", explain);
}

/// 404 - Not Found
pub fn error404(stream: TcpStream, explain: &str) {
    send_response(stream, "HTTP/1.0 404 Not Found", explain);
}

/// 409 - Conflict
pub fn error409(stream: TcpStream, explain: &str) {
    send_response(stream, "HTTP/1.0 409 Conflict", explain);
}

/// 429 - Too Many Requests
pub fn error429(stream: TcpStream, explain: &str) {
    send_response(stream, "HTTP/1.0 429 Too Many Requests", explain);
}

/// 500 - Internal Server Error
pub fn error500(stream: TcpStream, explain: &str) {
    send_response(stream, "HTTP/1.0 500 Internal Server Error", explain);
}

/// 503 - Service Unavailable
pub fn error503(stream: TcpStream, explain: &str) {
    send_response(stream, "HTTP/1.0 503 Service Unavailable", explain);
}

/// 200 - OK
pub fn res200(stream: TcpStream, explain: &str) {
    send_response(stream, "HTTP/1.0 200 OK", explain);
}
