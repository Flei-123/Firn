#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-profile-gl: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=release-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=1.20.1; VIEW=${PROF_VIEW:-8}; N=${PROF_RUNS:-3}
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
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
export LP_NUM_THREADS=${LP_NUM_THREADS:-4}
ERR=0
for i in $(seq 1 "$N"); do
    MCPLAY_GL=1 MCPLAY_WORKERS=3 MCPLAY_VSYNC=0 MCPLAY_FPS=0 FIRN_AUDIO=null MCPLAY_RUN_S=${PROF_RUN_S:-25} MCPLAY_VIEW=$VIEW MCPLAY_W=640 MCPLAY_H=360 \
        MCPLAY_PROFILE=1 MCPLAY_PROFILE_FROM=${PROF_FROM:-14} MCPLAY_SPIN=3 \
        timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" "Pr$i" > "$W/p$i.out" 2>&1
    rc=$?
    [ -n "${PROF_OUT:-}" ] && cp "$W/p$i.out" "$PROF_OUT/p$i.out" 2>/dev/null
    [ $rc -eq 0 ] || { echo "  FAIL  run $i exit code $rc"; ERR=1; continue; }
    echo "  run $i load $(cut -d' ' -f1 /proc/loadavg)"
    grep "mcplay: profile" "$W/p$i.out" | tail -40
done
[ $ERR -eq 0 ] && echo "RESULT mc-profile-gl: ok" || echo "RESULT mc-profile-gl: FAIL"
exit $ERR
