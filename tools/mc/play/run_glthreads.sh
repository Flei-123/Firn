#!/usr/bin/env bash
# tools/mc/play/run_glthreads.sh -- r167: mcplay GL path (MCPLAY_GL=1, Mesa llvmpipe, NO real GPU) with Firn mesher threads (MCPLAY_WORKERS=3),
# repeated GL_RUNS times (default 5) against the official 1.20.1 server (Xvfb + server + mcplay + xdotool), plus one inline run (WORKERS=0) for timing.
# Before the fix (thread.rs use_gs: thread block behind gs for glibc-linked programs) every threaded GL run segfaulted. Prints `RESULT mc-play-glthreads: ok`.
# Env: GL_RUNS, MC_VIEW (4), FIRNC. Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-glthreads: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=1.20.1; VIEW=${MC_VIEW:-4}; N=${GL_RUNS:-5}
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
# llvmpipe spins one rasterizer thread per core (32 here): on a loaded box that starves the frame -> small fixed pool
export LP_NUM_THREADS=${LP_NUM_THREADS:-4}
ERR=0
num() { grep -o "$2=[0-9]*" "$W/$1.out" | tail -1 | cut -d= -f2; }
run_one() { # name, workers
    local name=$1 wk=$2
    MCPLAY_GL=1 MCPLAY_WORKERS=$wk MCPLAY_VSYNC=0 FIRN_AUDIO=null MCPLAY_RUN_S=${GL_RUN_S:-8} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=${GL_W:-640} MCPLAY_H=${GL_H:-360} MCPLAY_SHOT="$W/$name.png" \
        timeout "${GL_TIMEOUT:-120}" "$W/mcplay" 127.0.0.1 "$PORT" "P$name" > "$W/$name.out" 2>&1 &
    local pp=$!
    wait $pp; local rc=$?; [ -n "${GL_OUT_DIR:-}" ] && cp "$W/$name.out" "$GL_OUT_DIR/$name.out" 2>/dev/null
    [ $rc -eq 0 ] || { echo "  FAIL  $name (workers=$wk) exit code $rc"; ERR=1; return; }
    grep -q 'mcplay: gl renderer=' "$W/$name.out" || { echo "  FAIL  $name: GL path did not start"; ERR=1; }
    if [ "$wk" -gt 0 ]; then grep -q "stream workers=$wk pool=true" "$W/$name.out" || { echo "  FAIL  $name: no mesher pool (workers=$wk)"; ERR=1; }; fi
    local f c; f=$(num $name frames); c=$(num $name chunks)
    [ "${f:-0}" -gt 5 ] && [ "${c:-0}" -gt 10 ] || { echo "  FAIL  $name: frames=${f:-0} chunks=${c:-0}"; ERR=1; }
    [ "$(stat -c %s "$W/$name.png" 2>/dev/null || echo 0)" -gt 1500 ] || { echo "  FAIL  $name: no screenshot"; ERR=1; }
    echo "  $name workers=$wk frames=${f:-?} chunks=${c:-?} $(grep 'world_draw' "$W/$name.out" | tail -1 | grep -o 'avg_us=[0-9]* max_us=[0-9]*') $(grep -o 'stream workers=.*' "$W/$name.out" | tail -1 | cut -c1-80)"
}
[ "${GL_SKIP_INLINE:-0}" = 1 ] || run_one inline 0
for i in $(seq 1 "$N"); do run_one t$i 3; done
[ $ERR -eq 0 ] && echo "RESULT mc-play-glthreads: ok" || echo "RESULT mc-play-glthreads: FAIL"
exit $ERR
