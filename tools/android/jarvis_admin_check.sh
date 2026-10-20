#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_admin_check.sh -- the Admin page of the JARVIS app on Linux (Xvfb + openbox) against tools/mini/fake_jarvis.py.
# FAKE_ROLE=admin (default): More > Admin opens the page, Users/Limits/Server/Logs are asked from /api/m/admin/*, a limit POST arrives.
# FAKE_ROLE=user: /api/m/me says admin=false; the app never asks /api/m/admin/* (the server would answer 403 anyway).
# Tap positions are for the 480x800 root with the 360x640 window at +52+74. PNGs: SHOTS=<dir>.
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
click 240 370; xdotool type --delay 30 justin; click 240 422; xdotool type --delay 30 pw; xdotool key Return; sleep 3
shot notice; click 240 446 3
sleep 3
click 395 714; sleep 2; shot more               # More tab
xdotool mousemove 240 400; for _ in 1 2 3 4 5 6; do xdotool click 5; done; sleep 1; shot more_scrolled
FAIL=0
chk() { if grep -q "$2" "$W/server.log"; then echo "  ok   $1"; else echo "  FAIL $1"; FAIL=$((FAIL + 1)); fi; }
nochk() { if grep -q "$2" "$W/server.log"; then echo "  FAIL $1"; FAIL=$((FAIL + 1)); else echo "  ok   $1"; fi; }
chk "me asked" '"path": "/api/m/me"'
if [ "${FAKE_ROLE:-admin}" = "user" ]; then
  nochk "no admin route asked by a normal user" '/api/m/admin/'
else
  click "${ADMIN_X:-343}" "${ADMIN_Y:-572}" 4
  shot admin_users
  chk "admin users asked" '"path": "/api/m/admin/users"'
  click 166 202 4; shot admin_limits
  chk "admin limits asked" '"path": "/api/m/admin/limits"'
  click 236 202 4; shot admin_server
  chk "admin server asked" '"path": "/api/m/admin/server"'
  click 302 202 4; shot admin_logs
  chk "admin logs asked" '"path": "/api/m/admin/logs'
  click 330 400 1; click 100 202 3
  # limits: + on sub-workers posts maxSubworkers 6
  click 166 202 3; click 228 236 3; shot admin_limit_set
  chk "limit POST" '"maxSubworkers": 6'
fi
[ $FAIL -eq 0 ] && echo "JARVIS ADMIN PASSED" || { echo "JARVIS ADMIN FAILED"; exit 1; }
