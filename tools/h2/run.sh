#!/bin/bash
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
export FIRNLIB="$ROOT/lib"
D="$(mktemp -d)"
SP=""
trap '[ -n "$SP" ] && kill $SP 2>/dev/null; rm -rf "$D"' EXIT
fail=0
pip download --no-deps -q -d "$D/whl" h2 hpack hyperframe >/dev/null 2>&1 || { echo "pip download failed"; exit 1; }
for f in "$D"/whl/*.whl; do python3 -I -m zipfile -e "$f" "$D/py" >/dev/null; done
for t in hpack_tool h2_client_check h2_fuzz_client; do
  "$FIRNC" "$ROOT/tools/h2/$t.fi" -o "$D/$t" || { echo "compile $t failed"; exit 1; }
done
for seed in 1 2 3; do
  python3 -I "$ROOT/tools/h2/hpack_cross.py" "$D/py" "$D/hpack_tool" $seed || fail=1
done
python3 -I "$ROOT/tools/h2/hpack_fuzz.py" "$D/py" "$D/hpack_tool" 1500 || fail=1
python3 -I "$ROOT/tools/h2/h2_fuzz.py" "$D/h2_fuzz_client" 400 || fail=1
python3 -I "$ROOT/tools/h2/h2_server.py" "$D/py" "$D/port" & SP=$!
for i in $(seq 50); do [ -s "$D/port" ] && break; sleep 0.1; done
timeout 120 "$D/h2_client_check" < "$D/port" || fail=1
kill $SP; SP=""
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout "$D/cert.pem.key" -out "$D/cert.pem" -days 2 -subj /CN=localhost -addext subjectAltName=DNS:localhost >/dev/null 2>&1
rm -f "$D/port"
python3 -I "$ROOT/tools/h2/h2_server.py" "$D/py" "$D/port" "$D/cert.pem" & SP=$!
for i in $(seq 50); do [ -s "$D/port" ] && break; sleep 0.1; done
(cat "$D/port"; echo; printf '%s' "$D/cert.pem") | timeout 120 "$D/h2_client_check" || fail=1
mkdir -p "$D/sc"
"$FIRNC" "$ROOT/tools/http/test_main.fi" -o "$D/h2srv" || { echo "compile h2srv failed"; exit 1; }
timeout 600 python3 -I "$ROOT/tools/h2/server_check.py" "$D/py" "$D/h2srv" "$D/sc" "${H2_FUZZ_RUNS:-1500}" || fail=1
[ $fail = 0 ] && echo "h2 all ok" || echo "h2 FAILED"
exit $fail
