#!/bin/bash

# Script de pruebas básicas para el servidor HTTP/1.0
# Proyecto Sistemas Operativos

echo "=== Pruebas Básicas del Servidor HTTP/1.0 ==="
echo

# Verificar si curl está disponible
if ! command -v curl &> /dev/null; then
    echo "Error: curl no está instalado. Por favor instala curl para ejecutar las pruebas."
    exit 1
fi

# Verificar si el servidor está corriendo
echo "Verificando si el servidor está corriendo..."
if ! curl -s http://127.0.0.1:8080/status > /dev/null 2>&1; then
    echo "Error: El servidor no está corriendo en http://127.0.0.1:8080"
    echo "Por favor, inicia el servidor con: cargo run"
    exit 1
fi

echo "✓ Servidor detectado"
echo

# Función para probar endpoint
test_endpoint() {
    local endpoint="$1"
    local description="$2"
    
    echo -n "Probando $description... "
    
    if response=$(curl -s -w "%{http_code}" "http://127.0.0.1:8080$endpoint" 2>/dev/null); then
        status_code="${response: -3}"
        if [ "$status_code" = "200" ]; then
            echo "✓ OK (HTTP $status_code)"
        else
            echo "✗ FALLO (HTTP $status_code)"
        fi
    else
        echo "✗ ERROR de conexión"
    fi
}

# Función para probar endpoint con parámetros
test_endpoint_with_params() {
    local endpoint="$1"
    local params="$2"
    local description="$3"
    
    echo -n "Probando $description... "
    
    if response=$(curl -s -w "%{http_code}" "http://127.0.0.1:8080$endpoint?$params" 2>/dev/null); then
        status_code="${response: -3}"
        if [ "$status_code" = "200" ]; then
            echo "✓ OK (HTTP $status_code)"
        else
            echo "✗ FALLO (HTTP $status_code)"
        fi
    else
        echo "✗ ERROR de conexión"
    fi
}

# === PRUEBAS BÁSICAS ===
echo "=== PRUEBAS BÁSICAS ==="
test_endpoint "/status" "Estado del servidor"
test_endpoint "/help" "Ayuda"
test_endpoint "/metrics" "Métricas"

echo

# === PRUEBAS DE TEXTO ===
echo "=== PRUEBAS DE TEXTO ==="
test_endpoint_with_params "/reverse" "text=hello" "Reversar texto"
test_endpoint_with_params "/toupper" "text=hello" "Convertir a mayúsculas"
test_endpoint_with_params "/hash" "text=hello" "Hash SHA256"

echo

# === PRUEBAS MATEMÁTICAS ===
echo "=== PRUEBAS MATEMÁTICAS ==="
test_endpoint_with_params "/fibonacci" "num=10" "Fibonacci"
test_endpoint_with_params "/random" "count=5&min=1&max=100" "Números aleatorios"
test_endpoint_with_params "/isprime" "n=17" "Verificar primalidad"
test_endpoint_with_params "/factor" "n=360" "Factorización"
test_endpoint_with_params "/pi" "digits=10" "Cálculo de Pi"

echo

# === PRUEBAS DE ARCHIVOS ===
echo "=== PRUEBAS DE ARCHIVOS ==="
test_endpoint_with_params "/createfile" "name=test.txt&content=Hello World" "Crear archivo"
test_endpoint_with_params "/wordcount" "name=test.txt" "Contar palabras"
test_endpoint_with_params "/grep" "name=test.txt&pattern=Hello" "Buscar patrón"
test_endpoint_with_params "/hashfile" "name=test.txt" "Hash de archivo"
test_endpoint_with_params "/deletefile" "name=test.txt" "Eliminar archivo"

echo

# === PRUEBAS DE SIMULACIÓN ===
echo "=== PRUEBAS DE SIMULACIÓN ==="
test_endpoint_with_params "/sleep" "seconds=1" "Sleep"
test_endpoint_with_params "/simulate" "seconds=1&task=test" "Simulación"
test_endpoint_with_params "/loadtest" "tasks=5&sleep=100" "Prueba de carga"

echo

# === PRUEBAS DE CÁLCULOS INTENSIVOS ===
echo "=== PRUEBAS DE CÁLCULOS INTENSIVOS ==="
test_endpoint_with_params "/mandelbrot" "width=50&height=50&max_iter=10" "Mandelbrot"
test_endpoint_with_params "/matrixmul" "size=10&seed=42" "Multiplicación de matrices"

echo

# === PRUEBAS DE JOBS ===
echo "=== PRUEBAS DE JOBS ==="
echo -n "Creando job... "
if job_response=$(curl -s "http://127.0.0.1:8080/jobs/submit?task=reverse&text=hello" 2>/dev/null); then
    if echo "$job_response" | grep -q "job_id"; then
        job_id=$(echo "$job_response" | grep -o '"job_id":"[^"]*"' | cut -d'"' -f4)
        echo "✓ OK (Job ID: $job_id)"
        
        test_endpoint_with_params "/jobs/status" "id=$job_id" "Estado del job"
        
        # Esperar un poco y verificar resultado
        sleep 2
        test_endpoint_with_params "/jobs/result" "id=$job_id" "Resultado del job"
    else
        echo "✗ FALLO - No se pudo crear job"
    fi
else
    echo "✗ ERROR de conexión"
fi

echo

# === PRUEBAS DE ERRORES ===
echo "=== PRUEBAS DE ERRORES ==="
echo -n "Probando error: parámetro faltante... "
if response=$(curl -s -w "%{http_code}" "http://127.0.0.1:8080/reverse" 2>/dev/null); then
    status_code="${response: -3}"
    if [ "$status_code" = "400" ]; then
        echo "✓ OK (HTTP $status_code)"
    else
        echo "✗ FALLO (Esperado: HTTP 400, Obtenido: HTTP $status_code)"
    fi
else
    echo "✗ ERROR de conexión"
fi

echo -n "Probando error: endpoint inexistente... "
if response=$(curl -s -w "%{http_code}" "http://127.0.0.1:8080/nonexistent" 2>/dev/null); then
    status_code="${response: -3}"
    if [ "$status_code" = "404" ]; then
        echo "✓ OK (HTTP $status_code)"
    else
        echo "✗ FALLO (Esperado: HTTP 404, Obtenido: HTTP $status_code)"
    fi
else
    echo "✗ ERROR de conexión"
fi

echo

# === RESUMEN ===
echo "=== RESUMEN ==="
echo "Pruebas completadas. Revisa los resultados arriba."
echo
echo "Para más pruebas detalladas, puedes usar:"
echo "  curl http://127.0.0.1:8080/status"
echo "  curl http://127.0.0.1:8080/metrics"
echo "  curl http://127.0.0.1:8080/help"
echo
echo "Para ver la respuesta JSON de un endpoint:"
echo "  curl http://127.0.0.1:8080/status | jq"
echo "  curl http://127.0.0.1:8080/metrics | jq"
