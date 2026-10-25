#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc; [ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$PWD/lib"
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-webdav.XXXXXX")
PID=""
cleanup() { [ -n "$PID" ] && kill "$PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
ERR=0
LEVELS="${WEBDAV_LEVELS:-dev-fast release-safe release-fast}"
start_server() {
  [ -n "$PID" ] && { kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; }
  rm -rf "$W/root" "$W/port"; mkdir -p "$W/root"
  python3 -I tools/webdav/fake_dav.py --root "$W/root" --port-file "$W/port" "$@" &
  PID=$!
  for _ in $(seq 1 100); do [ -s "$W/port" ] && break; sleep 0.05; done
  PORT=$(cat "$W/port")
}
for lvl in $LEVELS; do
  $FIRNC --opt-level=$lvl -o "$W/e2e.$lvl" tools/webdav/e2e_main.fi 2> "$W/err" || { echo "FAIL compile $lvl"; head -5 "$W/err"; ERR=1; continue; }
  run() {
    local name=$1 mode=$2 user=$3 pass=$4; shift 4
    start_server "$@"
    out=$(timeout 120 "$W/e2e.$lvl" "http://127.0.0.1:$PORT/dav" "$user" "$pass" "$mode" 2>&1)
    rc=$?
    echo "  $lvl $name: rc=$rc $(echo "$out" | tail -3 | tr '\n' ' ')"
    [ $rc -eq 0 ] || ERR=1
  }
  run none full "" "" --auth none
  run basic full alice 's3cret "q"' --auth basic
  run digest-md5 full alice 's3cret "q"' --auth digest
  run digest-sha256 full alice 's3cret "q"' --auth digest-sha256
  run digest-legacy full alice 's3cret "q"' --auth digest-legacy
  run default-ns full "" "" --auth none --ns default
  run alt-ns full "" "" --auth none --ns alt
  run redirect-dirs full "" "" --auth none --redirect-dirs
  run basic-badauth badauth alice wrong --auth basic
  run digest-badauth badauth alice wrong --auth digest
  run noauth-401 noauth "" "" --auth digest
  run cross-origin-redirect redirects alice 's3cret "q"' --auth digest
  run basic-plain-refused plainrefused alice x --auth basic
done
exit $ERR
