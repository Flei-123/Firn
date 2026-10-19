#!/usr/bin/env bash
# a53: stale-while-revalidate render test. run 1 fills the cache; run 2 (same HOME) talks to a SLOW (or failing) chat list and must show the stored list.
set -uo pipefail
WT=${1:?}; OUT=${2:?}; MODE=${3:-slow}
cd "$WT"
export FIRNLIB="$WT/lib"; FIRNC=/root/jarvis-android-builds/firnc-a48/firnc
export LANG=de_DE.UTF-8 LANGUAGE=de
W=$(mktemp -d); PIDS=""
trap 'for p in $PIDS; do kill $p 2>/dev/null; done; rm -rf "$W"' EXIT
"$FIRNC" --opt-level=dev-fast -o "$W/jarvis" examples/jarvis_android/main.fi 2>&1 | grep -v RWX | head -30
[ -x "$W/jarvis" ] || { echo "build failed"; exit 1; }
D=$((400 + RANDOM % 80))
Xvfb ":$D" -screen 0 480x800x24 -nolisten tcp >/dev/null 2>&1 & PIDS="$PIDS $!"
sleep 1; export DISPLAY=":$D"
openbox >/dev/null 2>&1 & PIDS="$PIDS $!"
mkdir -p "$OUT"
shot() { xwd -root -silent -out "$W/s.xwd" && python3 tools/fui/xwd2png.py "$W/s.xwd" "$OUT/$1.png" >/dev/null; }
click() { xdotool mousemove "$1" "$2" click 1; sleep "${3:-0.6}"; }
export HOME="$W/home"; mkdir -p "$HOME"
start_srv() { env "$@" FAKE_FOLDERS=1 FAKE_SIDEBAR=1 FAKE_NPROJECTS=4 python3 tools/mini/fake_jarvis.py "$W/server.log" >"$W/srv.out" 2>&1 & SRV=$!; PIDS="$PIDS $SRV"
  for _ in $(seq 50); do grep -q '^PORT' "$W/srv.out" && break; sleep 0.1; done; PORT=$(awk '/^PORT/{print $2}' "$W/srv.out"); }
start_srv X=1
"$W/jarvis" "http://127.0.0.1:$PORT" >"$W/app.out" 2>&1 & APP=$!; PIDS="$PIDS $APP"
sleep 4
click 240 370; xdotool type --delay 30 justin; click 195 422; xdotool type --delay 30 pw; xdotool key Return; sleep 12
click 240 446 3
click ${HX:-80} ${HY:-106} 2; shot run1_sidebar
kill $APP; kill $SRV; sleep 1
if [ "$MODE" = fail ]; then start_srv FAKE_FAIL_CHATS=1; else start_srv FAKE_SLOW_CHATS=20000; fi
T0=$(date +%s)
"$W/jarvis" "http://127.0.0.1:$PORT" >"$W/app2.out" 2>&1 & APP2=$!; PIDS="$PIDS $APP2"
sleep 8; shot run2_start
click ${HX:-80} ${HY:-106} 2; shot run2_sidebar
echo "run2 elapsed $(( $(date +%s) - T0 ))s mode=$MODE"; tail -n 4 "$W/app2.out"
