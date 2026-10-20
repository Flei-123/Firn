#!/usr/bin/env bash
# a61: ETag / 304 render + request-count test. Run 1 (online, empty store) fills list + ETag; run 2 (same HOME) must send If-None-Match,
# get a 304 and keep the sidebar filled. MODE=old: FAKE_NO_ETAG=1 (older server) -> no If-None-Match, no 304, sidebar filled as before.
set -uo pipefail
WT=${1:?}; OUT=${2:?}; MODE=${3:-new}
cd "$WT"
export FIRNLIB="$WT/lib"; FIRNC=/root/jarvis-android-builds/firnc-a48/firnc
export LANG=de_DE.UTF-8 LANGUAGE=de
W=$(mktemp -d); PIDS=""
trap 'for p in $PIDS; do kill $p 2>/dev/null; done; rm -rf "$W"' EXIT
"$FIRNC" --opt-level=dev-fast -o "$W/jarvis" examples/jarvis_android/main.fi 2>&1 | grep -v RWX | head -30
[ -x "$W/jarvis" ] || { echo "build failed"; exit 1; }
D=$((480 + RANDOM % 80))
Xvfb ":$D" -screen 0 480x800x24 -nolisten tcp >/dev/null 2>&1 & PIDS="$PIDS $!"
sleep 1; export DISPLAY=":$D"
openbox >/dev/null 2>&1 & PIDS="$PIDS $!"
mkdir -p "$OUT"
shot() { xwd -root -silent -out "$W/s.xwd" && python3 tools/fui/xwd2png.py "$W/s.xwd" "$OUT/$1.png" >/dev/null; }
click() { xdotool mousemove "$1" "$2" click 1; sleep "${3:-0.6}"; }
export HOME="$W/home"; mkdir -p "$HOME"
EXTRA=""; [ "$MODE" = old ] && EXTRA="FAKE_NO_ETAG=1"
FP=$((20000 + RANDOM % 20000))
start_srv() { : >"$W/server.log"; env X=1 FAKE_STABLE_CHATS=1 FAKE_PORT=$FP $EXTRA FAKE_FOLDERS=1 FAKE_SIDEBAR=1 FAKE_NPROJECTS=4 python3 tools/mini/fake_jarvis.py "$W/server.log" >"$W/srv.out" 2>&1 & SRV=$!; PIDS="$PIDS $SRV"
  for _ in $(seq 50); do grep -q '^PORT' "$W/srv.out" && break; sleep 0.1; done; PORT=$(awk '/^PORT/{print $2}' "$W/srv.out"); }
count() { grep "\"type\": \"rest\"" "$W/server.log" | grep -c "/api/m/chats?limit=100&offset=0"; }
start_srv
"$W/jarvis" "http://127.0.0.1:$PORT" >"$W/app.out" 2>&1 & APP=$!; PIDS="$PIDS $APP"
sleep 4
click 240 370; xdotool type --delay 30 justin; click 195 422; xdotool type --delay 30 pw; xdotool key Return; sleep 12
click 240 446 3
click ${HX:-80} ${HY:-106} 2; shot run1_sidebar
echo "run1 chat-list requests: $(count), with If-None-Match: $(grep '"path": "/api/m/chats?' "$W/server.log" | grep -c '"inm"'), 304s: $(grep -c '"what": "304"' "$W/server.log")"
cp "$W/server.log" "$OUT/server1.log"
kill $APP; kill $SRV; sleep 1
start_srv
"$W/jarvis" "http://127.0.0.1:$PORT" >"$W/app2.out" 2>&1 & APP2=$!; PIDS="$PIDS $APP2"
sleep 10; shot run2_start
click ${HX:-80} ${HY:-106} 2; shot run2_sidebar
sleep 8
echo "run2 chat-list requests: $(count), with If-None-Match: $(grep '"path": "/api/m/chats?' "$W/server.log" | grep -c '"inm"'), 304s: $(grep -c '"what": "304"' "$W/server.log")"
cp "$W/server.log" "$OUT/server2.log"
find "$HOME" -name 'etag-*' | head -3
