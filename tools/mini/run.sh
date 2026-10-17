#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/mini/run.sh -- the pieces of the JARVIS mini chat box held against the outside:
#   1. net.jarvisc against a fake JARVIS server (Python, websockets): login, socket,
#      events, history, confirm/ask/stop, reconnect + queue, stored login (std.secret)
#   2. fui.looppump: a window and an async.loop timer in one thread (Xvfb)
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=$(mktemp -d); trap 'rm -rf "$WORK"' EXIT
rc=0
"$FIRNC" --opt-level=dev-fast -o "$WORK/client" tools/mini/client_main.fi 2>&1 | grep -v RWX
"$FIRNC" --opt-level=dev-fast -o "$WORK/pump" tools/mini/pump_main.fi 2>&1 | grep -v RWX
echo "== 1. jarvisc vs fake server =="
python3 tools/mini/check.py "$WORK/client" | tail -3 || rc=1
"$FIRNC" --opt-level=dev-fast -o "$WORK/apiclient" tools/mini/api_main.fi 2>&1 | grep -v RWX
echo "== 1b. jarvisapi + jarvis_cache vs fake server (REST, offline cache) =="
python3 tools/mini/api_check.py "$WORK/apiclient" | tail -3 || rc=1
echo "== 2. window + loop in one thread (Xvfb) =="
if command -v Xvfb >/dev/null; then
    d=$((200 + RANDOM % 50))
    Xvfb ":$d" -screen 0 640x480x24 -nolisten tcp >/dev/null 2>&1 & xp=$!
    sleep 1
    out=$(DISPLAY=":$d" timeout 20 "$WORK/pump" 2>&1); code=$?
    kill $xp 2>/dev/null
    echo "$out" | tr '\n' ' '; echo "(exit $code)"
    [ $code -eq 0 ] && [ "$(echo "$out" | grep -c '^TICK')" -eq 3 ] || { echo "  FAIL pump"; rc=1; }
else echo "  skip: no Xvfb"; fi
[ $rc -eq 0 ] && echo "MINI PASSED" || echo "MINI FAILED"
exit $rc
