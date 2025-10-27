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
pub fn error400(stream: TcpStream, explain: &str, meta: &ResponseMeta) {
    send_response(stream, "HTTP/1.0 400 Bad Request", explain, meta);
}

/// 404 - Not Found
pub fn error404(stream: TcpStream, explain: &str, meta: &ResponseMeta) {
    send_response(stream, "HTTP/1.0 404 Not Found", explain, meta);
}

/// 409 - Conflict
pub fn error409(stream: TcpStream, explain: &str, meta: &ResponseMeta) {
    send_response(stream, "HTTP/1.0 409 Conflict", explain, meta);
}

/// 429 - Too Many Requests
pub fn error429(stream: TcpStream, explain: &str, meta: &ResponseMeta) {
    send_response(stream, "HTTP/1.0 429 Too Many Requests", explain, meta);
}

/// 500 - Internal Server Error
pub fn error500(stream: TcpStream, explain: &str, meta: &ResponseMeta) {
    send_response(stream, "HTTP/1.0 500 Internal Server Error", explain, meta);
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
