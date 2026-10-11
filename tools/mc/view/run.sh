#!/usr/bin/env bash
# tools/mc/view/run.sh [--offline|--live] -- demos/mcview against the independent python renderer (ref_render.py).
#   offline: replays the frozen world dump tools/mc/view/fixture/world.mcw WITHOUT a server (this part runs in tools/mc/run.sh)
#   live:    starts an official 1.20.1 server (offline mode, fixed seed, frozen: peaceful, no day cycle/weather/mob spawning,
#            time day), joins with demos/mcview, renders, compares, copies the PNG to the reports dir. Start it via heavy:
#            /root/jarvis/bin/heavy bash tools/mc/view/run.sh --live
# Offline runs the plain image plus flag variants (--all, sky/HUD at sunset, night + rain + box, fog off), each compared exactly.
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

# offline variant: <label> <env assignments> <flags> [yaw pitch]; the HUD frame rate is fixed (60) so the hash is reproducible
off_variant() {
    local lab=$1 envs=$2 flags=$3; shift 3
    mkdir -p "$W/off_$lab"
    # shellcheck disable=SC2086
    if env MCVIEW_FPS=60 $envs "$W/mcview" $flags --replay "$W/off_$lab" tools/mc/view/fixture/world.mcw "$@" > "$W/off_$lab.out" 2>&1; then
        grep -E "metric (frame_cold_ms|frame_cached_ms|triangles|sections_meshed|chunks_per_s|hash)" "$W/off_$lab.out" | sed "s/^/  [$lab] /"
        compare "$W/off_$lab" "offline fixture ($lab: ${flags:-no flags})" tools/mc/view/fixture/world.mcw
        # the cached second frame inside mcview equals the first one (checked in the program); the image must not be empty
        [ -s "$W/off_$lab/view.png" ] || { echo "  FAIL  no PNG ($lab)"; ERR=1; }
    else
        echo "  FAIL  mcview --replay ($lab)"; head -5 "$W/off_$lab.out"; ERR=1
    fi
}

if [ "$MODE" = both ] || [ "$MODE" = --offline ]; then
    off_variant plain "" ""                                   # no flags: exactly the plain world render
    off_variant all "MCVIEW_TICKS=2000" "--all"               # sky + fog colour + tint + selection box + HUD
    off_variant sunset "MCVIEW_TICKS=12000" "--sky --hud" 90 5   # sunset glow + sun disc, no pick hit
    off_variant night "MCVIEW_TICKS=18000 MCVIEW_RAIN=200" "--sky --box" 0 10   # night, rain, box only
    off_variant nofog "MCVIEW_FOG=0 MCVIEW_TICKS=500" "--all" 300 -10   # fog off with the sky
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
    # two joins: the plain image (compared exactly) and the full image (sky + selection box + HUD, also compared exactly)
    mkdir -p "$W/live_all"
    "$W/mcview" 127.0.0.1 "$PORT" Viewer "$W/live" "$VIEW" "${MC_SETTLE:-4000}" "${MC_YAW:-0}" "${MC_PITCH:-20}" "$PROTO" > "$W/live.out" 2>&1; rc=$?
    env ${MCVIEW_ALL_ENV:-MCVIEW_TICKS=2000} "$W/mcview" --all 127.0.0.1 "$PORT" Viewer "$W/live_all" "$VIEW" "${MC_SETTLE:-4000}" "${MC_YAW:-0}" "${MC_PITCH:-20}" "$PROTO" > "$W/live_all.out" 2>&1; rc2=$?
    echo stop >&3
    cat "$W/live.out" | grep -E "^metric|mcview" | sed 's/^/  /'
    if [ $rc -ne 0 ]; then echo "  FAIL  mcview live (rc $rc)"; ERR=1; else
        compare "$W/live" "live server 1.20.1"
    fi
    grep -E "^metric (frame_final_us|chunk_unloads|chunk_bad)" "$W/live_all.out" | sed 's/^/  [all] /'
    if [ $rc2 -ne 0 ]; then echo "  FAIL  mcview --all live (rc $rc2)"; ERR=1; else
        compare "$W/live_all" "live server 1.20.1 (--all: sky, box, HUD)"
        if [ -n "${MCVIEW_PNG_OUT:-}" ]; then cp "$W/live_all/view.png" "$MCVIEW_PNG_OUT"; cp "$W/live_all/world.mcw" "${MCVIEW_PNG_OUT%.png}.mcw" 2>/dev/null; echo "  PNG: $MCVIEW_PNG_OUT"; fi
    fi
fi
[ $ERR -eq 0 ] && echo "RESULT mc-view: ok" || echo "RESULT mc-view: FAIL"
exit $ERR
