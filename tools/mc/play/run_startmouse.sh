#!/usr/bin/env bash
# tools/mc/play/run_startmouse.sh -- start screen mouse + server list under Xvfb (no Java, no server needed).
# Click a list row -> fields filled; Remove -> entry gone; double-click a row -> join, config keeps the list (joined server first).
# Prints `RESULT mc-play-startmouse: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); XV_PID=""; MP_PID=""
cleanup() { [ -n "$MP_PID" ] && kill "$MP_PID" 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-startmouse: FAIL"; exit 1; }
for t in Xvfb xdotool; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --no-opt -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
N=120; while [ -e "/tmp/.X$N-lock" ] && [ $N -lt 160 ]; do N=$((N+1)); done
Xvfb ":$N" -screen 0 800x600x24 >/dev/null 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=":$N"
printf 'host=10.0.0.5\nport=25565\nname=Steve\nserver=10.0.0.5:25565\nserver=192.168.1.20:25599\nserver=172.16.0.9:30000\n' > "$W/cfg"
MCPLAY_CONFIG="$W/cfg" MCPLAY_LOG=- timeout 60 "$W/mcplay" > "$W/run.log" 2>&1 & MP_PID=$!
for i in $(seq 1 20); do xdotool search --name mcplay >/dev/null 2>&1 && break; sleep 0.5; done
xdotool search --name mcplay >/dev/null 2>&1 || fail "start screen window did not open"
sleep 1
xdotool mousemove 60 172; sleep 0.3; xdotool click 1; sleep 0.5
xdotool mousemove 494 368; sleep 0.3; xdotool click 1; sleep 0.5
xdotool mousemove 60 150; sleep 0.2; xdotool click 1; sleep 0.1; xdotool click 1
for i in $(seq 1 20); do grep -q '^server=172.16.0.9:30000' "$W/cfg" && grep -q '^host=10.0.0.5' "$W/cfg" && [ "$(stat -c %Y "$W/cfg")" -ge "$(date +%s -d '-30 seconds')" ] && break; sleep 0.5; done
grep -q '^host=10.0.0.5$' "$W/cfg" || fail "host not kept: $(cat "$W/cfg")"
grep -q '^server=10.0.0.5:25565$' "$W/cfg" || fail "joined server missing from list"
grep -q '^server=172.16.0.9:30000$' "$W/cfg" || fail "other server lost"
grep -q '192.168.1.20' "$W/cfg" && fail "removed server still in list"
echo "  ok    click select, remove, double-click join"
echo "RESULT mc-play-startmouse: ok"
