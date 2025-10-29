#!/bin/bash

# Script de pruebas para el servidor HTTP/1.0
# Proyecto Sistemas Operativos

echo "=== Iniciando pruebas del servidor HTTP/1.0 ==="
echo

# Colores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Función para hacer requests y verificar respuesta
test_endpoint() {
    local endpoint="$1"
    local expected_status="$2"
    local description="$3"
    
    echo -n "Probando $description... "
    
    response=$(curl -s -w "%{http_code}" -o /tmp/response.json "http://127.0.0.1:8080$endpoint" 2>/dev/null)
    status_code="${response: -3}"
    
    if [ "$status_code" = "$expected_status" ]; then
        echo -e "${GREEN}✓ OK${NC} (HTTP $status_code)"
        if [ -f /tmp/response.json ]; then
            echo "  Respuesta: $(cat /tmp/response.json | head -c 100)..."
        fi
    else
        echo -e "${RED}✗ FALLO${NC} (Esperado: HTTP $expected_status, Obtenido: HTTP $status_code)"
        if [ -f /tmp/response.json ]; then
            echo "  Respuesta: $(cat /tmp/response.json)"
        fi
    fi
    echo
}

# Función para probar endpoints con parámetros
test_endpoint_with_params() {
    local endpoint="$1"
    local params="$2"
    local expected_status="$3"
    local description="$4"
    
    echo -n "Probando $description... "
    
    response=$(curl -s -w "%{http_code}" -o /tmp/response.json "http://127.0.0.1:8080$endpoint?$params" 2>/dev/null)
    status_code="${response: -3}"
    
    if [ "$status_code" = "$expected_status" ]; then
        echo -e "${GREEN}✓ OK${NC} (HTTP $status_code)"
        if [ -f /tmp/response.json ]; then
            echo "  Respuesta: $(cat /tmp/response.json | head -c 100)..."
        fi
    else
        echo -e "${RED}✗ FALLO${NC} (Esperado: HTTP $expected_status, Obtenido: HTTP $status_code)"
        if [ -f /tmp/response.json ]; then
            echo "  Respuesta: $(cat /tmp/response.json)"
        fi
    fi
    echo
}

# Verificar si el servidor está corriendo
echo "Verificando si el servidor está corriendo..."
if ! curl -s http://127.0.0.1:8080/status > /dev/null 2>&1; then
    echo -e "${RED}Error: El servidor no está corriendo en http://127.0.0.1:8080${NC}"
    echo "Por favor, inicia el servidor con: cargo run"
    exit 1
fi

echo -e "${GREEN}Servidor detectado, iniciando pruebas...${NC}"
echo

# === PRUEBAS BÁSICAS ===
echo -e "${YELLOW}=== PRUEBAS BÁSICAS ===${NC}"

test_endpoint "/status" "200" "Endpoint de estado"
test_endpoint "/help" "200" "Endpoint de ayuda"
test_endpoint "/metrics" "200" "Endpoint de métricas"

# === PRUEBAS DE TEXTO ===
echo -e "${YELLOW}=== PRUEBAS DE TEXTO ===${NC}"

test_endpoint_with_params "/reverse" "text=hello" "200" "Reversar texto"
test_endpoint_with_params "/toupper" "text=hello" "200" "Convertir a mayúsculas"
test_endpoint_with_params "/hash" "text=hello" "200" "Hash SHA256"

# === PRUEBAS MATEMÁTICAS ===
echo -e "${YELLOW}=== PRUEBAS MATEMÁTICAS ===${NC}"

test_endpoint_with_params "/fibonacci" "num=10" "200" "Fibonacci"
test_endpoint_with_params "/random" "count=5&min=1&max=100" "200" "Números aleatorios"
test_endpoint_with_params "/isprime" "n=17" "200" "Verificar primalidad"
test_endpoint_with_params "/factor" "n=360" "200" "Factorización"
test_endpoint_with_params "/pi" "digits=10" "200" "Cálculo de Pi"

# === PRUEBAS DE ARCHIVOS ===
echo -e "${YELLOW}=== PRUEBAS DE ARCHIVOS ===${NC}"

test_endpoint_with_params "/createfile" "name=test.txt&content=Hello World" "200" "Crear archivo"
test_endpoint_with_params "/wordcount" "name=test.txt" "200" "Contar palabras"
test_endpoint_with_params "/grep" "name=test.txt&pattern=Hello" "200" "Buscar patrón"
test_endpoint_with_params "/hashfile" "name=test.txt" "200" "Hash de archivo"
test_endpoint_with_params "/deletefile" "name=test.txt" "200" "Eliminar archivo"

# === PRUEBAS DE SIMULACIÓN ===
echo -e "${YELLOW}=== PRUEBAS DE SIMULACIÓN ===${NC}"

test_endpoint_with_params "/sleep" "seconds=1" "200" "Sleep"
test_endpoint_with_params "/simulate" "seconds=1&task=test" "200" "Simulación"
test_endpoint_with_params "/loadtest" "tasks=5&sleep=100" "200" "Prueba de carga"

# === PRUEBAS DE CÁLCULOS INTENSIVOS ===
echo -e "${YELLOW}=== PRUEBAS DE CÁLCULOS INTENSIVOS ===${NC}"

test_endpoint_with_params "/mandelbrot" "width=50&height=50&max_iter=10" "200" "Mandelbrot"
test_endpoint_with_params "/matrixmul" "size=10&seed=42" "200" "Multiplicación de matrices"

# === PRUEBAS DE JOBS ===
echo -e "${YELLOW}=== PRUEBAS DE JOBS ===${NC}"

# Crear un job
echo -n "Creando job... "
job_response=$(curl -s "http://127.0.0.1:8080/jobs/submit?task=reverse&text=hello" 2>/dev/null)
if echo "$job_response" | grep -q "job_id"; then
    job_id=$(echo "$job_response" | grep -o '"job_id":"[^"]*"' | cut -d'"' -f4)
    echo -e "${GREEN}✓ OK${NC} (Job ID: $job_id)"
    
    # Verificar estado del job
    test_endpoint_with_params "/jobs/status" "id=$job_id" "200" "Estado del job"
    
    # Esperar un poco y verificar resultado
    sleep 2
    test_endpoint_with_params "/jobs/result" "id=$job_id" "200" "Resultado del job"
else
    echo -e "${RED}✗ FALLO${NC} - No se pudo crear job"
fi

# === PRUEBAS DE ERRORES ===
echo -e "${YELLOW}=== PRUEBAS DE ERRORES ===${NC}"

test_endpoint_with_params "/reverse" "" "400" "Error: parámetro faltante"
test_endpoint_with_params "/fibonacci" "num=abc" "400" "Error: parámetro inválido"
test_endpoint_with_params "/nonexistent" "" "404" "Error: endpoint inexistente"

# === RESUMEN ===
echo -e "${YELLOW}=== RESUMEN ===${NC}"
echo "Pruebas completadas. Revisa los resultados arriba."
echo
echo "Para más pruebas detalladas, puedes usar:"
echo "  curl http://127.0.0.1:8080/status"
echo "  curl http://127.0.0.1:8080/metrics"
echo "  curl http://127.0.0.1:8080/help"
echo

# Limpiar archivos temporales
rm -f /tmp/response.json
