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
# sound bank (cached: tools/sounds/gen is deterministic); FIRN_AUDIO=file writes the mix to a WAV
SND=${MCPLAY_SOUNDS:-${XDG_CACHE_HOME:-$HOME/.cache}/mcplay/sounds}
if [ ! -s "$SND/bank.json" ]; then
    $FIRNC -o "$W/sndgen" tools/sounds/gen/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "tools/sounds/gen does not compile"; }
    mkdir -p "$SND"; "$W/sndgen" tools/sounds/mc.json "$SND" >/dev/null 2>&1 || fail "sound bank generation"
fi
export MCPLAY_SOUNDS="$SND"
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
FIRN_AUDIO="file:$W/day.wav" MCPLAY_RUN_S=12 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/shot.png" \
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
# sound: bank loaded, walk/break/place sounds mixed into a non-empty WAV (the server cannot be heard; the file is the proof)
AB=$(grep -o 'audio bytes=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); AP=$(grep -o ' played=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
AS=$(grep -o 'sounds=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); AC=$(grep -o 'crc=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${AB:-0}" -gt 100000 ] || { echo "  FAIL  audio output too small (${AB:-0} bytes)"; ERR=1; }
[ "${AS:-0}" -ge 2 ] || { echo "  FAIL  too few sounds triggered (${AS:-0})"; ERR=1; }
[ "${AP:-0}" -ge 1 ] || { echo "  FAIL  no voice played (${AP:-0})"; ERR=1; }
if [ -s "$W/day.wav" ]; then
    python3 - "$W/day.wav" <<'PY' || ERR=1
import sys, wave, array
w = wave.open(sys.argv[1]); n = w.getnframes(); a = array.array('h'); a.frombytes(w.readframes(n))
peak = max((abs(x) for x in a), default=0)
print(f"  wav frames={n} rate={w.getframerate()} ch={w.getnchannels()} peak={peak}")
sys.exit(0 if n > 10000 and peak > 100 else 1)
PY
else echo "  FAIL  no day.wav"; ERR=1; fi
echo "  audio bytes=${AB:-?} crc=${AC:-?} sounds=${AS:-?} played=${AP:-?}"
# second short run at midnight: server time drives the sky (daylight 0) and the picture differs from noon
echo "time set 18000" >&3; sleep 1
MCPLAY_RUN_S=6 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/night.png" \
    "$W/mcplay" 127.0.0.1 "$PORT" Player2 > "$W/night.out" 2>&1; nrc=$?
[ $nrc -eq 0 ] || { echo "  FAIL  night run exit code $nrc"; ERR=1; }
ND=$(grep -o 'daylight=[0-9]*' "$W/night.out" | head -1 | cut -d= -f2)
[ "${ND:-256}" -le 20 ] || { echo "  FAIL  midnight daylight too high (${ND:-?})"; ERR=1; }
NH=$(grep -o 'hash=[0-9a-f]*' "$W/night.out" | head -1); DH=$(grep -o 'hash=[0-9a-f]*' "$W/play.out" | head -1)
[ -n "$NH" ] && [ "$NH" != "$DH" ] || { echo "  FAIL  night picture equals day picture"; ERR=1; }
# GL path (MCPLAY_GL=1): glwindow (EGL, Mesa llvmpipe = software, NO real GPU) + gl_mc/gl_scene, frame shown without read back
if [ "${MCPLAY_SKIP_GL:-0}" != 1 ]; then
echo "time set day" >&3; sleep 1
MCPLAY_GL=1 MCPLAY_VSYNC=0 FIRN_AUDIO=null MCPLAY_RUN_S=8 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/gl.png" \
    timeout 120 "$W/mcplay" 127.0.0.1 "$PORT" Player3 > "$W/gl.out" 2>&1; grc=$?
sed 's/^/  gl: /' "$W/gl.out" | tail -7
[ $grc -eq 0 ] || { echo "  FAIL  GL run exit code $grc"; ERR=1; }
grep -q 'mcplay: gl renderer=' "$W/gl.out" || { echo "  FAIL  GL path did not start"; ERR=1; }
GF=$(grep -o 'frames=[0-9]*' "$W/gl.out" | head -1 | cut -d= -f2); GC=$(grep -o 'chunks=[0-9]*' "$W/gl.out" | head -1 | cut -d= -f2)
[ "${GF:-0}" -gt 5 ] || { echo "  FAIL  GL: too few frames (${GF:-0})"; ERR=1; }
[ "${GC:-0}" -gt 10 ] || { echo "  FAIL  GL: too few chunks (${GC:-0})"; ERR=1; }
[ "$(stat -c %s "$W/gl.png" 2>/dev/null || echo 0)" -gt 1500 ] || { echo "  FAIL  GL: no screenshot"; ERR=1; }
GH=$(grep -o 'hash=[0-9a-f]*' "$W/gl.out" | head -1)
echo "  gl frames=${GF:-?} chunks=${GC:-?} $GH"
if [ -n "${MCPLAY_GL_PNG_OUT:-}" ] && [ -s "$W/gl.png" ]; then cp "$W/gl.png" "$MCPLAY_GL_PNG_OUT"; fi
fi
echo stop >&3; sleep 1
echo "  day daylight=${DL:-?} night daylight=${ND:-?} night $NH"
grep -o 'hash=[0-9a-f]*' "$W/play.out" | sed 's/^/  /'
if [ -n "${MCPLAY_PNG_OUT:-}" ] && [ -s "$W/shot.png" ]; then cp "$W/shot.png" "$MCPLAY_PNG_OUT"; echo "  PNG: $MCPLAY_PNG_OUT"; fi
[ $ERR -eq 0 ] && echo "RESULT mc-play: ok" || echo "RESULT mc-play: FAIL"
exit $ERR
