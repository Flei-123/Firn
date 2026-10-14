#!/usr/bin/env bash
# tools/mc/play/run_lod.sh -- mcplay with MCPLAY_LOD (mc_stream + mc_lod rings) against the official 1.20.1 server: Xvfb + server + mcplay + xdotool.
# Run A: MCPLAY_VIEW=3 MCPLAY_LOD=9 (3 chunks full detail, the rest LOD; the vanilla server sends its own view distance either way), walks 3 s, screenshot.
# Run B: MCPLAY_VIEW=3 (stream is the default, no LOD): control + worker pool + window sync (server radius > view + 2). Prints `RESULT mc-play-lod: ok`.
# Checks: exit 0, frames, A has LOD columns drawn (lod_cols > 0) and LOD triangles, B none, pictures differ.
# Env: MC_VIEW_SRV (10 = server view distance), FIRNC, MCPLAY_PNG_OUT (copy of the A screenshot). Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-lod: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=1.20.1; SV=${MC_VIEW_SRV:-10}
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
view-distance=$SV
simulation-distance=4
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
ERR=0
run_one() { # name, env...
    local name=$1; shift
    env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_RUN_S=26 MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/$name.png" "$@" \
        "$W/mcplay" 127.0.0.1 "$PORT" "P$name" > "$W/$name.out" 2>&1 &
    local pp=$!
    sleep 6
    echo "tp P$name 0.5 100 0.5" >&3
    sleep 4
    xdotool mousemove 320 180 2>/dev/null
    xdotool keydown w; sleep 3; xdotool keyup w
    wait $pp; local rc=$?
    [ $rc -eq 0 ] || { echo "  FAIL  $name exit code $rc"; ERR=1; }
    sed 's/^/  /' "$W/$name.out" | grep -E "stream|end frames" | tail -3
}
run_one A MCPLAY_VIEW=3 MCPLAY_LOD=9
run_one B MCPLAY_VIEW=3
num() { grep -o "$2=[0-9]*" "$W/$1.out" | tail -1 | cut -d= -f2; }
FA=$(num A frames); FB=$(num B frames); CA=$(num A chunks); CB=$(num B chunks)
LA=$(num A lod_cols); LB=$(num B lod_cols); TA=$(num A lod_tris); TB=$(num B lod_tris)
echo "  A frames=${FA:-?} chunks=${CA:-?} lod_cols=${LA:-?} lod_tris=${TA:-?}   B frames=${FB:-?} chunks=${CB:-?} lod_cols=${LB:-?}"
[ "${FA:-0}" -gt 20 ] && [ "${FB:-0}" -gt 20 ] || { echo "  FAIL  too few frames"; ERR=1; }
[ "${CA:-0}" -gt 100 ] || { echo "  FAIL  LOD run loaded only ${CA:-0} chunks"; ERR=1; }
[ "${LA:-0}" -gt 0 ] && [ "${TA:-0}" -gt 0 ] || { echo "  FAIL  no LOD columns drawn in run A"; ERR=1; }
[ "${LB:-0}" -eq 0 ] || { echo "  FAIL  LOD columns drawn without MCPLAY_LOD (${LB:-?})"; ERR=1; }
# worker pool + window logic (a2): B has view 3 but the server sends MC_VIEW_SRV chunks -> columns beyond view + 2 leave the stream (window_ops > 0), the pool is up
WB=$(num B window_ops); KB=$(grep -o " cols=[0-9]*" "$W/B.out" | tail -1 | cut -d= -f2)
grep -q 'stream workers=3 pool=true' "$W/B.out" || { echo "  FAIL  stream has no mesher pool (workers=3 expected)"; ERR=1; }
[ "${WB:-0}" -gt 0 ] || { echo "  FAIL  window sync did nothing (window_ops=${WB:-?}) although the server sends more than view+2"; ERR=1; }
[ "${KB:-9999}" -lt "${CB:-0}" ] || { echo "  FAIL  stream keeps all ${KB:-?} of ${CB:-?} columns"; ERR=1; }
echo "  B window_ops=${WB:-?} stream_cols=${KB:-?}"
[ -s "$W/A.png" ] && [ -s "$W/B.png" ] || { echo "  FAIL  no screenshot"; ERR=1; }
cmp -s "$W/A.png" "$W/B.png" && { echo "  FAIL  LOD picture equals the no-LOD picture"; ERR=1; }
[ -n "${MCPLAY_PNG_OUT:-}" ] && [ -s "$W/A.png" ] && cp "$W/A.png" "$MCPLAY_PNG_OUT"
[ $ERR -eq 0 ] && echo "RESULT mc-play-lod: ok" || echo "RESULT mc-play-lod: FAIL"
exit $ERR
