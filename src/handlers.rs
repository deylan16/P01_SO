use crate::state::SharedState;
use chrono::Utc;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use rand::Rng;
use serde_json;
use sha2::{Sha256, Digest};
use hex;
use std::time::{Duration, Instant};
use std::thread::sleep;
use std::thread;

/// Respuesta JSON de /status, incluyendo PID y lista de workers
#[derive(Serialize)]
struct StatusResponse {
    uptime_seconds: i64,
    total_connections: usize,
    pid: u32,
    workers: Vec<WorkerInfoResponse>,
}

#[derive(Serialize)]
struct WorkerInfoResponse {
    command: String,
    thread_id: String,
    busy: bool,
}

pub fn handle_status(state: SharedState) -> String {
    let st = state.lock().unwrap();
    let uptime = Utc::now().signed_duration_since(st.start_time);

    // Convertir la lista interna a la forma serializable
    let workers_resp = st.workers.iter().map(|w| WorkerInfoResponse {
        command: w.command.clone(),
        thread_id: w.thread_id.clone(),
        busy: w.busy,
    }).collect();

    let resp = StatusResponse {
        uptime_seconds: uptime.num_seconds(),
        total_connections: st.total_connections,
        pid: st.pid,
        workers: workers_resp,
    };
    let body = serde_json::to_string(&resp).unwrap();

    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: application/json\r\n\r\n\
         {}",
        body
    )
}

/// Calcula recursivamente el número de Fibonacci (Hay que mejorarla más adelante pero de momento sirve)
fn fib(n: usize) -> usize {
    match n {
        0 => 0,
        1 => 1,
        _ => fib(n - 1) + fib(n - 2),
    }
}

/// Parsea y valida el parámetro num=N para /fibonacci
///   - Err si falta `num`
///   - Err si `num` no es un entero >= 0
pub fn parse_fib_param(query: &str) -> Result<usize, String> {
    query
        .split('&')
        .find_map(|p| p.strip_prefix("num="))
        .ok_or_else(|| "Parámetro 'num' requerido".to_string())
        .and_then(|v| {
            v.parse::<usize>()
             .map_err(|_| "Parámetro 'num' debe ser un entero no negativo".to_string())
        })
}

/// Handler de Fibonacci recibe un 'n' validado
pub fn handle_fibonacci(n: usize) -> String {
    let result = fib(n);
    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: text/plain\r\n\r\n\
         {}\n",
        result
    )
}

/// Parsea y valida parámetros para /createfile
///   - `name` y `content` son obligatorios
///   - `repeat` es opcional (por defecto 1) y debe ser entero > 0
pub fn parse_createfile_params(query: &str) -> Result<(String, String, usize), String> {
    let mut params = query
        .split('&')
        .filter_map(|p| {
            let mut kv = p.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some(k), Some(v)) => Some((k.to_string(), v.to_string())),
                _ => None,
            }
        })
        .collect::<std::collections::HashMap<_, _>>();

    let name = params
        .remove("name")
        .ok_or_else(|| "Parámetro 'name' requerido".to_string())?;
    let content = params
        .remove("content")
        .ok_or_else(|| "Parámetro 'content' requerido".to_string())?;
    let repeat = params
        .remove("repeat")
        .map_or(Ok(1), |v| {
            v.parse::<usize>()
             .map_err(|_| "Parámetro 'repeat' debe ser un entero positivo".to_string())
        })?;

    Ok((name, content, repeat))
}

/// Handler de createfile, recibe parámetros ya validados
pub fn handle_createfile(name: &str, content: &str, repeat: usize) -> String {
    match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(name)
    {
        Ok(mut file) => {
            for _ in 0..repeat {
                if let Err(e) = write!(file, "{}", content) {
                    return format!(
                        "HTTP/1.0 500 Internal Server Error\r\n\r\nError escribiendo en '{}': {}\n",
                        name, e
                    );
                }
            }
            format!(
                "HTTP/1.0 200 OK\r\n\r\nArchivo creado: '{}', repitiendo {} veces\n",
                name, repeat
            )
        }
        Err(e) => format!(
            "HTTP/1.0 500 Internal Server Error\r\n\r\nNo se pudo crear '{}': {}\n",
            name, e
        ),
    }
}

/// Parsea y valida `name` para /deletefile
pub fn parse_deletefile_param(query: &str) -> Result<String, String> {
    query
        .split('&')
        .find_map(|p| p.strip_prefix("name="))
        .map(|s| s.to_string())
        .ok_or_else(|| "Parámetro 'name' requerido".to_string())
}

/// Handler de deletefile, recibe `name` ya validado
pub fn handle_deletefile(name: &str) -> String {
    match std::fs::remove_file(name) {
        Ok(_) => format!(
            "HTTP/1.0 200 OK\r\n\r\nArchivo eliminado: '{}'\n",
            name
        ),
        Err(e) => format!(
            "HTTP/1.0 500 Internal Server Error\r\n\r\nNo se pudo eliminar '{}': {}\n",
            name, e
        ),
    }
}

/// Parsea y valida `text` para /reverse
///   - Error 400 si falta `text`
pub fn parse_text_param(query: &str) -> Result<String, String> {
    query
        .split('&')
        .find_map(|p| p.strip_prefix("text="))
        .map(|s| s.to_string())
        .ok_or_else(|| "Parámetro 'text' requerido".to_string())
}

/// Handler de reverse, recibe `text` ya validado
pub fn handle_reverse(text: &str) -> String {
    let reversed: String = text.chars().rev().collect();
    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: text/plain\r\n\r\n\
         {}\n",
        reversed
    )
}

/// Handler de toupper, recibe `text` ya validado
pub fn handle_toupper(text: &str) -> String {
    let upper = text.to_uppercase();
    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: text/plain\r\n\r\n\
         {}\n",
        upper
    )
}

/// Parsea y valida parámetros para /random
///   - `count` es obligatorio y debe ser entero > 0
///   - `min` es obligatorio y debe ser entero
///   - `max` es obligatorio y debe ser entero
///   - Error 400 si falta alguno o no cumple el formato
///   - Error 400 si `min` > `max`
pub fn parse_random_params(query: &str) -> Result<(usize, i64, i64), String> {
    let mut params = query
        .split('&')
        .filter_map(|p| {
            let mut kv = p.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some(k), Some(v)) => Some((k, v)),
                _ => None,
            }
        })
        .collect::<std::collections::HashMap<_, _>>();

    let count = params
        .remove("count")
        .ok_or_else(|| "Parámetro 'count' requerido".to_string())
        .and_then(|v| v.parse::<usize>()
            .map_err(|_| "Parámetro 'count' debe ser un entero positivo".to_string()))
        .and_then(|c| if c > 0 {
            Ok(c)
        } else {
            Err("Parámetro 'count' debe ser mayor que cero".to_string())
        })?;

    let min = params
        .remove("min")
        .ok_or_else(|| "Parámetro 'min' requerido".to_string())
        .and_then(|v| v.parse::<i64>()
            .map_err(|_| "Parámetro 'min' debe ser un entero".to_string()))?;

    let max = params
        .remove("max")
        .ok_or_else(|| "Parámetro 'max' requerido".to_string())
        .and_then(|v| v.parse::<i64>()
            .map_err(|_| "Parámetro 'max' debe ser un entero".to_string()))?;

    if min > max {
        return Err("Parámetro 'min' no puede ser mayor que 'max'".to_string());
    }

    Ok((count, min, max))
}

/// Handler de /random recibe valores ya validados
pub fn handle_random(count: usize, min: i64, max: i64) -> String {
    let mut rng = rand::thread_rng();
    let nums: Vec<i64> = (0..count)
        .map(|_| rng.gen_range(min..=max))
        .collect();

    let body = serde_json::to_string(&nums).unwrap_or_else(|_| "[]".to_string());
    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: application/json\r\n\r\n\
         {}\n",
        body
    )
}

/// Devuelve la hora actual del sistema en formato ISO-8601 (UTC)
pub fn handle_timestamp() -> String {
    let now = Utc::now().to_rfc3339();
    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: text/plain\r\n\r\n\
         {}\n",
        now
    )
}

/// Handler de /hash, recibe `text` ya validado
pub fn handle_hash(text: &str) -> String {
    // Calcula SHA-256
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    let hex_str = hex::encode(result);

    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: text/plain\r\n\r\n\
         {}\n",
        hex_str
    )
}

/// Parsea y valida parámetros para /simulate
///   - `seconds` es obligatorio y debe ser entero >= 0
///   - `task` es obligatorio
pub fn parse_simulate_params(query: &str) -> Result<(u64, String), String> {
    let mut params = query
        .split('&')
        .filter_map(|p| {
            let mut kv = p.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some(k), Some(v)) => Some((k, v)),
                _ => None,
            }
        })
        .collect::<std::collections::HashMap<_, _>>();

    let seconds = params
        .remove("seconds")
        .ok_or_else(|| "Parámetro 'seconds' requerido".to_string())
        .and_then(|v| v.parse::<u64>()
            .map_err(|_| "Parámetro 'seconds' debe ser un entero no negativo".to_string()))?;

    let task = params
        .remove("task")
        .ok_or_else(|| "Parámetro 'task' requerido".to_string())?;

    Ok((seconds, task.to_string()))
}

/// Handler de /simulate, recibe valores ya validados
pub fn handle_simulate(seconds: u64, task_name: String) -> String {
    sleep(Duration::from_secs(seconds));
    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: text/plain\r\n\r\n\
         Tarea '{}' completada en {} segundo(s)\n",
        task_name, seconds
    )
}

/// Parsea y valida `seconds` para /sleep
///   - Error 400 si falta `seconds`
///   - Error 400 si no es un entero no negativo
pub fn parse_sleep_param(query: &str) -> Result<u64, String> {
    query
        .split('&')
        .find_map(|p| p.strip_prefix("seconds="))
        .ok_or_else(|| "Parámetro 'seconds' requerido".to_string())
        .and_then(|v| v.parse::<u64>()
            .map_err(|_| "Parámetro 'seconds' debe ser un entero no negativo".to_string()))
}

/// Handler de /sleep, recibe `seconds` ya validado
pub fn handle_sleep(seconds: u64) -> String {
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: text/plain\r\n\r\n\
         Espera de {} segundo(s) completada\n",
        seconds
    )
}

/// Parsea y valida parámetros para /loadtest
///   - `tasks` es obligatorio y debe ser un entero > 0
///   - `sleep` es obligatorio y debe ser un entero >= 0
pub fn parse_loadtest_params(query: &str) -> Result<(usize, u64), String> {
    let mut params = query
        .split('&')
        .filter_map(|p| {
            let mut kv = p.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some(k), Some(v)) => Some((k, v)),
                _ => None,
            }
        })
        .collect::<std::collections::HashMap<_, _>>();

    let tasks = params
        .remove("tasks")
        .ok_or_else(|| "Parámetro 'tasks' requerido".to_string())?
        .parse::<usize>()
        .map_err(|_| "Parámetro 'tasks' debe ser un entero positivo".to_string())
        .and_then(|n| if n > 0 {
            Ok(n)
        } else {
            Err("Parámetro 'tasks' debe ser mayor que cero".to_string())
        })?;

    let sleep_secs = params
        .remove("sleep")
        .ok_or_else(|| "Parámetro 'sleep' requerido".to_string())?
        .parse::<u64>()
        .map_err(|_| "Parámetro 'sleep' debe ser un entero no negativo".to_string())?;

    Ok((tasks, sleep_secs))
}

/// Handler de /loadtest, recibe valores ya validados
pub fn handle_loadtest(tasks: usize, sleep_secs: u64) -> String {
    let start = Instant::now();
    let mut handles = Vec::with_capacity(tasks);

    for _ in 0..tasks {
        let secs = sleep_secs;
        handles.push(thread::spawn(move || {
            thread::sleep(Duration::from_secs(secs));
        }));
    }

    for h in handles {
        let _ = h.join();
    }

    let elapsed = start.elapsed().as_secs();

    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: text/plain\r\n\r\n\
         Carga de {} tarea(s) con {} seg de sleep completada en {} segundo(s)\n",
        tasks, sleep_secs, elapsed
    )
}

/// Muestra ayuda: listado de rutas y parámetros
pub fn handle_help() -> String {
    let body = r#"Rutas disponibles:
/status
    -> GET: devuelve uptime y total_connections en JSON

/reverse?text=...
    -> GET: invierte el texto

/fibonacci?num=...
    -> GET: calcula Fibonacci recursivo

/toupper?text=...
    -> GET: convierte texto a MAYÚSCULAS

/createfile?name=...&content=...&repeat=...
    -> GET: crea archivo con contenido repetido

/deletefile?name=...
    -> GET: elimina archivo

/random?count=...&min=...&max=...
    -> GET: genera array JSON de números aleatorios

/timestamp
    -> GET: hora actual UTC en texto plano

/hash?text=...
    -> GET: SHA-256 del texto en hex

/simulate?seconds=...&task=...
    -> GET: simula tarea con delay

/sleep?seconds=...
    -> GET: simula retardo simple

/loadtest?tasks=...&sleep=...
    -> GET: carga concurrente de tareas con sleep

/help
    -> GET: muestra esta ayuda
"#;
    // Cabecera con charset=utf-8 y cuerpo ASCII-only
    format!(
        "HTTP/1.0 200 OK\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\n\
         {}",
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fib_param_success() {
        assert_eq!(parse_fib_param("num=0").unwrap(), 0);
        assert_eq!(parse_fib_param("num=10").unwrap(), 10);
    }

    #[test]
    fn parse_fib_param_missing() {
        assert_eq!(parse_fib_param("").unwrap_err(), "Parámetro 'num' requerido");
    }

    #[test]
    fn parse_fib_param_invalid() {
        assert_eq!(parse_fib_param("num=abc").unwrap_err(), "Parámetro 'num' debe ser un entero no negativo");
    }

    #[test]
    fn parse_text_param_success() {
        assert_eq!(parse_text_param("text=Hola").unwrap(), "Hola".to_string());
    }

    #[test]
    fn parse_text_param_missing() {
        let err = parse_text_param("").unwrap_err();
        assert_eq!(err, "Parámetro 'text' requerido");
    }

    #[test]
    fn handle_toupper_response_contains_uppercase() {
        let resp = handle_toupper("rust");
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        assert!(resp.contains("RUST\n"));
    }

    #[test]
    fn handle_fibonacci_response_contains_result() {
        let resp = handle_fibonacci(7);
        // Fibonacci(7) = 13
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        assert!(resp.contains("13\n"));
    }
    
    #[test]
    fn parse_createfile_params_success() {
        let q = "name=foo.txt&content=Hello&repeat=3";
        let (name, content, repeat) = parse_createfile_params(q).unwrap();
        assert_eq!(name, "foo.txt");
        assert_eq!(content, "Hello");
        assert_eq!(repeat, 3);
    }

    #[test]
    fn parse_createfile_params_default_repeat() {
        let q = "name=bar.txt&content=Hi";
        let (name, content, repeat) = parse_createfile_params(q).unwrap();
        assert_eq!(name, "bar.txt");
        assert_eq!(content, "Hi");
        assert_eq!(repeat, 1);
    }

    #[test]
    fn parse_createfile_missing_name() {
        let err = parse_createfile_params("content=Hi&repeat=2").unwrap_err();
        assert_eq!(err, "Parámetro 'name' requerido");
    }

    #[test]
    fn parse_createfile_invalid_repeat() {
        let err = parse_createfile_params("name=a.txt&content=X&repeat=abc").unwrap_err();
        assert_eq!(err, "Parámetro 'repeat' debe ser un entero positivo");
    }

    // Tests para deletefile
    #[test]
    fn parse_deletefile_success() {
        let name = parse_deletefile_param("name=test.txt").unwrap();
        assert_eq!(name, "test.txt");
    }

    #[test]
    fn parse_deletefile_missing() {
        let err = parse_deletefile_param("").unwrap_err();
        assert_eq!(err, "Parámetro 'name' requerido");
    }

    #[test]
    fn parse_random_params_success() {
        let q = "count=5&min=1&max=10";
        let (count, min, max) = parse_random_params(q).unwrap();
        assert_eq!(count, 5);
        assert_eq!(min, 1);
        assert_eq!(max, 10);
    }

    #[test]
    fn parse_random_missing_count() {
        let err = parse_random_params("min=1&max=10").unwrap_err();
        assert_eq!(err, "Parámetro 'count' requerido");
    }

    #[test]
    fn parse_random_invalid_count() {
        let err = parse_random_params("count=0&min=1&max=10").unwrap_err();
        assert_eq!(err, "Parámetro 'count' debe ser mayor que cero");
    }

    #[test]
    fn parse_random_min_gt_max() {
        let err = parse_random_params("count=5&min=10&max=1").unwrap_err();
        assert_eq!(err, "Parámetro 'min' no puede ser mayor que 'max'");
    }

    // Tests para /sleep
    #[test]
    fn parse_sleep_success() {
        assert_eq!(parse_sleep_param("seconds=3").unwrap(), 3);
    }

    #[test]
    fn parse_sleep_missing() {
        let err = parse_sleep_param("").unwrap_err();
        assert_eq!(err, "Parámetro 'seconds' requerido");
    }

    #[test]
    fn parse_sleep_invalid() {
        let err = parse_sleep_param("seconds=abc").unwrap_err();
        assert_eq!(err, "Parámetro 'seconds' debe ser un entero no negativo");
    }

    // Tests para /simulate
    #[test]
    fn parse_simulate_success() {
        let (s, t) = parse_simulate_params("seconds=2&task=test").unwrap();
        assert_eq!(s, 2);
        assert_eq!(t, "test".to_string());
    }

    #[test]
    fn parse_simulate_missing_seconds() {
        let err = parse_simulate_params("task=test").unwrap_err();
        assert_eq!(err, "Parámetro 'seconds' requerido");
    }

    #[test]
    fn parse_simulate_missing_task() {
        let err = parse_simulate_params("seconds=2").unwrap_err();
        assert_eq!(err, "Parámetro 'task' requerido");
    }

    // Tests para /loadtest
    #[test]
    fn parse_loadtest_success() {
        let (tasks, secs) = parse_loadtest_params("tasks=4&sleep=1").unwrap();
        assert_eq!(tasks, 4);
        assert_eq!(secs, 1);
    }

    #[test]
    fn parse_loadtest_missing_tasks() {
        let err = parse_loadtest_params("sleep=1").unwrap_err();
        assert_eq!(err, "Parámetro 'tasks' requerido");
    }

    #[test]
    fn parse_loadtest_invalid_sleep() {
        let err = parse_loadtest_params("tasks=4&sleep=abc").unwrap_err();
        assert_eq!(err, "Parámetro 'sleep' debe ser un entero no negativo");
    }

    // Test para handle_random: debe generar un JSON con el número correcto de elementos
    #[test]
    fn handle_random_response_length() {
        let resp = handle_random(10, 1, 5);
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        // tras la cabecera y salto de línea, debe haber un array de 10 elementos
        let body = resp.split("\r\n\r\n").nth(1).unwrap();
        let vec: Vec<i64> = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(vec.len(), 10);
        // cada elemento debe estar en [1,5]
        assert!(vec.iter().all(|&x| x >= 1 && x <= 5));
    }

    // Test para handle_hash: el SHA-256 de "abc" es altamente conocido
    #[test]
    fn handle_hash_known_value() {
        let resp = handle_hash("abc");
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        let body = resp.split("\r\n\r\n").nth(1).unwrap().trim();
        assert_eq!(
            body,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn handle_sleep_zero() {
        let start = Instant::now();
        let resp = handle_sleep(0);
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        assert!(resp.contains("Espera de 0 segundo(s) completada"));
        assert!(start.elapsed().as_secs() == 0);
    }

    // handle_simulate con 0 segundos retorna inmediatamente
    #[test]
    fn handle_simulate_zero() {
        let resp = handle_simulate(0, "x".into());
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        assert!(resp.contains("Tarea 'x' completada en 0 segundo(s)"));
    }

    // handle_loadtest con 1 tarea y 0 sleep retorna 0 segundos
    #[test]
    fn handle_loadtest_zero() {
        let resp = handle_loadtest(1, 0);
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        assert!(resp.contains("Carga de 1 tarea(s) con 0 seg de sleep completada en 0 segundo(s)"));
    }

    // handle_help contiene al menos la ruta /status y /help
    #[test]
    fn handle_help_contains_routes() {
        let resp = handle_help();
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        let body = resp.split("\r\n\r\n").nth(1).unwrap();
        assert!(body.contains("/status"));
        assert!(body.contains("/help"));
    }

    #[test]
    fn direct_handle_reverse() {
        let resp = handle_reverse("Rust");
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        assert!(resp.contains("tsuR\n"));
    }

    // Direct tests para handle_createfile y handle_deletefile
    #[test]
    fn direct_handle_createfile_and_deletefile() {
        let filename = "tmp_test_file.txt";
        // Crear archivo
        let resp_create = handle_createfile(filename, "AB", 2);
        assert!(resp_create.starts_with("HTTP/1.0 200 OK"));
        // Verificar contenido
        let content = std::fs::read_to_string(filename).unwrap();
        assert_eq!(content, "ABAB");
        // Borrar archivo
        let resp_delete = handle_deletefile(filename);
        assert!(resp_delete.starts_with("HTTP/1.0 200 OK"));
        assert!(!std::path::Path::new(filename).exists());
    }

    #[test]
    fn handle_deletefile_error_direct() {
        let resp = handle_deletefile("this_file_does_not_exist.xyz");
        assert!(resp.starts_with("HTTP/1.0 500 Internal Server Error"));
    }

    #[test]
    fn handle_loadtest_multiple_zero() {
        let resp = handle_loadtest(2, 0);
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        assert!(resp.contains("Carga de 2 tarea(s) con 0 seg de sleep completada en 0 segundo(s)\n"));
    }
}