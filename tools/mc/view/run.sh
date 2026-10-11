#!/usr/bin/env bash
# tools/mc/view/run.sh [--offline|--live] -- demos/mcview against the independent python renderer (ref_render.py).
#   offline: replays the frozen world dump tools/mc/view/fixture/world.mcw WITHOUT a server (this part runs in tools/mc/run.sh)
#   live:    starts an official 1.20.1 server (offline mode, fixed seed, frozen: peaceful, no day cycle/weather/mob spawning,
#            time day), joins with demos/mcview, renders, compares, copies the PNG to the reports dir. Start it via heavy:
#            /root/jarvis/bin/heavy bash tools/mc/view/run.sh --live
# Prints `RESULT mc-view: ok`. Env: MC_SEED (4242) MC_VIEW (7) MC_SETTLE (4000) MC_YAW (0) MC_PITCH (20) MCVIEW_PNG_OUT (copy target)
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
MODE=${1:-both}
W=$(mktemp -d)
SRV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
ERR=0
$FIRNC -o "$W/mcview" demos/mcview/main.fi 2>"$W/err" || { echo "  FAIL  demos/mcview does not compile"; head -10 "$W/err"; echo "RESULT mc-view: FAIL"; exit 1; }

compare() { # <dir> <label> [world file]
    python3 -I tools/mc/view/ref_render.py "$1" ${3:-} > "$W/cmp.txt" 2>&1; rc=$?
    sed 's/^/  /' "$W/cmp.txt"
    [ $rc -eq 0 ] && echo "  ok    $2: python renderer == Firn image" || { echo "  FAIL  $2 (code $rc)"; ERR=1; }
}

if [ "$MODE" = both ] || [ "$MODE" = --offline ]; then
    mkdir -p "$W/off"
    if "$W/mcview" --replay "$W/off" tools/mc/view/fixture/world.mcw > "$W/off.out" 2>&1; then
        grep -E "metric (frame_cold_ms|frame_cached_ms|triangles|sections_meshed|chunks_per_s|hash)" "$W/off.out" | sed 's/^/  /'
        compare "$W/off" "offline fixture" tools/mc/view/fixture/world.mcw
        # the cached second frame inside mcview equals the first one (checked in the program); the image must not be empty
        [ -s "$W/off/view.png" ] || { echo "  FAIL  no PNG"; ERR=1; }
    else
        echo "  FAIL  mcview --replay"; head -5 "$W/off.out"; ERR=1
    fi
fi

if [ "$MODE" = both ] || [ "$MODE" = --live ]; then
    MCV=1.20.1; PROTO=763; SEED=${MC_SEED:-4242}; VIEW=${MC_VIEW:-7}
    mkdir -p "$W/srv" "$W/live"
    bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || { echo "  FAIL  server download"; echo "RESULT mc-view: FAIL"; exit 1; }
    echo "eula=true" > "$W/srv/eula.txt"
    PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
    cat > "$W/srv/server.properties" <<EOF
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
level-seed=$SEED
view-distance=$VIEW
simulation-distance=$VIEW
spawn-protection=0
difficulty=peaceful
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
network-compression-threshold=256
enable-rcon=false
enable-query=false
motd=mcview test server
EOF
    mkfifo "$W/in"; exec 3<>"$W/in"
    ( cd "$W/srv" && exec "${JAVA:-/usr/bin/java}" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
    SRV_PID=$!
    for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
    grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; echo "  FAIL  server did not start"; echo "RESULT mc-view: FAIL"; exit 1; }
    for gr in "randomTickSpeed 0" "doFireTick false" "doMobSpawning false" "doWeatherCycle false" "doDaylightCycle false"; do echo "gamerule $gr" >&3; done
    echo "time set day" >&3; echo "weather clear" >&3
    sleep 4
    "$W/mcview" 127.0.0.1 "$PORT" Viewer "$W/live" "$VIEW" "${MC_SETTLE:-4000}" "${MC_YAW:-0}" "${MC_PITCH:-20}" "$PROTO" > "$W/live.out" 2>&1; rc=$?
    echo stop >&3
    cat "$W/live.out" | grep -E "^metric|mcview" | sed 's/^/  /'
    if [ $rc -ne 0 ]; then echo "  FAIL  mcview live (rc $rc)"; ERR=1; else
        compare "$W/live" "live server 1.20.1"
        if [ -n "${MCVIEW_PNG_OUT:-}" ]; then cp "$W/live/view.png" "$MCVIEW_PNG_OUT"; cp "$W/live/world.mcw" "${MCVIEW_PNG_OUT%.png}.mcw" 2>/dev/null; echo "  PNG: $MCVIEW_PNG_OUT"; fi
    fi
fi
[ $ERR -eq 0 ] && echo "RESULT mc-view: ok" || echo "RESULT mc-view: FAIL"
exit $ERR
