use std::collections::HashMap;
use std::fs::{File, read_to_string, remove_file};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chrono::Utc;
use flate2::Compression;
use flate2::write::GzEncoder;
use rand::{Rng, thread_rng};
use sha2::{Digest, Sha256};

use crate::errors::{error400, res200, res200_json};

fn stream_clone(stream: &TcpStream) -> TcpStream {
    stream.try_clone().unwrap()
}

pub fn handle_command(path: &str, qmap: &HashMap<String, String>, stream: &TcpStream) -> bool {
    match path {
        "/reverse" => {
            if let Some(text) = qmap.get("text") {
                let rev: String = text.chars().rev().collect();
                res200(stream_clone(stream), &rev);
            } else {
                error400(stream_clone(stream), "Falta parámetro 'text'");
            }
            true
        }

        "/toupper" => {
            if let Some(text) = qmap.get("text") {
                let up = text.to_uppercase();
                res200(stream_clone(stream), &up);
            } else {
                error400(stream_clone(stream), "Falta parámetro 'text'");
            }
            true
        }

        "/fibonacci" => {
            match qmap.get("n").and_then(|s| s.parse::<u64>().ok()) {
                Some(n) if n <= 93 => {
                    let (mut a, mut b) = (0u128, 1u128);
                    for _ in 0..n {
                        let t = a + b;
                        a = b;
                        b = t;
                    }
                    res200(stream_clone(stream), &format!("{}", a));
                }
                Some(_) => {
                    error400(stream_clone(stream), "n demasiado grande (max 93)");
                }
                None => {
                    error400(stream_clone(stream), "Parámetro 'n' inválido o faltante");
                }
            }
            true
        }

        "/random" => {
            let min = qmap
                .get("min")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let max = qmap
                .get("max")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(100);

            if min > max {
                error400(stream_clone(stream), "min debe ser <= max");
            } else {
                let mut rng = thread_rng();
                let n = rng.gen_range(min..=max);
                res200(stream_clone(stream), &n.to_string());
            }
            true
        }

        "/hash" => {
            if let Some(text) = qmap.get("text") {
                let mut hasher = Sha256::new();
                hasher.update(text.as_bytes());
                let digest = hasher.finalize();
                let hex = format!("{:x}", digest);
                res200(stream_clone(stream), &hex);
            } else {
                error400(stream_clone(stream), "Falta parámetro 'text'");
            }
            true
        }

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
                "GET /mandelbrot?width=..&height=..&max_iter=..",
                "GET /matrixmul?n=...",
                "GET /sortfile?path=...",
                "GET /wordcount?path=...",
                "GET /grep?path=..&pattern=..",
                "GET /compress?path=...",
                "GET /hashfile?path=...",
                "GET /simulate?ms=...",
                "GET /loadtest?jobs=..&ms=..",
                "GET /metrics",
            ]
            .join("\n");
            res200(stream_clone(stream), &help);
            true
        }

        "/timestamp" => {
            res200(stream_clone(stream), &Utc::now().to_rfc3339());
            true
        }

        "/sleep" => {
            let ms = qmap
                .get("ms")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            sleep(Duration::from_millis(ms));
            res200(stream_clone(stream), &format!("slept {} ms", ms));
            true
        }

        "/createfile" => {
            if let Some(path) = qmap.get("path") {
                let content = qmap.get("content").map(String::as_str).unwrap_or("");
                match File::create(path) {
                    Ok(mut f) => {
                        let _ = f.write_all(content.as_bytes());
                        res200(stream_clone(stream), &format!("created {}", path));
                    }
                    Err(e) => error400(stream_clone(stream), &format!("no se pudo crear: {}", e)),
                }
            } else {
                error400(stream_clone(stream), "Falta parámetro 'path'");
            }
            true
        }

        "/deletefile" => {
            if let Some(path) = qmap.get("path") {
                match remove_file(path) {
                    Ok(_) => res200(stream_clone(stream), &format!("deleted {}", path)),
                    Err(e) => error400(stream_clone(stream), &format!("no se pudo borrar: {}", e)),
                }
            } else {
                error400(stream_clone(stream), "Falta parámetro 'path'");
            }
            true
        }

        "/isprime" => {
            if let Some(n) = qmap.get("n").and_then(|s| s.parse::<u128>().ok()) {
                if n < 2 {
                    res200(stream_clone(stream), "false");
                } else if n % 2 == 0 {
                    res200(stream_clone(stream), if n == 2 { "true" } else { "false" });
                } else {
                    let mut d = 3u128;
                    let mut prime = true;
                    while d * d <= n {
                        if n % d == 0 {
                            prime = false;
                            break;
                        }
                        d += 2;
                    }
                    res200(stream_clone(stream), if prime { "true" } else { "false" });
                }
            } else {
                error400(stream_clone(stream), "Parámetro 'n' inválido o faltante");
            }
            true
        }

        "/factor" => {
            if let Some(mut n) = qmap.get("n").and_then(|s| s.parse::<u128>().ok()) {
                if n < 2 {
                    res200(stream_clone(stream), "1");
                } else {
                    let mut res: Vec<(u128, u32)> = Vec::new();
                    let mut d = 2u128;
                    while d * d <= n {
                        let mut cnt = 0;
                        while n % d == 0 {
                            n /= d;
                            cnt += 1;
                        }
                        if cnt > 0 {
                            res.push((d, cnt));
                        }
                        d += if d == 2 { 1 } else { 2 };
                    }
                    if n > 1 {
                        res.push((n, 1));
                    }
                    let body = res
                        .into_iter()
                        .map(|(p, a)| {
                            if a == 1 {
                                format!("{}", p)
                            } else {
                                format!("{}^{}", p, a)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" * ");
                    res200(stream_clone(stream), &body);
                }
            } else {
                error400(stream_clone(stream), "Parámetro 'n' inválido o faltante");
            }
            true
        }

        "/pi" => {
            let iters = qmap
                .get("iters")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(100_000);
            let mut sum = 0.0f64;
            for k in 0..iters {
                let term = if k % 2 == 0 { 1.0 } else { -1.0 };
                sum += term / (2.0 * (k as f64) + 1.0);
            }
            let pi = 4.0 * sum;
            res200(stream_clone(stream), &format!("{:.10}", pi));
            true
        }

        "/mandelbrot" => {
            let w = qmap
                .get("width")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(80);
            let h = qmap
                .get("height")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(24);
            let max_iter = qmap
                .get("max_iter")
                .or_else(|| qmap.get("iter"))
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(50);

            if w == 0 || h == 0 || w > 1000 || h > 1000 || max_iter == 0 {
                error400(
                    stream_clone(stream),
                    "parámetros inválidos (1<=W,H<=1000, max_iter>0)",
                );
            } else {
                let mut iters = vec![vec![0u32; w]; h];

                for y in 0..h {
                    for x in 0..w {
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

                if let Some(fname) = qmap.get("file") {
                    let mut f = match File::create(fname) {
                        Ok(f) => f,
                        Err(e) => {
                            error400(
                                stream_clone(stream),
                                &format!("no se pudo crear {}: {}", fname, e),
                            );
                            return true;
                        }
                    };

                    let _ = f.write_all(format!("P2\n{} {}\n255\n", w, h).as_bytes());
                    for y in 0..h {
                        for x in 0..w {
                            let v = if max_iter == 0 {
                                0
                            } else {
                                (iters[y][x] as u64 * 255 / max_iter as u64) as u32
                            };
                            let _ = f.write_all(format!("{} ", v).as_bytes());
                        }
                        let _ = f.write_all(b"\n");
                    }

                    res200(stream_clone(stream), &format!("PGM escrito en {}", fname));
                    return true;
                }

                let body = serde_json::to_string(&iters).unwrap();
                res200_json(stream_clone(stream), &body);
            }
            true
        }

        "/matrixmul" => {
            let n = qmap
                .get("n")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(100);
            if n == 0 || n > 600 {
                error400(stream_clone(stream), "n inválido (1..600)");
            } else {
                let mut rng = thread_rng();
                let mut a = vec![0f64; n * n];
                let mut b = vec![0f64; n * n];
                for v in a.iter_mut() {
                    *v = rng.gen_range(0.0..1.0);
                }
                for v in b.iter_mut() {
                    *v = rng.gen_range(0.0..1.0);
                }
                let mut sum = 0.0f64;
                for i in 0..n {
                    for k in 0..n {
                        let aik = a[i * n + k];
                        for j in 0..n {
                            sum += aik * b[k * n + j];
                        }
                    }
                }
                res200(stream_clone(stream), &format!("checksum={:.6}", sum));
            }
            true
        }

        "/sortfile" => {
            if let Some(path) = qmap.get("path") {
                match read_to_string(path) {
                    Ok(s) => {
                        let mut lines: Vec<&str> = s.lines().collect();
                        lines.sort_unstable();
                        match File::create(path) {
                            Ok(mut f) => {
                                for (i, ln) in lines.iter().enumerate() {
                                    if i > 0 {
                                        let _ = f.write_all(b"\n");
                                    }
                                    let _ = f.write_all(ln.as_bytes());
                                }
                                res200(stream_clone(stream), &format!("sorted {}", lines.len()));
                            }
                            Err(e) => error400(
                                stream_clone(stream),
                                &format!("no se pudo escribir: {}", e),
                            ),
                        }
                    }
                    Err(e) => error400(stream_clone(stream), &format!("no se pudo leer: {}", e)),
                }
            } else {
                error400(stream_clone(stream), "Falta parámetro 'path'");
            }
            true
        }

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
                            if n == 0 {
                                break;
                            }
                            lines += 1;
                            bytes += buf.as_bytes().len();
                            words += buf.split_whitespace().count();
                        }

                        res200(
                            stream_clone(stream),
                            &format!("bytes={} lines={} words={}", bytes, lines, words),
                        );
                    }
                    Err(e) => error400(stream_clone(stream), &format!("no se pudo abrir: {}", e)),
                }
            } else {
                error400(stream_clone(stream), "Falta parámetro 'path'");
            }
            true
        }

        "/grep" => {
            if let (Some(path), Some(pat)) = (qmap.get("path"), qmap.get("pattern")) {
                match File::open(path) {
                    Ok(f) => {
                        let mut cnt = 0usize;
                        for line in BufReader::new(f).lines() {
                            if let Ok(l) = line {
                                if l.contains(pat) {
                                    cnt += 1;
                                }
                            }
                        }
                        res200(stream_clone(stream), &format!("matches={}", cnt));
                    }
                    Err(e) => error400(stream_clone(stream), &format!("no se pudo abrir: {}", e)),
                }
            } else {
                error400(stream_clone(stream), "Faltan 'path' y/o 'pattern'");
            }
            true
        }

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
                                    let n = rdr
                                        .by_ref()
                                        .take(64 * 1024)
                                        .read_to_end(&mut buf)
                                        .unwrap_or(0);
                                    if n == 0 {
                                        break;
                                    }
                                    let _ = enc.write_all(&buf[..n]);
                                }
                                let _ = enc.finish();
                                res200(stream_clone(stream), &format!("compressed -> {}", gz_path));
                            }
                            Err(e) => error400(
                                stream_clone(stream),
                                &format!("no se pudo crear .gz: {}", e),
                            ),
                        }
                    }
                    Err(e) => error400(stream_clone(stream), &format!("no se pudo abrir: {}", e)),
                }
            } else {
                error400(stream_clone(stream), "Falta parámetro 'path'");
            }
            true
        }

        "/hashfile" => {
            if let Some(path) = qmap.get("path") {
                match File::open(path) {
                    Ok(f) => {
                        let mut hasher = Sha256::new();
                        let mut rdr = BufReader::new(f);
                        let mut buf = [0u8; 64 * 1024];
                        loop {
                            let n = rdr.read(&mut buf).unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            hasher.update(&buf[..n]);
                        }
                        let hex = format!("{:x}", hasher.finalize());
                        res200(stream_clone(stream), &hex);
                    }
                    Err(e) => error400(stream_clone(stream), &format!("no se pudo abrir: {}", e)),
                }
            } else {
                error400(stream_clone(stream), "Falta parámetro 'path'");
            }
            true
        }

        "/simulate" => {
            let ms = qmap
                .get("ms")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let until = Instant::now() + Duration::from_millis(ms);
            let mut x = 0u64;
            while Instant::now() < until {
                x = x.wrapping_add(1);
            }
            res200(
                stream_clone(stream),
                &format!("busy {} ms, counter={}", ms, x),
            );
            true
        }

        "/loadtest" => {
            let jobs = qmap
                .get("jobs")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(10);
            let per = qmap
                .get("ms")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(50);
            let start = Instant::now();
            for _ in 0..jobs {
                let until = Instant::now() + Duration::from_millis(per);
                let mut x = 0u64;
                while Instant::now() < until {
                    x = x.wrapping_add(1);
                }
            }
            let elapsed = start.elapsed().as_millis();
            res200(
                stream_clone(stream),
                &format!("jobs={} per_ms={} total_ms={}", jobs, per, elapsed),
            );
            true
        }

        "/metrics" => {
            res200(stream_clone(stream), "metrics: use /status JSON");
            true
        }

        _ => false,
    }
}
