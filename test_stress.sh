#!/usr/bin/env bash
set -euo pipefail

BASE=${BASE:-http://127.0.0.1:8080}
CONCURRENCY=${CONCURRENCY:-8}
MATRIX_ROUNDS=${MATRIX_ROUNDS:-3}
MANDELBROT_ROUNDS=${MANDELBROT_ROUNDS:-2}
LOADTEST_ROUNDS=${LOADTEST_ROUNDS:-2}
SIMULATE_ROUNDS=${SIMULATE_ROUNDS:-2}
HASH_ROUNDS=${HASH_ROUNDS:-3}
WORDCOUNT_ROUNDS=${WORDCOUNT_ROUNDS:-2}
JOB_COUNT=${JOB_COUNT:-8}
JOB_TIMEOUT=${JOB_TIMEOUT:-150}
MATRIX_SIZE=${MATRIX_SIZE:-500}
MAND_WIDTH=${MAND_WIDTH:-900}
MAND_HEIGHT=${MAND_HEIGHT:-600}
MAND_ITER=${MAND_ITER:-220}
LOADTEST_TASKS=${LOADTEST_TASKS:-2200}
LOADTEST_SLEEP=${LOADTEST_SLEEP:-3}
SIMULATE_SECONDS=${SIMULATE_SECONDS:-2}
REPEAT_WRITES=${REPEAT_WRITES:-10000}

banner() {
  printf '\n==== %s ====\n' "$*"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Error: se requiere el comando '$1' en PATH" >&2
    exit 1
  fi
}

require_command curl
require_command python3

if ! curl -s --max-time 2 "$BASE/status" >/dev/null; then
  echo "Error: no se detecta un servidor activo en $BASE" >&2
  echo "Inícialo, por ejemplo: cargo run" >&2
  exit 1
fi

HAVE_JQ=0
if command -v jq >/dev/null 2>&1; then
  HAVE_JQ=1
fi

TMP_DIR=$(mktemp -d -t p01_stress.XXXXXX)
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

urlencode() {
  python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$1"
}

STRESS_CONTENT=$(printf '0123456789ABCDEF%.0s' {1..64})
LARGE_FILE="$TMP_DIR/stress_payload.txt"
ENC_LARGE_FILE=$(urlencode "$LARGE_FILE")
LARGE_ARCHIVE="${LARGE_FILE}.gz"
ENC_LARGE_ARCHIVE=$(urlencode "$LARGE_ARCHIVE")

curl_expect_200() {
  local label=$1
  shift
  local tmp
  tmp=$(mktemp "$TMP_DIR/${label//[^a-zA-Z0-9_]/_}.XXXX")
  local code
  code=$(curl -sS -o "$tmp" -w "%{http_code}" "$@")
  if [ "$code" != "200" ]; then
    echo "Error en $label (HTTP $code)" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    return 1
  fi
  rm -f "$tmp"
}

request_get() {
  local path=$1
  local code
  code=$(curl -sS -o /dev/null -w "%{http_code}" "$BASE$path")
  if [ "$code" != "200" ]; then
    echo "Fallo HTTP $code en $path" >&2
    return 1
  fi
}

run_burst() {
  local label=$1
  local concurrency=$2
  local total=$3
  local fn=$4
  banner "$label ($total requests @ $concurrency concurrentes)"
  local -a pids=()
  local errors=0
  for i in $(seq 1 "$total"); do
    (
      "$fn" "$i"
    ) &
    pids+=($!)
    if [ "${#pids[@]}" -ge "$concurrency" ]; then
      wait "${pids[0]}" || errors=$((errors + 1))
      pids=("${pids[@]:1}")
    fi
  done
  for pid in "${pids[@]}"; do
    wait "$pid" || errors=$((errors + 1))
  done
  if [ "$errors" -gt 0 ]; then
    echo "$label completado con $errors fallos" >&2
    return 1
  fi
  echo "$label completado sin fallos"
}

matrix_heavy() {
  local idx=$1
  local seed=$(( (RANDOM % 100000) + idx ))
  request_get "/matrixmul?size=$MATRIX_SIZE&seed=$seed"
}

mandelbrot_heavy() {
  local jitter=$((RANDOM % 40))
  local width=$((MAND_WIDTH - 20 + jitter))
  local height=$((MAND_HEIGHT - 20 + jitter))
  request_get "/mandelbrot?width=$width&height=$height&max_iter=$MAND_ITER"
}

loadtest_heavy() {
  request_get "/loadtest?tasks=$LOADTEST_TASKS&sleep=$LOADTEST_SLEEP"
}

simulate_heavy() {
  local tag=$((RANDOM % 1000))
  request_get "/simulate?seconds=$SIMULATE_SECONDS&task=stress_$tag"
}

hashfile_heavy() {
  request_get "/hashfile?name=$ENC_LARGE_FILE"
}

wordcount_heavy() {
  request_get "/wordcount?name=$ENC_LARGE_FILE"
}

submit_job() {
  local tmp
  tmp=$(mktemp "$TMP_DIR/job_submit.XXXX")
  local code
  code=$(curl -sS -o "$tmp" -w "%{http_code}" \
    "$BASE/jobs/submit?task=mandelbrot&width=$MAND_WIDTH&height=$MAND_HEIGHT&max_iter=$MAND_ITER")
  if [ "$code" != "200" ]; then
    echo "Error al encolar job (HTTP $code)" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    return 1
  fi
  local job_id
  job_id=$(grep -o '"job_id":"[^"]*"' "$tmp" | cut -d'"' -f4)
  rm -f "$tmp"
  if [ -z "$job_id" ]; then
    echo "No se pudo extraer job_id de la respuesta" >&2
    return 1
  fi
  echo "$job_id"
}

wait_job_until_done() {
  local job_id=$1
  local elapsed=0
  while [ "$elapsed" -lt "$JOB_TIMEOUT" ]; do
    local tmp
    tmp=$(mktemp "$TMP_DIR/job_status.XXXX")
    local code
    code=$(curl -sS -o "$tmp" -w "%{http_code}" \
      "$BASE/jobs/status?id=$job_id")
    if [ "$code" != "200" ]; then
      echo "Job $job_id -> fallo al consultar estado (HTTP $code)" >&2
      cat "$tmp" >&2
      rm -f "$tmp"
      sleep 1
      elapsed=$((elapsed + 1))
      continue
    fi
    local status
    status=$(grep -o '"status":"[^"]*"' "$tmp" | cut -d'"' -f4)
    rm -f "$tmp"
    case "$status" in
      done|failed|cancelled)
        echo "Job $job_id finalizó con estado: $status"
        return 0
        ;;
    esac
    sleep 1
    elapsed=$((elapsed + 1))
  done
  echo "Job $job_id no finalizó en ${JOB_TIMEOUT}s" >&2
  return 1
}

show_job_result() {
  local job_id=$1
  local tmp
  tmp=$(mktemp "$TMP_DIR/job_result.XXXX")
  local code
  code=$(curl -sS -o "$tmp" -w "%{http_code}" \
    "$BASE/jobs/result?id=$job_id")
  if [ "$code" != "200" ]; then
    echo "Job $job_id -> error al obtener resultado (HTTP $code)" >&2
    cat "$tmp" >&2
  else
    printf 'Job %s resultado: %s...\n' "$job_id" "$(head -c 120 "$tmp")"
  fi
  rm -f "$tmp"
}

banner "Servidor detectado en $BASE"
if [ "$HAVE_JQ" -eq 1 ]; then
  curl -s "$BASE/status" | jq '{uptime_seconds, workers, queues, config, jobs}'
else
  curl -s "$BASE/status"
fi

banner "Creando dataset grande en $LARGE_FILE"
curl_expect_200 "createfile" -G \
  --data-urlencode "name=$LARGE_FILE" \
  --data-urlencode "content=$STRESS_CONTENT" \
  --data-urlencode "repeat=$REPEAT_WRITES" \
  "$BASE/createfile"

banner "Rondas CPU intensivas: /matrixmul"
run_burst "matrixmul" "$CONCURRENCY" "$((CONCURRENCY * MATRIX_ROUNDS))" matrix_heavy

banner "Rondas CPU intensivas: /mandelbrot"
run_burst "mandelbrot" "$CONCURRENCY" "$((CONCURRENCY * MANDELBROT_ROUNDS))" mandelbrot_heavy

banner "Rondas CPU intensivas: /loadtest"
run_burst "loadtest" "$CONCURRENCY" "$((CONCURRENCY * LOADTEST_ROUNDS))" loadtest_heavy

banner "Rondas CPU intensivas: /simulate"
run_burst "simulate" "$CONCURRENCY" "$((CONCURRENCY * SIMULATE_ROUNDS))" simulate_heavy

banner "Lecturas pesadas sobre archivo grande: /hashfile"
run_burst "hashfile" "$CONCURRENCY" "$((CONCURRENCY * HASH_ROUNDS))" hashfile_heavy

banner "Lecturas pesadas sobre archivo grande: /wordcount"
run_burst "wordcount" "$CONCURRENCY" "$((CONCURRENCY * WORDCOUNT_ROUNDS))" wordcount_heavy

banner "Comprimiendo dataset con /compress"
curl_expect_200 "compress" -G \
  --data-urlencode "name=$LARGE_FILE" \
  --data-urlencode "codec=gzip" \
  "$BASE/compress"

banner "Encolando jobs pesados ($JOB_COUNT envíos)"
job_ids=()
for _ in $(seq 1 "$JOB_COUNT"); do
  job_id=$(submit_job)
  job_ids+=("$job_id")
done

banner "Esperando finalización de jobs"
for job_id in "${job_ids[@]}"; do
  wait_job_until_done "$job_id"
done

banner "Mostrando primeros resultados de jobs"
for job_id in "${job_ids[@]}"; do
  show_job_result "$job_id"
done

banner "Limpiando archivos temporales vía API"
curl_expect_200 "deletefile_original" -G \
  --data-urlencode "name=$LARGE_FILE" \
  "$BASE/deletefile"
curl_expect_200 "deletefile_gzip" -G \
  --data-urlencode "name=$LARGE_ARCHIVE" \
  "$BASE/deletefile"

banner "Métricas tras la sobrecarga"
if [ "$HAVE_JQ" -eq 1 ]; then
  curl -s "$BASE/metrics" | jq '{latency_ms, commands}'
else
  curl -s "$BASE/metrics"
fi

echo
echo "Stress test completado"
