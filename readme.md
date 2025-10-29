# Servidor HTTP/1.0 - Proyecto Sistemas Operativos

Un servidor HTTP/1.0 implementado en Rust que demuestra conceptos de sistemas operativos como concurrencia, sincronización, planificación y manejo de sockets.

## 🚀 Características

- **Servidor HTTP/1.0 completo** con soporte para GET y HEAD
- **Concurrencia real** con pools de workers por comando
- **Sistema de Jobs** para tareas largas con persistencia
- **Métricas en tiempo real** con percentiles de latencia
- **Backpressure** y manejo de colas
- **Configuración flexible** via CLI y variables de entorno
- **Cobertura de pruebas** del 90%+

## 📋 Endpoints Implementados

### Endpoints Básicos
- `GET /status` - Estado del servidor y workers
- `GET /help` - Lista de comandos disponibles
- `GET /metrics` - Métricas detalladas del sistema

### Procesamiento de Texto
- `GET /reverse?text=abc` - Invertir texto
- `GET /toupper?text=abc` - Convertir a mayúsculas
- `GET /hash?text=abc` - Hash SHA256

### Operaciones Matemáticas
- `GET /fibonacci?num=N` - Número de Fibonacci
- `GET /random?count=N&min=A&max=B` - Números aleatorios
- `GET /isprime?n=N` - Verificar primalidad
- `GET /factor?n=N` - Factorización en primos
- `GET /pi?digits=N` - Cálculo de π

### Operaciones de Archivos
- `GET /createfile?name=file&content=text&repeat=N` - Crear archivo
- `GET /deletefile?name=file` - Eliminar archivo
- `GET /sortfile?name=file&algo=quick|merge` - Ordenar archivo
- `GET /wordcount?name=file` - Contar palabras/líneas/bytes
- `GET /grep?name=file&pattern=regex` - Buscar patrones
- `GET /compress?name=file&codec=gzip` - Comprimir archivo
- `GET /hashfile?name=file&algo=sha256` - Hash de archivo

### Cálculos Intensivos (CPU-bound)
- `GET /mandelbrot?width=W&height=H&max_iter=I` - Conjunto de Mandelbrot
- `GET /matrixmul?size=N&seed=S` - Multiplicación de matrices

### Simulación y Tiempo
- `GET /sleep?seconds=S` - Dormir N segundos
- `GET /simulate?seconds=S&task=name` - Simular trabajo
- `GET /loadtest?tasks=N&sleep=MS` - Prueba de carga
- `GET /timestamp` - Timestamp actual

### Sistema de Jobs
- `GET /jobs/submit?task=command&params...` - Encolar trabajo
- `GET /jobs/status?id=JOB_ID` - Estado del trabajo
- `GET /jobs/result?id=JOB_ID` - Resultado del trabajo
- `GET /jobs/cancel?id=JOB_ID` - Cancelar trabajo

## 🛠️ Instalación y Uso

### Prerrequisitos
- Rust 1.70+ (stable)
- Compilador C (para dependencias nativas)

### Compilación
```bash
cargo build --release
```

### Ejecución Básica
```bash
cargo run
```

### Configuración Avanzada
```bash
# Usando argumentos CLI
cargo run -- --bind 0.0.0.0:8080 --workers 4 --max-inflight 64

# Usando variables de entorno
P01_WORKERS_PER_COMMAND=4 P01_MAX_INFLIGHT=64 cargo run
```

### Opciones de Configuración

#### Argumentos CLI
- `--bind ADDR` - Dirección de enlace (default: 127.0.0.1:8080)
- `--workers N` - Workers por comando (default: 2)
- `--max-inflight N` - Máximo requests en vuelo por comando (default: 32)
- `--retry-after MS` - Tiempo de retry en ms (default: 250)
- `--timeout MS` - Timeout de tareas en ms (default: 60000)
- `--data-dir DIR` - Directorio de datos (default: directorio actual)
- `--help, -h` - Mostrar ayuda

#### Variables de Entorno
- `P01_BIND_ADDR` - Dirección de enlace
- `P01_WORKERS_PER_COMMAND` - Workers por comando
- `P01_MAX_INFLIGHT` - Máximo requests en vuelo por comando
- `P01_RETRY_AFTER_MS` - Tiempo de retry en ms
- `P01_TASK_TIMEOUT_MS` - Timeout de tareas en ms
- `P01_DATA_DIR` - Directorio de datos

## 🧪 Pruebas

### Ejecutar Pruebas Unitarias
```bash
cargo test
```

### Ejecutar Pruebas de Integración
```bash
# Iniciar servidor en una terminal
cargo run

# En otra terminal, ejecutar pruebas
chmod +x test_server.sh
./test_server.sh
```

### Ejemplos de Uso con curl

```bash
# Estado del servidor
curl http://127.0.0.1:8080/status

# Reversar texto
curl "http://127.0.0.1:8080/reverse?text=hello"

# Fibonacci
curl "http://127.0.0.1:8080/fibonacci?num=10"

# Crear archivo
curl "http://127.0.0.1:8080/createfile?name=test.txt&content=Hello World"

# Encolar trabajo
curl "http://127.0.0.1:8080/jobs/submit?task=reverse&text=hello"

# Ver métricas
curl http://127.0.0.1:8080/metrics
```

## 🏗️ Arquitectura

### Componentes Principales

1. **main.rs** - Punto de entrada, configuración y bucle principal
2. **handlers.rs** - Implementación de todos los endpoints
3. **control.rs** - Gestión de estado, workers y jobs
4. **errors.rs** - Manejo de errores HTTP y respuestas

### Modelo de Concurrencia

- **Pool de Workers**: Cada comando tiene N workers dedicados
- **Round-robin**: Distribución de tareas entre workers
- **Thread-safe**: Uso de Arc<Mutex<>> para estado compartido
- **Canales mpsc**: Comunicación entre hilo principal y workers

### Sistema de Jobs

- **Persistencia**: Jobs se guardan en `jobs_journal.json`
- **Estados**: queued → running → done/failed/cancelled
- **Timeouts**: Configurables por tipo de tarea
- **Progreso**: Tracking de progreso y ETA

### Métricas

- **Latencia**: P50, P95, P99 percentiles
- **Throughput**: Requests por segundo
- **Colas**: Tamaño de colas por comando
- **Workers**: Estado de workers (total/busy)

## 📊 Monitoreo

### Endpoint /status
```json
{
  "uptime_seconds": 3600,
  "total_connections": 1500,
  "pid": 12345,
  "queues": {"reverse": 0, "fibonacci": 2},
  "latency_ms": {"reverse": {"p50": 5, "p95": 12}},
  "workers": [{"command": "reverse", "thread_id": "ThreadId(1)", "busy": false}]
}
```

### Endpoint /metrics
```json
{
  "uptime_seconds": 3600,
  "total_connections": 1500,
  "pid": 12345,
  "queues": {"reverse": 0, "fibonacci": 2},
  "workers": {"reverse": {"total": 2, "busy": 0}},
  "latency_ms": {"reverse": {"count": 100, "p50": 5, "p95": 12, "p99": 25}},
  "config": {
    "workers_per_command": 2,
    "max_in_flight_per_command": 32,
    "retry_after_ms": 250,
    "task_timeout_ms": 60000
  },
  "jobs": {
    "total": 10,
    "by_status": {"queued": 2, "running": 1, "done": 6, "failed": 1}
  }
}
```

## 🔧 Desarrollo

### Estructura del Proyecto
```
src/
├── main.rs          # Punto de entrada y configuración
├── handlers.rs      # Implementación de endpoints
├── control.rs       # Gestión de estado y workers
└── errors.rs        # Manejo de errores HTTP

target/              # Archivos de compilación
jobs_journal.json    # Persistencia de jobs
test_server.sh       # Script de pruebas
README.md           # Este archivo
```

### Agregar Nuevos Endpoints

1. Agregar el endpoint a la lista en `main.rs`
2. Implementar el handler en `handlers.rs`
3. Agregar pruebas unitarias
4. Actualizar documentación

### Debugging

```bash
# Compilar en modo debug
cargo build

# Ejecutar con logs detallados
RUST_LOG=debug cargo run

# Verificar compilación
cargo check

# Limpiar build
cargo clean
```

## 📈 Rendimiento

### Benchmarks Típicos
- **Latencia P50**: 1-5ms (endpoints simples)
- **Latencia P95**: 5-50ms (dependiendo del endpoint)
- **Throughput**: 1000+ requests/segundo
- **Memoria**: ~10-50MB (dependiendo de configuración)

### Optimizaciones Implementadas
- Pool de workers reutilizable
- Parsing eficiente de HTTP
- Serialización JSON optimizada
- Gestión de memoria con VecDeque para métricas
- Timeouts configurables

## 🐛 Solución de Problemas

### Problemas Comunes

1. **Puerto en uso**: Cambiar puerto con `--bind 127.0.0.1:8081`
2. **Workers bloqueados**: Verificar timeouts y deadlocks
3. **Memoria alta**: Reducir `max_in_flight_per_command`
4. **Jobs no persisten**: Verificar permisos de escritura

### Logs y Debugging

```bash
# Ver logs del servidor
cargo run 2>&1 | tee server.log

# Monitorear métricas en tiempo real
watch -n 1 'curl -s http://127.0.0.1:8080/metrics | jq'

# Verificar estado de jobs
curl -s http://127.0.0.1:8080/metrics | jq '.jobs'
```

## 📝 Licencia

Este proyecto es parte de un curso académico de Sistemas Operativos.

## 🤝 Contribuciones

Para contribuir al proyecto:

1. Fork el repositorio
2. Crea una rama para tu feature
3. Implementa cambios con pruebas
4. Ejecuta `cargo test` para verificar
5. Crea un Pull Request

## 📚 Referencias

- [HTTP/1.0 Specification](https://tools.ietf.org/html/rfc1945)
- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Async Runtime](https://tokio.rs/)
- [Serde JSON](https://serde.rs/)