#!/usr/bin/env bash
set -euo pipefail

BASE="http://127.0.0.1:8080"
TMP_TXT="/tmp/p01_data.txt"
TMP_PGM="/tmp/mb.pgm"

banner(){ printf "\n==== %s ====\n" "$*"; }

cleanup(){
  rm -f "$TMP_TXT" "$TMP_PGM" "$TMP_TXT.gz" || true
}
trap cleanup EXIT

banner "STATUS"
curl -s "$BASE/status" | jq . || curl -s "$BASE/status" ; echo

banner "HELP"
curl -s "$BASE/help" ; echo

banner "REVERSE"
curl -sG --data-urlencode "text=Hola Mundo" "$BASE/reverse" ; echo

banner "TOUPPER"
curl -sG --data-urlencode "text=Hola Mundo" "$BASE/toupper" ; echo

banner "FIBONACCI (n=30)"
curl -s "$BASE/fibonacci?n=30" ; echo

banner "RANDOM [5,10] (3 veces)"
for _ in 1 2 3; do curl -s "$BASE/random?min=5&max=10" ; echo; done

banner "HASH (sha256 de 'hola')"
curl -sG --data-urlencode "text=hola" "$BASE/hash" ; echo

banner "TIMESTAMP (UTC ISO8601)"
curl -s "$BASE/timestamp" ; echo

banner "SLEEP 100ms"
time curl -s "$BASE/sleep?ms=100" ; echo

banner "SIMULATE 200ms"
curl -s "$BASE/simulate?ms=200" ; echo

banner "LOADTEST jobs=10 ms=50"
curl -s "$BASE/loadtest?jobs=10&ms=50" ; echo

banner "ISPRIME 97"
curl -s "$BASE/isprime?n=97" ; echo

banner "FACTOR 840"
curl -s "$BASE/factor?n=840" ; echo

banner "PI iters=200000"
curl -s "$BASE/pi?iters=200000" ; echo

banner "CREATEFILE"
curl -sG \
  --data-urlencode "path=$TMP_TXT" \
  --data-urlencode $'content=zeta\nbeta\nalfa\nfoo bar baz\nfoo\nbar' \
  "$BASE/createfile" ; echo
echo "-- contenido actual --"
cat "$TMP_TXT" ; echo

banner "SORTFILE"
curl -sG --data-urlencode "path=$TMP_TXT" "$BASE/sortfile" ; echo
echo "-- contenido ordenado --"
cat "$TMP_TXT" ; echo

banner "WORDCOUNT"
curl -sG --data-urlencode "path=$TMP_TXT" "$BASE/wordcount" ; echo

banner "GREP pattern=foo"
curl -sG --data-urlencode "path=$TMP_TXT" \
        --data-urlencode "pattern=foo" \
        "$BASE/grep" ; echo

banner "COMPRESS (gzip)"
curl -sG --data-urlencode "path=$TMP_TXT" "$BASE/compress" ; echo
ls -lh "$TMP_TXT" "$TMP_TXT.gz" || true
echo "Probar integridad gzip:"
if gzip -t "$TMP_TXT.gz" 2>/dev/null; then echo "OK gzip"; else echo "Fallo gzip"; fi

banner "HASHFILE (sha256 del archivo original)"
curl -sG --data-urlencode "path=$TMP_TXT" "$BASE/hashfile" ; echo

banner "MANDELBROT JSON pequeño (16x10, max_iter=80)"
curl -s "$BASE/mandelbrot?width=16&height=10&max_iter=80" | jq . || true

banner "MANDELBROT a PGM (200x120, max_iter=100)"
curl -s "$BASE/mandelbrot?width=200&height=120&max_iter=100&file=$TMP_PGM" ; echo
ls -lh "$TMP_PGM" || true
echo "Cabecera PGM:"
head -n 3 "$TMP_PGM" || true

banner "MATRIXMUL n=120 "
curl -s "$BASE/matrixmul?n=120" ; echo

banner "METRICS"
curl -s "$BASE/metrics" ; echo

banner "DELETEFILE de prueba"
curl -sG --data-urlencode "path=$TMP_TXT" "$BASE/deletefile" ; echo

banner "terminado"
