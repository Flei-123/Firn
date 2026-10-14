#!/usr/bin/env bash
# tools/mc/play/run.sh -- demos/mcplay end to end: Xvfb + official 1.20.1 server (creative, offline mode) + mcplay + xdotool.
# Flies/walks ~8 s (W + mouse look), clicks (break + place), quits by MCPLAY_RUN_S, writes a screenshot PNG + hash.
# Needs Xvfb, xdotool, java, network for the server jar (tools/mcclient/fetch_server.sh). Prints `RESULT mc-play: ok`.
# Env: MCPLAY_PNG_OUT (copy target of the screenshot), MC_VIEW (4), FIRNC. Start it via heavy in a full run (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=1.20.1; VIEW=${MC_VIEW:-4}
mkdir -p "$W/srv"
bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=creative
force-gamemode=true
level-seed=4242
view-distance=$VIEW
simulation-distance=$VIEW
spawn-protection=0
difficulty=peaceful
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
enable-rcon=false
enable-query=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "${JAVA:-/usr/bin/java}" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
MCPLAY_RUN_S=12 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/shot.png" \
    "$W/mcplay" 127.0.0.1 "$PORT" Player > "$W/play.out" 2>&1 &
PP=$!
sleep 4
xdotool mousemove 320 180 2>/dev/null
xdotool mousemove_relative -- 40 5; sleep 0.3
xdotool keydown w; sleep 3; xdotool keyup w
xdotool key space; sleep 0.3
xdotool click 1; sleep 0.5
xdotool click 3; sleep 0.5
xdotool key 3; sleep 0.3
xdotool click 4; sleep 0.3
xdotool mousemove_relative -- 100 0; sleep 1
wait $PP; rc=$?
echo stop >&3; sleep 1
sed 's/^/  /' "$W/play.out" | tail -8
ERR=0
[ $rc -eq 0 ] || { echo "  FAIL  mcplay exit code $rc"; ERR=1; }
grep -q "joined the game" "$W/server.log" || { echo "  FAIL  server never saw the join"; ERR=1; }
grep -qE "moved wrongly|moved too quickly" "$W/server.log" && { echo "  FAIL  server rejected our movement"; ERR=1; }
FR=$(grep -o 'frames=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); CH=$(grep -o 'chunks=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); TK=$(grep -o 'ticks=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${FR:-0}" -gt 20 ] || { echo "  FAIL  too few frames (${FR:-0})"; ERR=1; }
[ "${CH:-0}" -gt 10 ] || { echo "  FAIL  too few chunks (${CH:-0})"; ERR=1; }
[ "${TK:-0}" -gt 20 ] || { echo "  FAIL  too few ticks (${TK:-0})"; ERR=1; }
[ -s "$W/shot.png" ] || { echo "  FAIL  no screenshot"; ERR=1; }
# light + time of day: update_time packets were applied, the 5x5 light region loaded, noon = daylight 256
grep -q 'light=true region=true' "$W/play.out" || { echo "  FAIL  light region not loaded"; ERR=1; }
TP=$(grep -o 'time_packets=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); DL=$(grep -o 'daylight=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${TP:-0}" -gt 0 ] || { echo "  FAIL  no update_time packet applied (${TP:-0})"; ERR=1; }
[ "${DL:-0}" -ge 200 ] || { echo "  FAIL  noon daylight too low (${DL:-0})"; ERR=1; }
# second short run at midnight: server time drives the sky (daylight 0) and the picture differs from noon
echo "time set 18000" >&3; sleep 1
MCPLAY_RUN_S=6 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/night.png" \
    "$W/mcplay" 127.0.0.1 "$PORT" Player2 > "$W/night.out" 2>&1; nrc=$?
[ $nrc -eq 0 ] || { echo "  FAIL  night run exit code $nrc"; ERR=1; }
ND=$(grep -o 'daylight=[0-9]*' "$W/night.out" | head -1 | cut -d= -f2)
[ "${ND:-256}" -le 20 ] || { echo "  FAIL  midnight daylight too high (${ND:-?})"; ERR=1; }
NH=$(grep -o 'hash=[0-9a-f]*' "$W/night.out" | head -1); DH=$(grep -o 'hash=[0-9a-f]*' "$W/play.out" | head -1)
[ -n "$NH" ] && [ "$NH" != "$DH" ] || { echo "  FAIL  night picture equals day picture"; ERR=1; }
echo "  day daylight=${DL:-?} night daylight=${ND:-?} night $NH"
grep -o 'hash=[0-9a-f]*' "$W/play.out" | sed 's/^/  /'
if [ -n "${MCPLAY_PNG_OUT:-}" ] && [ -s "$W/shot.png" ]; then cp "$W/shot.png" "$MCPLAY_PNG_OUT"; echo "  PNG: $MCPLAY_PNG_OUT"; fi
[ $ERR -eq 0 ] && echo "RESULT mc-play: ok" || echo "RESULT mc-play: FAIL"
exit $ERR
