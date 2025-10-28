use crate::control::SharedState;
use serde_json::{json, Value};
use std::io::Write;
use std::net::TcpStream;

#[derive(Clone)]
pub struct JobResponseMeta {
    pub state: SharedState,
    pub job_id: String,
}

#[derive(Clone)]
pub struct ResponseMeta {
    pub request_id: String,
    pub worker_pid: String,
    pub extra_headers: Vec<(String, String)>,
    pub job_meta: Option<JobResponseMeta>,
    suppress_body: bool,
}

impl ResponseMeta {
    pub fn new(request_id: impl Into<String>, worker_pid: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            worker_pid: worker_pid.into(),
            extra_headers: Vec::new(),
            job_meta: None,
            suppress_body: false,
        }
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.push((key.into(), value.into()));
        self
    }

    pub fn with_job(mut self, state: SharedState, job_id: impl Into<String>) -> Self {
        self.job_meta = Some(JobResponseMeta {
            state,
            job_id: job_id.into(),
        });
        self
    }

    pub fn for_head(mut self) -> Self {
        self.suppress_body = true;
        self
    }

    pub fn suppress_body(&self) -> bool {
        self.suppress_body
    }
}

/// Envía una respuesta HTTP con código, mensaje y cuerpo
fn send_response(mut stream: TcpStream, status_line: &str, body: &str, meta: &ResponseMeta) {
    if finalize_job_if_needed(status_line, body, meta) {
        return;
    }
    let mut head = format!(
        "{status_line}\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\
        Content-Length: {len}\r\n\
        X-Request-Id: {req}\r\n\
        X-Worker-Pid: {worker}\r\n\
        Connection: close\r\n",
        len = body.len(),
        req = meta.request_id,
        worker = meta.worker_pid
    );
    for (key, value) in &meta.extra_headers {
        head.push_str(&format!("{k}: {v}\r\n", k = key, v = value));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    if !meta.suppress_body() {
        let _ = stream.write_all(body.as_bytes());
    }
}

/// 400 - Bad Request
pub fn error400(stream: TcpStream, explain: &str, meta: &ResponseMeta) -> bool {
    send_response(stream, "HTTP/1.0 400 Bad Request", explain, meta);
    false // siempre devuelve false
}

/// 404 - Not Found
pub fn error404(stream: TcpStream, explain: &str, meta: &ResponseMeta) -> bool {
    send_response(stream, "HTTP/1.0 404 Not Found", explain, meta);
    false // siempre devuelve false
}

/// 409 - Conflict
pub fn error409(stream: TcpStream, explain: &str, meta: &ResponseMeta) -> bool {
    send_response(stream, "HTTP/1.0 409 Conflict", explain, meta);
    false // siempre devuelve false
}

/// 429 - Too Many Requests
pub fn error429(stream: TcpStream, explain: &str, meta: &ResponseMeta)-> bool{
    send_response(stream, "HTTP/1.0 429 Too Many Requests", explain, meta);
    false // siempre devuelve false
}

/// 500 - Internal Server Error
pub fn error500(stream: TcpStream, explain: &str, meta: &ResponseMeta) -> bool{
    send_response(stream, "HTTP/1.0 500 Internal Server Error", explain, meta);
    false // siempre devuelve false

}

/// 503 - Service Unavailable
pub fn error503(stream: TcpStream, explain: &str, meta: &ResponseMeta) {
    send_response(stream, "HTTP/1.0 503 Service Unavailable", explain, meta);
}

/// 200 - OK
pub fn res200(stream: TcpStream, explain: &str, meta: &ResponseMeta) {
    send_response(stream, "HTTP/1.0 200 OK", explain, meta);
}

// responder con content-type configurable
fn send_response_with_ct(
    mut stream: TcpStream,
    status_line: &str,
    content_type: &str,
    body: &str,
    meta: &ResponseMeta,
) {
    if finalize_job_if_needed(status_line, body, meta) {
        return;
    }
    let bytes = body.as_bytes();
    let mut head = format!(
        "{status}\r\nContent-Type: {ct}\r\nContent-Length: {len}\r\nX-Request-Id: {req}\r\nX-Worker-Pid: {worker}\r\nConnection: close\r\n",
        status = status_line,
        ct = content_type,
        len = bytes.len(),
        req = meta.request_id,
        worker = meta.worker_pid,
    );

    for (key, value) in &meta.extra_headers {
        head.push_str(&format!("{k}: {v}\r\n", k = key, v = value));
    }
    head.push_str("\r\n");

    let _ = stream.write_all(head.as_bytes());
    if !meta.suppress_body() {
        let _ = stream.write_all(bytes);
    }
    let _ = stream.flush();
}

// 200 OK con JSON
pub fn res200_json(stream: TcpStream, body: &str, meta: &ResponseMeta) {
    send_response_with_ct(
        stream,
        "HTTP/1.0 200 OK",
        "application/json; charset=utf-8",
        body,
        meta,
    );
}

pub fn error503_json(stream: TcpStream, body: &str, meta: &ResponseMeta) {
    send_response_with_ct(
        stream,
        "HTTP/1.0 503 Service Unavailable",
        "application/json; charset=utf-8",
        body,
        meta,
    );
}

fn finalize_job_if_needed(status_line: &str, body: &str, meta: &ResponseMeta) -> bool {
    let Some(job_meta) = &meta.job_meta else {
        return false;
    };

    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("500")
        .to_string();
    let is_success = status_code.starts_with('2');

    let parsed = serde_json::from_str::<Value>(body).unwrap_or_else(|_| json!({
        "raw": body,
    }));

    let mut st = job_meta.state.lock().unwrap();
    if let Some(job) = st.jobs.get_mut(&job_meta.job_id) {
        if is_success {
            job.status = "done".to_string();
            job.result = parsed;
            job.error_message.clear();
        } else {
            job.status = "failed".to_string();
            job.error_message = body.to_string();
            job.result = Value::Null;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::io::{Read, Write};
    use serde_json::json;

    use crate::control::ServerState; // o ajusta según tu estructura real

    // 🔧 Helper para crear un stream TCP simulado
    fn mock_stream() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    // 🔧 Helper para leer respuesta HTTP
    fn read_response(mut stream: TcpStream) -> String {
        let mut buffer = String::new();
        stream.read_to_string(&mut buffer).unwrap();
        buffer
    }

    #[test]
    fn test_error400_generates_valid_http_response() {
        let (client, server) = mock_stream();
        let meta = ResponseMeta::new("req1", "pid123");

        error400(server, "bad request", &meta);
        let response = read_response(client);

        assert!(response.contains("HTTP/1.0 400 Bad Request"));
        assert!(response.contains("Content-Type: text/plain"));
        assert!(response.contains("bad request"));
    }

    #[test]
    fn test_res200_json_sends_correct_headers() {
        let (client, server) = mock_stream();
        let meta = ResponseMeta::new("req42", "workerA");

        res200_json(server, r#"{"ok":true}"#, &meta);
        let response = read_response(client);

        assert!(response.contains("HTTP/1.0 200 OK"));
        assert!(response.contains("Content-Type: application/json"));
        assert!(response.contains("\"ok\":true"));
    }

    #[test]
    fn test_finalize_job_on_success() {
        // Creamos estado simulado
        let job_id = "1".to_string();
        let mut st = ServerState::default(); // asegúrate de tener un Default para State
        st.jobs.insert(
            job_id.clone(),
            crate::Job {
                id: job_id.clone(),
                status: "running".into(),
                error_message: "".into(),
                result: Value::Null,
                progress: 0,
                eta_ms: 0,
            },
        );

        let state = Arc::new(Mutex::new(st));

        let meta = ResponseMeta::new("req123", "pid999")
            .with_job(state.clone(), job_id.clone());

        // Simular respuesta exitosa
        finalize_job_if_needed("HTTP/1.0 200 OK", r#"{"result":"ok"}"#, &meta);

        let st_guard = state.lock().unwrap();
        let job = st_guard.jobs.get(&job_id).unwrap();
        assert_eq!(job.status, "done");
        assert_eq!(job.result["result"], "ok");
        assert_eq!(job.error_message, "");
    }

    #[test]
    fn test_finalize_job_on_failure() {
        let job_id = "2".to_string();
        let mut st = ServerState::default();
        st.jobs.insert(
            job_id.clone(),
            crate::Job {
                id: job_id.clone(),
                status: "running".into(),
                error_message: "".into(),
                result: Value::Null,
                progress: 0,
                eta_ms: 0,
            },
        );

        let state = Arc::new(Mutex::new(st));
        let meta = ResponseMeta::new("req333", "pid555")
            .with_job(state.clone(), job_id.clone());

        finalize_job_if_needed("HTTP/1.0 500 Internal Server Error", "crash", &meta);

        let st_guard = state.lock().unwrap();
        let job = st_guard.jobs.get(&job_id).unwrap();
        assert_eq!(job.status, "failed");
        assert_eq!(job.error_message, "crash");
        assert_eq!(job.result, Value::Null);
    }
}
