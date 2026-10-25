#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
MCV=1.20.1; PROTO=763
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}
OUT=${WATER_OUT:-}
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "$OUT" ]; then mkdir -p "$OUT"; cp "$W"/*.png "$W"/*.out "$W"/server.log "$OUT"/ 2>/dev/null; fi; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; sleep 1; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; [ -f "$W/server.log" ] && tail -3 "$W/server.log" | cut -c1-200; echo "RESULT mc-play-water: FAIL"; exit 1; }
for t in Xvfb xdotool; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
mkdir -p "$W/srv"
MCCLIENT_JAR_CACHE=${MCCLIENT_JAR_CACHE:-/root/.w5/jars} bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=creative
force-gamemode=true
level-type=minecraft\:flat
view-distance=4
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
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "spawnRadius 0"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; sleep 1; done; return 1; }
srv_cmd() { local nl; nl=$(wc -l < "$W/server.log"); echo "$1" >&3; wait_log "$2" "${3:-30}" "$nl"; }
srv_cmd "forceload add 0 0 15 15" "force loaded" || fail "forceload"
sleep 3
srv_cmd "fill 0 -61 0 12 -61 12 minecraft:sand" "Successfully filled" || fail "floor"
srv_cmd "fill 0 -60 0 12 -58 12 minecraft:air" "filled" || fail "clear"
srv_cmd "fill 0 -60 0 12 -59 12 minecraft:stone hollow" "Successfully filled" || fail "rim"
srv_cmd "fill 1 -60 1 11 -59 11 minecraft:water" "Successfully filled" || fail "water"
srv_cmd "fill 3 -63 3 6 -62 6 minecraft:water" "Successfully filled" || fail "deep"
srv_cmd "fill 3 -61 3 6 -61 6 minecraft:water" "Successfully filled" || fail "deep2"
srv_cmd "fill 8 -60 8 9 -59 9 minecraft:glass" "Successfully filled" || fail "glass"
srv_cmd "setblock 12 -59 5 minecraft:water[level=0]" "Changed the block|Could not" || true
srv_cmd "fill 12 -60 4 12 -60 6 minecraft:air" "filled" || true
srv_cmd "setblock 12 -60 5 minecraft:water" "Changed the block" || fail "source"
sleep 6
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
run_one() {
    local name=$1 pos=$2 rot=$3; shift 3
    local att pn pp ok nl
    for att in 1 2 3; do
        pn="W${name}${att}"; ok=1; rm -f "$W/$name.png"
        env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_CREATIVE=1 MCPLAY_RUN_S=${RUN_S:-30} MCPLAY_FPS=30 MCPLAY_W=${WATER_W:-640} MCPLAY_H=${WATER_H:-360} MCPLAY_VIEW=3 \
            MC_VANILLA=1 MC_VANILLA_JAR=${MC_VANILLA_JAR:-/root/.w5/jars/client-1.20.1.jar} MC_VANILLA_CACHE="$W/vcache" MC_PACKS= \
            MCPLAY_SHOT="$W/$name.png" "$@" "$W/mcplay" 127.0.0.1 "$PORT" "$pn" > "$W/$name.out" 2>&1 &
        pp=$!
        if wait_log "$pn joined the game" 120; then
            sleep 3
            xdotool mousemove 320 180 2>/dev/null
            xdotool key f
            nl=$(wc -l < "$W/server.log")
            echo "tp $pn $pos $rot" >&3
            wait_log "Teleported $pn to" 10 "$nl" || ok=0
        else
            ok=0
        fi
        wait $pp
        [ $ok -eq 1 ] && break
        echo "  retry $name (attempt $att)"
    done
    [ $ok -eq 1 ] || { echo "  FAIL  $name never positioned"; ERR=1; }
    echo "  $name load=$(cut -d' ' -f1 /proc/loadavg)"
}
SCENES=${WATER_SCENES:-top under up}
for s in $SCENES; do
    case $s in
        top) P="6.5 -55 -4.5"; R="0 38";;
        under) P="6.5 -60.0 9.5"; R="180 0";;
        up) P="5.0 -62.2 5.0"; R="0 -70";;
        edge) P="12.5 -58 1.5"; R="0 45";;
        *) continue;;
    esac
    for v in ${WATER_VARIANTS:-def}; do
        case $v in
            def) E="";;
            t0) E="MCPLAY_WATER_TINT=0";;
            t1) E="MCPLAY_WATER_TINT=1";;
            gl) E="MCPLAY_GL=1";;
            vk) E="MCPLAY_GFX=vk";;
        esac
        run_one "${s}_$v" "$P" "$R" $E
    done
done
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-water: ok" || echo "RESULT mc-play-water: FAIL"
exit $ERR
