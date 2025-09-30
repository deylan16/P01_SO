use crate::state::SharedState;
use crate::handlers;

pub fn route(path_and_query: &str, state: SharedState) -> String {
    // Separa ruta y query (en caso de que haya '?')
    let mut parts = path_and_query.splitn(2, '?');
    let path = parts.next().unwrap_or("");
    let query = parts.next().unwrap_or("");

    match path {
        "/status" => {handlers::handle_status(state)}
        "/fibonacci" => {
            match handlers::parse_fib_param(query) {
                Ok(n)    => handlers::handle_fibonacci(n),
                Err(msg) => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}",
                    msg
                ),
            }
        }
        "/createfile" => {
            match handlers::parse_createfile_params(query) {
                Ok((name, content, repeat)) => {
                    handlers::handle_createfile(&name, &content, repeat)
                }
                Err(msg) => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}\n",
                    msg
                ),
            }
        }
        "/deletefile" => {
            match handlers::parse_deletefile_param(query) {
                Ok(name) => handlers::handle_deletefile(&name),
                Err(msg)  => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}\n",
                    msg
                ),
            }
        }
        "/reverse" => {
            match handlers::parse_text_param(query) {
                Ok(text)  => handlers::handle_reverse(&text),
                Err(msg)  => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}\n",
                    msg
                ),
            }
        }
        "/toupper" => {
            match handlers::parse_text_param(query) {
                Ok(text)  => handlers::handle_toupper(&text),
                Err(msg)  => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}\n",
                    msg
                ),
            }
        }
        "/random" => {
            match handlers::parse_random_params(query) {
                Ok((count, min, max)) => handlers::handle_random(count, min, max),
                Err(msg) => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}\n",
                    msg
                ),
            }
        }
        "/timestamp" => {
            if query.is_empty() {
                handlers::handle_timestamp()
            } else {
                format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\nRuta '/timestamp' no acepta parámetros\n"
                )
            }
        }
        "/hash" => {
            match handlers::parse_text_param(query) {
                Ok(text)  => handlers::handle_hash(&text),
                Err(msg)  => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}\n",
                    msg
                ),
            }
        }
        "/simulate" => {
            match handlers::parse_simulate_params(query) {
                Ok((seconds, task_name)) => handlers::handle_simulate(seconds, task_name),
                Err(msg) => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}\n",
                    msg
                ),
            }
        }
        "/sleep" => {
            match handlers::parse_sleep_param(query) {
                Ok(seconds) => handlers::handle_sleep(seconds),
                Err(msg)    => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}\n",
                    msg
                ),
            }
        }
        "/loadtest" => {
            match handlers::parse_loadtest_params(query) {
                Ok((tasks, sleep_secs)) => handlers::handle_loadtest(tasks, sleep_secs),
                Err(msg) => format!(
                    "HTTP/1.0 400 Bad Request\r\n\r\n{}\n",
                    msg
                ),
            }
        }
        "/help"        => handlers::handle_help(),
        _ => {
            // 404 por defecto
            format!(
                "HTTP/1.0 404 Not Found\r\n\r\nRuta no implementada: {}",
                path_and_query
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::new_state;
    use std::path::Path;

    #[test]
    fn route_not_found() {
        let state = new_state();
        let resp = route("/noexiste", state.clone());
        assert!(resp.starts_with("HTTP/1.0 404 Not Found"));
    }

    #[test]
    fn route_reverse_ok() {
        let state = new_state();
        let resp = route("/reverse?text=abc", state.clone());
        assert!(resp.contains("cba\n"));
    }

    #[test]
    fn route_reverse_missing_param() {
        let state = new_state();
        let resp = route("/reverse", state.clone());
        assert!(resp.starts_with("HTTP/1.0 400 Bad Request"));
    }

    #[test]
    fn route_fibonacci_ok() {
        let state = new_state();
        let resp = route("/fibonacci?num=6", state.clone());
        assert!(resp.contains("8\n"));
    }

    #[test]
    fn route_fibonacci_invalid() {
        let state = new_state();
        let resp = route("/fibonacci?num=xyz", state.clone());
        assert!(resp.starts_with("HTTP/1.0 400 Bad Request"));
    }

    #[test]
    fn route_status_json() {
        let state = new_state();
        let resp = route("/status", state.clone());
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        assert!(resp.contains("total_connections"));
        assert!(resp.contains("pid"));
    }

    #[test]
    fn route_unknown_returns_404() {
        let state = new_state();
        let resp = route("/nope", state.clone());
        assert!(resp.starts_with("HTTP/1.0 404 Not Found"));
    }

    #[test]
    fn route_toupper_ok() {
        let state = new_state();
        let resp = route("/toupper?text=abc", state.clone());
        assert!(resp.contains("ABC\n"));
    }

    #[test]
    fn route_timestamp_no_params() {
        let state = new_state();
        let resp = route("/timestamp", state.clone());
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
    }

    #[test]
    fn route_timestamp_with_params() {
        let state = new_state();
        let resp = route("/timestamp?foo=bar", state.clone());
        assert!(resp.starts_with("HTTP/1.0 400 Bad Request"));
    }

    #[test]
    fn route_help() {
        let state = new_state();
        let resp = route("/help", state.clone());
        assert!(resp.contains("/status"));
        assert!(resp.contains("/help"));
    }

    #[test]
    fn route_random_ok() {
        let state = new_state();
        let resp = route("/random?count=2&min=5&max=5", state.clone());
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        // el cuerpo debe ser "[5,5]"
        let body = resp.split("\r\n\r\n").nth(1).unwrap().trim();
        assert_eq!(body, "[5,5]");
    }

    #[test]
    fn route_createfile_and_deletefile_success() {
        let state = new_state();
        let filename = "test_router.txt";
        // Crear
        let url = format!("/createfile?name={}&content=Hi&repeat=2", filename);
        let resp = route(&url, state.clone());
        assert!(resp.starts_with("HTTP/1.0 200 OK"));
        assert!(Path::new(filename).exists());
        // Eliminar
        let url2 = format!("/deletefile?name={}", filename);
        let resp2 = route(&url2, state.clone());
        assert!(resp2.starts_with("HTTP/1.0 200 OK"));
        assert!(!Path::new(filename).exists());
    }

    #[test]
    fn route_createfile_missing_param() {
        let state = new_state();
        let resp = route("/createfile?content=Hi", state.clone());
        assert!(resp.starts_with("HTTP/1.0 400 Bad Request"));
    }

    #[test]
    fn route_deletefile_missing_param() {
        let state = new_state();
        let resp = route("/deletefile", state.clone());
        assert!(resp.starts_with("HTTP/1.0 400 Bad Request"));
    }

    #[test]
    fn route_hash_ok_and_missing() {
        let state = new_state();
        // Missing text
        let resp_err = route("/hash", state.clone());
        assert!(resp_err.starts_with("HTTP/1.0 400 Bad Request"));
        // Valid
        let resp_ok = route("/hash?text=abc", state.clone());
        assert!(resp_ok.starts_with("HTTP/1.0 200 OK"));
        assert!(resp_ok.contains(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
    }

    #[test]
    fn route_simulate_and_sleep_and_loadtest() {
        let state = new_state();
        // Simulate
        let resp_sim = route("/simulate?seconds=0&task=x", state.clone());
        assert!(resp_sim.contains("completada en 0 segundo(s)"));
        // Sleep
        let resp_sl = route("/sleep?seconds=0", state.clone());
        assert!(resp_sl.contains("Espera de 0 segundo(s) completada"));
        // Loadtest
        let resp_lt = route("/loadtest?tasks=1&sleep=0", state.clone());
        assert!(resp_lt.contains("Carga de 1 tarea(s) con 0 seg de sleep completada en 0 segundo(s)"));
    }

    #[test]
    fn route_simulate_missing_params() {
        let state = new_state();
        let resp = route("/simulate?task=x", state.clone());
        assert!(resp.starts_with("HTTP/1.0 400 Bad Request"));
    }

    #[test]
    fn route_sleep_missing_params() {
        let state = new_state();
        let resp = route("/sleep", state.clone());
        assert!(resp.starts_with("HTTP/1.0 400 Bad Request"));
    }

    #[test]
    fn route_loadtest_missing_or_invalid() {
        let state = new_state();
        let r1 = route("/loadtest", state.clone());
        assert!(r1.starts_with("HTTP/1.0 400 Bad Request"));
        let r2 = route("/loadtest?tasks=2&sleep=abc", state.clone());
        assert!(r2.starts_with("HTTP/1.0 400 Bad Request"));
    }
}
