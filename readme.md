# Servidor HTTP/1.0 – Proyecto Sistemas Operativos

Implementación en Rust de un servidor HTTP/1.0 pensado para ejercitar conceptos de sistemas operativos: multiplexación de E/S, sincronización entre hilos, control de backpressure y persistencia de trabajos de larga duración.

---

## Requisitos
- Rust 1.70 o superior (toolchain estable).
- `cargo` para compilar y ejecutar.
- Herramientas opcionales para pruebas:
  - `ab` (ApacheBench) para la prueba de carga.
  - Postman o `curl` para pruebas funcionales manuales.
  - `bash` + utilidades GNU para ejecutar los scripts de verificación.

## Compilación
```bash
cargo build --release
```
El binario resultante queda en `target/release/Proyecto_1`.

## Ejecución
```bash
# Ejecución básica en modo debug
cargo run

# Ejecutar el binario generado en release
./target/release/Proyecto_1
```

### Configuración
Los parámetros pueden indicarse por argumentos CLI o variables de entorno. Ambos caminos son equivalentes; si se usan simultáneamente, los argumentos tienen prioridad.

| Opción CLI          | Variable de entorno        | Descripción                                           | Valor por defecto      |
|---------------------|----------------------------|-------------------------------------------------------|------------------------|
| `--bind ADDR`       | `P01_BIND_ADDR`            | Dirección y puerto de escucha                         | `127.0.0.1:8080`       |
| `--workers N`       | `P01_WORKERS_PER_COMMAND`  | Hilos de trabajo por comando                          | `2`                    |
| `--max-inflight N`  | `P01_MAX_INFLIGHT`         | Solicitudes concurrentes permitidas por comando       | `32`                   |
| `--retry-after MS`  | `P01_RETRY_AFTER_MS`       | Cabecera `Retry-After` cuando se aplica backpressure  | `250` ms               |
| `--timeout MS`      | `P01_TASK_TIMEOUT_MS`      | Tiempo máximo por tarea antes de cancelar             | `60000` ms             |
| `--data-dir DIR`    | `P01_DATA_DIR`             | Directorio de trabajo para archivos temporales        | Directorio actual      |

Ejemplo completo:
```bash
P01_WORKERS_PER_COMMAND=4 P01_MAX_INFLIGHT=64 cargo run -- \
  --bind 0.0.0.0:8080 \
  --retry-after 500 \
  --timeout 90000
```

## Pruebas y Validación

### 1. Pruebas unitarias y de lógica
```bash
cargo test
```
Las pruebas cubren utilidades de parsing, manejo de jobs y estadísticas de latencia.

### 2. Pruebas automatizadas de endpoints
```bash
chmod +x test_basic.sh test_endpoints.sh test_server.sh
./test_server.sh            # Levanta el servidor, ejecuta comprobaciones y lo detiene
./test_endpoints.sh         # Requiere un servidor en ejecución en 127.0.0.1:8080
```

### 3. Prueba de carga
Con el servidor en ejecución:
```bash
ab -n 30 -c 30 http://127.0.0.1:8080/sleep?seconds=1
```
El endpoint `/sleep` se usa como tarea de latencia controlada para verificar el comportamiento bajo concurrencia.

### 4. Pruebas funcionales manuales
Las validaciones manuales se realizaron con una colección de Postman (ver captura incluida en la carpeta del proyecto). Para replicarlas:
1. Importar la colección `P01_functional_tests` en Postman.
2. Configurar la variable `{{baseUrl}}` con `http://127.0.0.1:8080`.
3. Ejecutar los requests agrupados en _Rutas felices_ y _Errores esperados_ para inspeccionar las respuestas JSON.

## Uso rápido con curl
```bash
# Estado del servidor y workers
curl http://127.0.0.1:8080/status

# Procesamiento de texto
curl "http://127.0.0.1:8080/reverse?text=hola"
curl "http://127.0.0.1:8080/toupper?text=hola"

# Operaciones numéricas
curl "http://127.0.0.1:8080/fibonacci?num=20"
curl "http://127.0.0.1:8080/random?count=5&min=10&max=99"

# Archivos
curl "http://127.0.0.1:8080/createfile?name=demo.txt&content=Hola&repeat=2"
curl "http://127.0.0.1:8080/wordcount?name=demo.txt"
curl "http://127.0.0.1:8080/compress?name=demo.txt&codec=gzip"

# Jobs asíncronos
curl "http://127.0.0.1:8080/jobs/submit?task=mandelbrot&width=80&height=40"
```

## Arquitectura

### Visión general
1. **`src/main.rs`** prepara la configuración, levanta el `TcpListener` y controla la asignación de solicitudes a los workers de cada comando. También implementa el control de backpressure y la inspección de `/status` y `/metrics`.
2. **`src/control.rs`** mantiene el estado global (`ServerState`) compuesto por estadísticas, colas por comando, información de workers y el registro persistente de jobs (`jobs_journal.json`). Expone utilidades para persistencia y cálculo de percentiles de latencia.
3. **`src/handlers.rs`** contiene la lógica de cada endpoint; las funciones trabajan sobre parámetros ya parseados y devuelven JSON uniforme mediante `ResponseMeta`.
4. **`src/errors.rs`** centraliza el formato de respuestas HTTP (errores, JSON, cabeceras adicionales) y actualiza jobs en función del resultado.

### Flujo de una solicitud
1. El hilo principal acepta la conexión y parsea la petición (solo se admiten `GET`/`HEAD`).  
2. De existir saturación (`max_in_flight_per_command`), responde `503` con cabecera `Retry-After` calculada.  
3. Cada comando está asociado a un pool de `mpsc::Sender<Task>`; el hilo principal rota (`round-robin`) sobre ellos para distribuir la carga.  
4. El worker marca su estado como ocupado, ejecuta `handle_command` con un deadline por tarea y, al finalizar, actualiza latencias y métricas.  
5. Las respuestas incluyen `X-Request-Id` y `X-Worker-Pid` para trazabilidad.

### Sistema de jobs
- Persistencia: los trabajos se guardan en `jobs_journal.json` y se recargan al iniciar (`load_jobs`).  
- Estados soportados: `pending`, `running`, `done`, `cancelled`, `error`.  
- Endpoints REST dedicados (`/jobs/submit`, `/jobs/status`, `/jobs/result`, `/jobs/cancel`).  
- Los resultados se almacenan como `serde_json::Value`, lo que permite estructurar respuestas complejas sin perder el tipo dinámico.

### Manejo de datos y archivos
- Las operaciones sobre archivos incluyen normalización de rutas para evitar traversal (`sanitize_path`).  
- Se soportan operaciones de hash (`/hash`, `/hashfile`), compresión gzip (`/compress`) y ordenamiento configurable (`/sortfile`).  
- Los datos de ejemplo viven en `data*.txt` y pueden reutilizarse para pruebas.

### Métricas y observabilidad
- `/status` expone información resumida: uptime, conexiones totales, workers activos y colas.  
- `/metrics` retorna métricas agregadas por comando, incluyendo percentiles P50/P95/P99 calculados sobre las últimas muestras (`MAX_LATENCY_SAMPLES`).  
- El contador global de solicitudes (`REQUEST_COUNTER`) permite etiquetar respuestas y jobs.

## Endpoints destacados
Las rutas están agrupadas en categorías para facilitar la exploración; todas responden JSON (salvo errores puntuales):
- Texto: `/reverse`, `/toupper`, `/hash`.
- Números: `/fibonacci`, `/random`, `/isprime`, `/factor`, `/pi`, `/mandelbrot`, `/matrixmul`.
- Archivos: `/createfile`, `/deletefile`, `/sortfile`, `/wordcount`, `/grep`, `/compress`, `/hashfile`.
- Gestión de tiempo: `/sleep`, `/simulate`, `/loadtest`, `/timestamp`.
- Monitoreo: `/status`, `/help`, `/metrics`.
- Jobs: `/jobs/submit`, `/jobs/status`, `/jobs/result`, `/jobs/cancel`.

## Recursos útiles
- Scripts auxiliares: `test_basic.sh`, `test_endpoints.sh`, `test_server.sh`.
- Datos de prueba: `data.txt`, `data2.txt`, `data.txt.gz`.
- Snapshot de pruebas manuales: captura de Postman incluidas en el documento 

