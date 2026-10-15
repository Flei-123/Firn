#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/mini/app_check.sh -- the mini chat app (examples/jarvis_mini) end to end on X11 against the fake JARVIS server:
# frameless + topmost + bottom right corner, login (user, password), message -> token stream -> answer in the window,
# Ctrl+Alt+J hides / shows, the next start needs no login (stored token). Needs Xvfb, openbox, xdotool, python3 websockets.
#   FIRNC=<firnc> APP_SHOTS=<dir> bash tools/mini/app_check.sh      (APP_SHOTS keeps the screenshots as PNG)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
export FIRNLIB="$ROOT/lib"
for t in Xvfb openbox xdotool xwd xwininfo xprop python3; do
    command -v "$t" >/dev/null || { echo "skip: $t missing"; exit 0; }
done
python3 -c "import websockets" 2>/dev/null || { echo "skip: python3 websockets missing"; exit 0; }
WORK=$(mktemp -d)
PIDS=""
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null; done; rm -rf "$WORK"; }
trap cleanup EXIT
FAIL=0
ok() { echo "  ok   $1"; }
bad() { echo "  FAIL $1"; FAIL=$((FAIL + 1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

"$FIRNC" --opt-level=dev-fast -o "$WORK/jmini" examples/jarvis_mini/main.fi 2>&1 | grep -v RWX
[ -x "$WORK/jmini" ] || { echo "build failed"; exit 1; }

python3 tools/mini/fake_jarvis.py "$WORK/server.log" > "$WORK/server.out" 2>"$WORK/server.err" &
PIDS="$PIDS $!"
for _ in $(seq 50); do grep -q '^PORT' "$WORK/server.out" 2>/dev/null && break; sleep 0.1; done
PORT=$(awk '/^PORT/{print $2}' "$WORK/server.out")
[ -n "$PORT" ] || { echo "fake server did not start"; exit 1; }

D=$((300 + RANDOM % 100))
Xvfb ":$D" -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
PIDS="$PIDS $!"
sleep 1
export DISPLAY=":$D" HOME="$WORK" XDG_DATA_HOME="$WORK/data"
openbox >/dev/null 2>&1 &
PIDS="$PIDS $!"
sleep 1

shot() {   # shot <name>: screenshot of the whole screen
    xwd -root -silent -out "$WORK/$1.xwd"
    if [ -n "${APP_SHOTS:-}" ]; then
        mkdir -p "$APP_SHOTS"
        python3 tools/fui/xwd2png.py "$WORK/$1.xwd" "$APP_SHOTS/$1.png" >/dev/null 2>&1
    fi
}
find_win() {
    for _ in $(seq 100); do
        W=$(xdotool search --onlyvisible --name '^JARVIS$' 2>/dev/null | head -1)
        [ -n "$W" ] && return 0
        sleep 0.1
    done
    return 1
}
log_has() { grep -q "$1" "$WORK/server.log" 2>/dev/null; }
wait_log() { for _ in $(seq 80); do log_has "$1" && return 0; sleep 0.1; done; return 1; }
geom() { xwininfo -id "$W" | awk -v k="$1" '$0 ~ k {print $NF}' | head -1; }
click_entry() {
    local x y w h
    x=$(geom "Absolute upper-left X"); y=$(geom "Absolute upper-left Y"); w=$(geom "Width"); h=$(geom "Height")
    xdotool mousemove $((x + w / 2)) $((y + h - 37)) click 1
}
type_line() { xdotool type --delay 30 "$1"; sleep 0.2; xdotool key Return; sleep 0.4; }

echo "== 1. window =="
"$WORK/jmini" "http://127.0.0.1:$PORT" > "$WORK/app1.out" 2>&1 &
APP=$!
PIDS="$PIDS $APP"
find_win || { bad "window appears"; cat "$WORK/app1.out"; exit 1; }
sleep 0.8
ok "window appears ($W)"
check "frameless (motif hints)" 'xprop -id "$W" _MOTIF_WM_HINTS 2>/dev/null | grep -q "0x2, 0x0, 0x0"'
check "topmost (_NET_WM_STATE_ABOVE)" 'xprop -id "$W" _NET_WM_STATE 2>/dev/null | grep -q ABOVE'
RIGHT=$(( $(geom "Absolute upper-left X") + $(geom "Width") )); BOTTOM=$(( $(geom "Absolute upper-left Y") + $(geom "Height") ))
check "bottom right corner ($RIGHT x $BOTTOM of 1024 x 768)" '[ $RIGHT -ge 980 ] && [ $RIGHT -le 1024 ] && [ $BOTTOM -ge 700 ] && [ $BOTTOM -le 768 ]'
shot 1_start

echo "== 2. login + message =="
click_entry
type_line "justin"
type_line "pw"
check "server saw the login" 'wait_log "\"type\": *\"login\""'
check "socket open (tz message)" 'wait_log "\"tz\""'
shot 2_login
H0=$(geom Height)
type_line "hallo"
check "server got the message" 'wait_log "\"text\": *\"hallo\""'
sleep 1.5
shot 3_answer
python3 - "$WORK/2_login.xwd" "$WORK/3_answer.xwd" <<'EOF' || bad "the answer changed nothing on the screen"
import sys
a, b = open(sys.argv[1], "rb").read(), open(sys.argv[2], "rb").read()
d = sum(1 for i in range(0, min(len(a), len(b)), 4) if a[i:i+4] != b[i:i+4])
print("  changed pixels after the answer:", d)
sys.exit(0 if d > 500 else 1)
EOF
H1=$(geom Height)
check "window height follows the chat ($H0 -> $H1)" '[ "$H1" -gt "$H0" ]'
check "window stays at the bottom edge" '[ $(( $(geom "Absolute upper-left Y") + $(geom Height) )) -ge 700 ]'

echo "== 3. hotkey =="
xdotool key ctrl+alt+j
sleep 0.6
check "Ctrl+Alt+J hides" 'xwininfo -id "$W" | grep -q "IsUnMapped"'
xdotool key ctrl+alt+j
sleep 0.6
check "Ctrl+Alt+J shows again" 'xwininfo -id "$W" | grep -q "IsViewable"'
shot 4_after_hotkey

echo "== 4. second start: stored login =="
kill "$APP" 2>/dev/null; wait "$APP" 2>/dev/null
LOGINS=$(grep -c '"type": *"login"' "$WORK/server.log")
"$WORK/jmini" "http://127.0.0.1:$PORT" > "$WORK/app2.out" 2>&1 &
APP2=$!
PIDS="$PIDS $APP2"
sleep 3
check "no second login (stored token)" '[ "$(grep -c "\"type\": *\"login\"" "$WORK/server.log")" = "$LOGINS" ]'
check "second client sent a tz message (socket open)" '[ "$(grep -c "\"tz\"" "$WORK/server.log")" -ge 2 ]'
shot 5_restart
kill "$APP2" 2>/dev/null

[ $FAIL -eq 0 ] && echo "MINI APP PASSED" || echo "MINI APP FAILED ($FAIL)"
exit $((FAIL > 0))
