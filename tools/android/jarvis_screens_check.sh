#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_screens_check.sh -- the Leitstand/Needs/Tasks/Devices screens of the JARVIS app on Linux (Xvfb + openbox)
# against tools/mini/fake_jarvis.py: login, Board (projects, needs answer, worker chat), Tasks (add, done), Dev.
# Passes when the fake server saw the need answer, the task add and the task complete (the worker chat is only visible in the PNG). PNGs: SHOTS=<dir>.
# FAKE_NO_MOBILE=1 plays an older server (no /api/m/*): the app must fall back to the old routes. Default: the mobile routes are used.
# Tap positions are for the 480x800 root with the 360x640 window at +52+74 (pixel layout of fui.app).
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
export FIRNLIB="$ROOT/lib"
for t in Xvfb openbox xdotool xwd python3; do command -v "$t" >/dev/null || { echo "skip: $t missing"; exit 0; }; done
python3 -c "import websockets" 2>/dev/null || { echo "skip: python3 websockets missing"; exit 0; }
W=$(mktemp -d); PIDS=""
trap 'for p in $PIDS; do kill $p 2>/dev/null; done; rm -rf "$W"' EXIT
"$FIRNC" --opt-level=dev-fast -o "$W/jarvis" examples/jarvis_android/main.fi 2>&1 | grep -v RWX
[ -x "$W/jarvis" ] || { echo "build failed"; exit 1; }
D=$((300 + RANDOM % 80))
Xvfb ":$D" -screen 0 480x800x24 -nolisten tcp >/dev/null 2>&1 & PIDS="$PIDS $!"
sleep 1; export DISPLAY=":$D"
openbox >/dev/null 2>&1 & PIDS="$PIDS $!"
python3 tools/mini/fake_jarvis.py "$W/server.log" >"$W/srv.out" 2>&1 & PIDS="$PIDS $!"
for _ in $(seq 50); do grep -q '^PORT' "$W/srv.out" && break; sleep 0.1; done
PORT=$(awk '/^PORT/{print $2}' "$W/srv.out")
export HOME="$W/home"; mkdir -p "$HOME"
"$W/jarvis" "http://127.0.0.1:$PORT" >"$W/app.out" 2>&1 & PIDS="$PIDS $!"
shot() { [ -n "${SHOTS:-}" ] || return 0; mkdir -p "$SHOTS"; xwd -root -silent -out "$W/s.xwd" && python3 tools/fui/xwd2png.py "$W/s.xwd" "$SHOTS/$1.png" >/dev/null; }
click() { xdotool mousemove "$1" "$2" click 1; sleep "${3:-0.6}"; }
sleep 3
click 240 398; xdotool type --delay 30 justin; click 240 441; xdotool type --delay 30 pw; xdotool key Return; sleep 3
click 165 714; sleep 4; shot board
click 240 180; sleep 3; click 130 267; sleep 1.5; click 206 391; sleep 4; shot need_answered
click 363 180; sleep 3; click 130 298; sleep 4; shot worker_chat
click 245 714; sleep 4
click 200 521; xdotool type --delay 30 "call dad @2026-10-19T08:00"; click 362 521; sleep 3; shot tasks_add
click 130 220; sleep 0.5; click 137 555; sleep 3; shot tasks_done
click 320 714; sleep 4; shot devices
FAIL=0
chk() { if grep -q "$2" "$W/server.log"; then echo "  ok   $1"; else echo "  FAIL $1"; FAIL=$((FAIL + 1)); fi; }
if [ -n "${FAKE_NO_MOBILE:-}" ]; then
  # an older server: /api/m/ping answers 404, the app keeps the old routes
  chk "ping asked" '"path": "/api/m/ping"'
  chk "need answer posted (old route)" '"action": "answer", "id": "n1", "answer": "Yes"'
  chk "task add posted (old route)" '"action": "add", "text": "call dad", "due": "2026-10-19T08:00"'
  chk "task complete posted (old route)" '"action": "complete"'
  ! grep -q '"path": "/api/m/tasks' "$W/server.log" && echo "  ok   no mobile task route used" || { echo "  FAIL mobile route used on an old server"; FAIL=$((FAIL + 1)); }
else
  chk "ping asked" '"path": "/api/m/ping"'
  chk "need answer posted (mobile)" '"path": "/api/m/needs/answer".*"project": "p1", "id": "n1", "answer": "Yes"'
  chk "task add posted (mobile)" '"path": "/api/m/tasks".*"text": "call dad", "due": "2026-10-19T08:00"'
  chk "task complete posted (mobile)" '"path": "/api/m/tasks/done"'
  chk "projects over mobile route" '"path": "/api/m/projects"'
  chk "devices over mobile route" '"path": "/api/m/devices"'
fi
[ $FAIL -eq 0 ] && echo "JARVIS SCREENS PASSED" || { echo "JARVIS SCREENS FAILED"; exit 1; }
