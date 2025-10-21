mod control;
mod errors;
use control::{new_state, WorkerInfo};

use std::net::{TcpListener, TcpStream};
use std::io::Read; 
use std::sync::mpsc::{self, Sender};
use std::collections::HashMap; 
use std::thread;

use errors::{error400, error404, error500, res200};
use errors::res200_json;

use chrono::Utc;
use serde_json::json;
use urlencoding::decode;

use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;
use sha2::{Sha256, Digest};
use std::time::{Duration, Instant};

use std::fs::{File, remove_file, read_to_string};
use std::io::{self, Read as IoRead, Write as IoWrite, BufRead, BufReader};

// sleep del hilo
use std::thread::sleep;

// flate2 para gzip
use flate2::write::GzEncoder;
use flate2::Compression;



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

                // --- Helper local: parsea "a=b&c=d" haciendo también + -> ' ' ---
                let parse_query = |qs: &str| -> std::collections::HashMap<String, String> {
                    let mut m = std::collections::HashMap::new();
                    for pair in qs.split('&') {
                        if pair.is_empty() { continue; }
                        let mut it = pair.splitn(2, '=');

                        // valores crudos
                        let k0 = it.next().unwrap_or("");
                        let v0 = it.next().unwrap_or("");

                        // '+' => espacio (form-urlencoded)
                        let k_fixed = k0.replace('+', " ");
                        let v_fixed = v0.replace('+', " ");

                        // decodifica %xx sin violar el borrow checker
                        let k = match decode(&k_fixed) {
                            Ok(cow) => cow.into_owned(),
                            Err(_)  => k_fixed,
                        };
                        let v = match decode(&v_fixed) {
                            Ok(cow) => cow.into_owned(),
                            Err(_)  => v_fixed,
                        };

                        m.insert(k, v);
                    }
                    m
                };

                //El for pasa escuchando si entra una nueva tarea al canal del worker
                'worker: for task in rx {
                    //El worker se pone como ocupado
                    {
                        let mut st = state_clone.lock().unwrap();
                        if let Some(w) = st.workers.iter_mut().find(|w| w.thread_id == format!("{:?}", tid)) {
                            w.busy = true;
                        }
                    }

                    // Separa path y query de la solicitud original 
                    let (path, qmap) = {
                        let mut it = task.path_and_args.splitn(2, '?');
                        let p = it.next().unwrap_or("/");
                        let q = it.next().unwrap_or("");
                        (p, parse_query(q))
                    };

                    match path {
                        // /reverse?text=texto
                        "/reverse" => {
                            if let Some(text) = qmap.get("text") {
                                let rev: String = text.chars().rev().collect();
                                res200(task.stream.try_clone().unwrap(), &rev);
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Falta parámetro 'text'");
                            }
                        },

                        // /toupper?text=texto -> TEXTO
                        "/toupper" => {
                            if let Some(text) = qmap.get("text") {
                                let up = text.to_uppercase();
                                res200(task.stream.try_clone().unwrap(), &up);
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Falta parámetro 'text'");
                            }
                        },

                        // /fibonacci?n=30 -> valor de F(n)
                        "/fibonacci" => {
                            match qmap.get("n").and_then(|s| s.parse::<u64>().ok()) {
                                Some(n) if n <= 93 => {
                                    let (mut a, mut b) = (0u128, 1u128);
                                    for _ in 0..n { let t = a + b; a = b; b = t; }
                                    res200(task.stream.try_clone().unwrap(), &format!("{}", a));
                                }
                                Some(_) => {
                                    error400(task.stream.try_clone().unwrap(), "n demasiado grande (max 93)");
                                }
                                None => {
                                    error400(task.stream.try_clone().unwrap(), "Parámetro 'n' inválido o faltante");
                                }
                            }
                        },

                        // /random?min=0&max=100  -> entero aleatorio en [min, max]
                        "/random" => {
                            let min = qmap.get("min").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                            let max = qmap.get("max").and_then(|s| s.parse::<i64>().ok()).unwrap_or(100);

                            if min > max {
                                error400(task.stream.try_clone().unwrap(), "min debe ser <= max");
                            } else {
                                let mut rng = rand::thread_rng();
                                let n = rng.gen_range(min..=max);
                                res200(task.stream.try_clone().unwrap(), &n.to_string());
                            }
                        },

                        // /hash?text=hola  -> sha256 en hex
                        "/hash" => {
                            if let Some(text) = qmap.get("text") {
                                let mut hasher = Sha256::new();
                                hasher.update(text.as_bytes());
                                let digest = hasher.finalize();
                                let hex = format!("{:x}", digest);
                                res200(task.stream.try_clone().unwrap(), &hex);
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Falta parámetro 'text'");
                            }
                        },

                        // /help -> listado simple de rutas
                        "/help" => {
                            let help = [
                                "GET /status",
                                "GET /help",
                                "GET /reverse?text=...",
                                "GET /toupper?text=...",
                                "GET /fibonacci?n=...",
                                "GET /random?min=..&max=..",
                                "GET /hash?text=...",
                                "GET /timestamp",
                                "GET /sleep?ms=...",
                                "GET /createfile?path=..&content=..",
                                "GET /deletefile?path=..",
                                "GET /isprime?n=...",
                                "GET /factor?n=...",
                                "GET /pi?iters=...",
                                "GET /mandelbrot?width=..&height=..&max_iter=..&file=..(opcional)",
                                "GET /matrixmul?n=...",
                                "GET /sortfile?path=...",
                                "GET /wordcount?path=...",
                                "GET /grep?path=..&pattern=..",
                                "GET /compress?path=...",
                                "GET /hashfile?path=...",
                                "GET /simulate?ms=...",
                                "GET /loadtest?jobs=..&ms=..",
                                "GET /metrics",
                            ].join("\n");
                            res200(task.stream.try_clone().unwrap(), &help);
                        },

                        // ---------- TIEMPO ----------
                        // /timestamp -> fecha/hora ISO-8601 UTC
                        "/timestamp" => {
                            res200(task.stream.try_clone().unwrap(), &Utc::now().to_rfc3339());
                        },

                        // /sleep?ms=1000 -> duerme X ms (bloquea el worker, no el servidor)
                        "/sleep" => {
                            let ms = qmap.get("ms").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                            sleep(Duration::from_millis(ms));
                            res200(task.stream.try_clone().unwrap(), &format!("slept {} ms", ms));
                        },

                        // ---------- ARCHIVOS PEQUEÑOS ----------
                        // /createfile?path=/tmp/a.txt&content=hola
                        "/createfile" => {
                            if let Some(path) = qmap.get("path") {
                                let content = qmap.get("content").map(String::as_str).unwrap_or("");
                                match File::create(path) {
                                    Ok(mut f) => {
                                        let _ = f.write_all(content.as_bytes());
                                        res200(task.stream.try_clone().unwrap(), &format!("created {}", path));
                                    }
                                    Err(e) => error400(task.stream.try_clone().unwrap(), &format!("no se pudo crear: {}", e)),
                                }
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Falta parámetro 'path'");
                            }
                        },

                        // /deletefile?path=/tmp/a.txt
                        "/deletefile" => {
                            if let Some(path) = qmap.get("path") {
                                match remove_file(path) {
                                    Ok(_)  => res200(task.stream.try_clone().unwrap(), &format!("deleted {}", path)),
                                    Err(e) => error400(task.stream.try_clone().unwrap(), &format!("no se pudo borrar: {}", e)),
                                }
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Falta parámetro 'path'");
                            }
                        },

                        // ---------- NÚMEROS / CÁLCULO ----------
                        // /isprime?n=... -> "true"/"false"
                        "/isprime" => {
                            if let Some(n) = qmap.get("n").and_then(|s| s.parse::<u128>().ok()) {
                                if n < 2 {
                                    res200(task.stream.try_clone().unwrap(), "false");
                                } else if n % 2 == 0 {
                                    res200(task.stream.try_clone().unwrap(), if n == 2 { "true" } else { "false" });
                                } else {
                                    let mut d = 3u128;
                                    let mut prime = true;
                                    while d * d <= n {
                                        if n % d == 0 { prime = false; break; }
                                        d += 2;
                                    }
                                    res200(task.stream.try_clone().unwrap(), if prime { "true" } else { "false" });
                                }
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Parámetro 'n' inválido o faltante");
                            }
                        },

                        // /factor?n=... -> factorización prima en "p1^a1 * p2^a2 ..."
                        "/factor" => {
                            if let Some(mut n) = qmap.get("n").and_then(|s| s.parse::<u128>().ok()) {
                                if n < 2 {
                                    res200(task.stream.try_clone().unwrap(), "1");
                                } else {
                                    let mut res: Vec<(u128, u32)> = Vec::new();
                                    let mut d = 2u128;
                                    while d * d <= n {
                                        let mut cnt = 0;
                                        while n % d == 0 {
                                            n /= d; cnt += 1;
                                        }
                                        if cnt > 0 { res.push((d, cnt)); }
                                        d += if d == 2 { 1 } else { 2 };
                                    }
                                    if n > 1 { res.push((n, 1)); }
                                    let body = res.into_iter()
                                        .map(|(p,a)| if a==1 { format!("{}", p) } else { format!("{}^{}", p,a) })
                                        .collect::<Vec<_>>()
                                        .join(" * ");
                                    res200(task.stream.try_clone().unwrap(), &body);
                                }
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Parámetro 'n' inválido o faltante");
                            }
                        },

                        // /pi?iters=1000000 -> aproxima PI (serie Leibniz)
                        "/pi" => {
                            let iters = qmap.get("iters").and_then(|s| s.parse::<u64>().ok()).unwrap_or(100_000);
                            let mut sum = 0.0f64;
                            for k in 0..iters {
                                let term = if k % 2 == 0 { 1.0 } else { -1.0 };
                                sum += term / (2.0 * (k as f64) + 1.0);
                            }
                            let pi = 4.0 * sum;
                            res200(task.stream.try_clone().unwrap(), &format!("{:.10}", pi));
                        },

                        // /mandelbrot?width=..&height=..&max_iter=..[&file=...]
                        "/mandelbrot" => {
                            // lee parámetros (acepta max_iter o iter)
                            let w = qmap.get("width").and_then(|s| s.parse::<usize>().ok()).unwrap_or(80);
                            let h = qmap.get("height").and_then(|s| s.parse::<usize>().ok()).unwrap_or(24);
                            let max_iter = qmap
                                .get("max_iter")
                                .or_else(|| qmap.get("iter"))
                                .and_then(|s| s.parse::<u32>().ok())
                                .unwrap_or(50);

                            if w == 0 || h == 0 || w > 1000 || h > 1000 || max_iter == 0 {
                                error400(task.stream.try_clone().unwrap(), "parámetros inválidos (1<=W,H<=1000, max_iter>0)");
                            } else {
                                // matriz de iteraciones HxW
                                let mut iters = vec![vec![0u32; w]; h];

                                for y in 0..h {
                                    for x in 0..w {
                                        // mapeo a plano complejo
                                        let cx = (x as f64 / w as f64) * 3.5 - 2.5;
                                        let cy = (y as f64 / h as f64) * 2.0 - 1.0;

                                        let (mut zx, mut zy) = (0.0_f64, 0.0_f64);
                                        let mut i = 0u32;
                                        while zx * zx + zy * zy <= 4.0 && i < max_iter {
                                            let xt = zx * zx - zy * zy + cx;
                                            zy = 2.0 * zx * zy + cy;
                                            zx = xt;
                                            i += 1;
                                        }
                                        iters[y][x] = i;
                                    }
                                }

                                // si piden archivo, lo escribimos PGM y respondemos corto
                                if let Some(fname) = qmap.get("file") {
                                    use std::io::Write;
                                    let mut f = match std::fs::File::create(fname) {
                                        Ok(f) => f,
                                        Err(e) => {
                                            error400(
                                                task.stream.try_clone().unwrap(),
                                                &format!("no se pudo crear {}: {}", fname, e),
                                            );
                                            continue 'worker; // <- volvemos al loop del worker
                                        }
                                    };

                                    // cabecera PGM (ascii P2)
                                    let _ = f.write_all(format!("P2\n{} {}\n255\n", w, h).as_bytes());
                                    for y in 0..h {
                                        for x in 0..w {
                                            // escala lineal a 0..255
                                            let v = if max_iter == 0 {
                                                0
                                            } else {
                                                (iters[y][x] as u64 * 255 / max_iter as u64) as u32
                                            };
                                            let _ = f.write_all(format!("{} ", v).as_bytes());
                                        }
                                        let _ = f.write_all(b"\n");
                                    }

                                    res200(
                                        task.stream.try_clone().unwrap(),
                                        &format!("PGM escrito en {}", fname),
                                    );
                                    continue 'worker;
                                }

                                // si no pidieron archivo, respondemos la matriz en JSON
                                let body = serde_json::to_string(&iters).unwrap();
                                res200_json(task.stream.try_clone().unwrap(), &body);
                            }
                        }

                        // /matrixmul?n=200 -> multiplica dos matrices NxN y devuelve checksum
                        "/matrixmul" => {
                            let n = qmap.get("n").and_then(|s| s.parse::<usize>().ok()).unwrap_or(100);
                            if n == 0 || n > 600 {
                                error400(task.stream.try_clone().unwrap(), "n inválido (1..600)");
                            } else {
                                let mut rng = rand::thread_rng();
                                let mut a = vec![0f64; n*n];
                                let mut b = vec![0f64; n*n];
                                for v in a.iter_mut() { *v = rng.gen_range(0.0..1.0); }
                                for v in b.iter_mut() { *v = rng.gen_range(0.0..1.0); }
                                let mut sum = 0.0f64;
                                // simple ijk (no optimizado)
                                for i in 0..n {
                                    for k in 0..n {
                                        let aik = a[i*n + k];
                                        for j in 0..n {
                                            sum += aik * b[k*n + j];
                                        }
                                    }
                                }
                                res200(task.stream.try_clone().unwrap(), &format!("checksum={:.6}", sum));
                            }
                        },

                        // ---------- ARCHIVOS GRANDES ----------
                        // /sortfile?path=... -> ordena líneas in-place (lexicográfico)
                        "/sortfile" => {
                            if let Some(path) = qmap.get("path") {
                                match read_to_string(path) {
                                    Ok(s) => {
                                        let mut lines: Vec<&str> = s.lines().collect();
                                        lines.sort_unstable();
                                        match File::create(path) {
                                            Ok(mut f) => {
                                                for (i, ln) in lines.iter().enumerate() {
                                                    if i > 0 { let _ = f.write_all(b"\n"); }
                                                    let _ = f.write_all(ln.as_bytes());
                                                }
                                                res200(task.stream.try_clone().unwrap(), &format!("sorted {}", lines.len()));
                                            }
                                            Err(e) => error400(task.stream.try_clone().unwrap(), &format!("no se pudo escribir: {}", e)),
                                        }
                                    }
                                    Err(e) => error400(task.stream.try_clone().unwrap(), &format!("no se pudo leer: {}", e)),
                                }
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Falta parámetro 'path'");
                            }
                        },

                        // /wordcount?path=... -> conteo de bytes, líneas, palabras
                        "/wordcount" => {
                            if let Some(path) = qmap.get("path") {
                                match File::open(path) {
                                    Ok(f) => {
                                        let mut bytes = 0usize;
                                        let mut lines = 0usize;
                                        let mut words = 0usize;
                                        let mut buf = String::new();
                                        let mut rdr = BufReader::new(f);

                                        loop {
                                            buf.clear();
                                            let n = rdr.read_line(&mut buf).unwrap_or(0);
                                            if n == 0 { break; }
                                            lines += 1;
                                            bytes += buf.as_bytes().len();
                                            words += buf.split_whitespace().count();
                                        }

                                        res200(
                                            task.stream.try_clone().unwrap(),
                                            &format!("bytes={} lines={} words={}", bytes, lines, words),
                                        );
                                    }
                                    Err(e) => error400(
                                        task.stream.try_clone().unwrap(),
                                        &format!("no se pudo abrir: {}", e),
                                    ),
                                }
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Falta parámetro 'path'");
                            }
                        },

                        // /grep?path=...&pattern=... -> número de líneas que contienen el patrón
                        "/grep" => {
                            if let (Some(path), Some(pat)) = (qmap.get("path"), qmap.get("pattern")) {
                                match File::open(path) {
                                    Ok(f) => {
                                        let mut cnt = 0usize;
                                        for line in BufReader::new(f).lines() {
                                            if let Ok(l) = line {
                                                if l.contains(pat) { cnt += 1; }
                                            }
                                        }
                                        res200(task.stream.try_clone().unwrap(), &format!("matches={}", cnt));
                                    }
                                    Err(e) => error400(task.stream.try_clone().unwrap(), &format!("no se pudo abrir: {}", e)),
                                }
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Faltan 'path' y/o 'pattern'");
                            }
                        },

                        // /compress?path=... -> crea path.gz con gzip
                        "/compress" => {
                            if let Some(path) = qmap.get("path") {
                                match File::open(path) {
                                    Ok(mut f) => {
                                        let gz_path = format!("{}.gz", path);
                                        match File::create(&gz_path) {
                                            Ok(out) => {
                                                let mut enc = GzEncoder::new(out, Compression::default());
                                                let mut rdr = BufReader::new(&mut f);
                                                let mut buf = Vec::with_capacity(64 * 1024);
                                                loop {
                                                    buf.clear();
                                                    use std::io::Read;
                                                    let n = rdr.by_ref().take(64 * 1024).read_to_end(&mut buf).unwrap_or(0);
                                                    if n == 0 { break; }
                                                    let _ = enc.write_all(&buf[..n]);
                                                }
                                                let _ = enc.finish();
                                                res200(task.stream.try_clone().unwrap(), &format!("compressed -> {}", gz_path));
                                            }
                                            Err(e) => error400(task.stream.try_clone().unwrap(), &format!("no se pudo crear .gz: {}", e)),
                                        }
                                    }
                                    Err(e) => error400(task.stream.try_clone().unwrap(), &format!("no se pudo abrir: {}", e)),
                                }
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Falta parámetro 'path'");
                            }
                        },

                        // /hashfile?path=... -> sha256 hex del archivo
                        "/hashfile" => {
                            if let Some(path) = qmap.get("path") {
                                match File::open(path) {
                                    Ok(f) => {
                                        let mut hasher = Sha256::new();
                                        let mut rdr = BufReader::new(f);
                                        let mut buf = [0u8; 64 * 1024];
                                        loop {
                                            use std::io::Read;
                                            let n = rdr.read(&mut buf).unwrap_or(0);
                                            if n == 0 { break; }
                                            hasher.update(&buf[..n]);
                                        }
                                        let hex = format!("{:x}", hasher.finalize());
                                        res200(task.stream.try_clone().unwrap(), &hex);
                                    }
                                    Err(e) => error400(task.stream.try_clone().unwrap(), &format!("no se pudo abrir: {}", e)),
                                }
                            } else {
                                error400(task.stream.try_clone().unwrap(), "Falta parámetro 'path'");
                            }
                        },

                        // ---------- CARGA / BENCH SENCILLO ----------
                        // /simulate?ms=... -> busy-loop de CPU durante ms
                        "/simulate" => {
                            let ms = qmap.get("ms").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                            let until = Instant::now() + Duration::from_millis(ms);
                            let mut x = 0u64;
                            while Instant::now() < until {
                                x = x.wrapping_add(1);
                            }
                            res200(task.stream.try_clone().unwrap(), &format!("busy {} ms, counter={}", ms, x));
                        },

                        // /loadtest?jobs=...&ms=... -> corre N simulaciones
                        "/loadtest" => {
                            let jobs = qmap.get("jobs").and_then(|s| s.parse::<u64>().ok()).unwrap_or(10);
                            let per  = qmap.get("ms").and_then(|s| s.parse::<u64>().ok()).unwrap_or(50);
                            let start = Instant::now();
                            for _ in 0..jobs {
                                let until = Instant::now() + Duration::from_millis(per);
                                let mut x = 0u64;
                                while Instant::now() < until { x = x.wrapping_add(1); }
                            }
                            let elapsed = start.elapsed().as_millis();
                            res200(task.stream.try_clone().unwrap(), &format!("jobs={} per_ms={} total_ms={}", jobs, per, elapsed));
                        },

                        // /metrics -> por ahora alias minimal a /status (sin tocar estado)
                        "/metrics" => {
                            // Si más adelante quieres algo distinto, lo cambiamos. Por ahora es stub simple.
                            res200(task.stream.try_clone().unwrap(), "metrics: use /status JSON");
                        },

                        // fallback
                        _ => {
                            let message = format!("Ejecutando {} en el worker {:?}", &task.path_and_args, tid);
                            res200(task.stream.try_clone().unwrap(), &message);
                        },
                    }

                    // ------------------------------------------------------------------

                    //El worker se pone como disponible
                    {
                        let mut st = state_clone.lock().unwrap();
                        if let Some(w) = st.workers.iter_mut().find(|w| w.thread_id == format!("{:?}", tid)) {
                            w.busy = false;
                        }
                    }
                } // <-- cierre del bucle 'worker
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

                //  /status responde JSON aquí ---
                if path == "/status" {
                    let st = state.lock().unwrap();
                    let uptime = (Utc::now() - st.start_time).num_seconds();

                    let body = json!({
                        "uptime_seconds": uptime,
                        "total_connections": st.total_connections,
                        "pid": st.pid,
                        "workers": st.workers.iter().map(|w| {
                            json!({"command": w.command, "thread_id": w.thread_id, "busy": w.busy})
                        }).collect::<Vec<_>>()
                    }).to_string();

                    res200_json(stream, &body);
                    continue; // importante: no encolar esta solicitud
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
