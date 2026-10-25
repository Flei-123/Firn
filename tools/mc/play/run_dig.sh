#!/usr/bin/env bash
# tools/mc/play/run_dig.sh -- block breaking checked on the SERVER: mcplay digs a stone block under the player, the vanilla 1.20.1 server must
# report the block gone (execute if block) and, in survival, the inventory must hold +1 cobblestone. Modes: survival (iron pickaxe, held button)
# and creative (single click). Prints `RESULT mc-play-dig: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""; PP=""
cleanup() { exec 3>&- 2>/dev/null; [ "${DIG_WINE:-0}" = 1 ] && WINEPREFIX="$W/wp" wineserver -k 2>/dev/null; [ -n "${MCPLAY_KEEP_DIR:-}" ] && { mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; }; [ -n "$PP" ] && kill "$PP" 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-dig: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
RUN="$W/mcplay"
if [ "${DIG_WINE:-0}" = 1 ]; then
    command -v wine >/dev/null || fail "missing tool: wine"
    $FIRNC --target=x86_64-windows --opt-level=dev-fast -o "$W/mcplay.exe" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not cross-compile"; }
    RUN="wine $W/mcplay.exe"
    export MCPLAY_GL=0
else
    $FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
fi
MCV=${MC_VERSION:-1.20.1}; VIEW=${MC_VIEW:-4}
mkdir -p "$W/srv"
bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=survival
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
if [ "${DIG_WINE:-0}" = 1 ]; then export WINEPREFIX="$W/wp" WINEDEBUG=-all WINEARCH=win64; wine wineboot -i >/dev/null 2>&1; fi
ERR=0
wj() { local i; for i in $(seq 1 1800); do grep -q "$1 joined the game" "$W/server.log" 2>/dev/null && return 0; sleep 0.1; done; echo "  WARN  $1 did not join"; return 1; }
prog() { grep -o "progress .*" "$W/$1.out" 2>/dev/null | tail -1 | grep -o " $2=[^ ]*" | cut -d= -f2; }
wait_ticks() { local t0 t; t0=$(prog "$1" ticks); t0=${t0:-0}; for i in $(seq 1 $(( ${3:-180} * 10 ))); do t=$(prog "$1" ticks); [ "${t:-0}" -ge $(( t0 + $2 )) ] && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 0.1; done; echo "  WARN  wait_ticks $2 timed out"; return 1; }
server_ask() { local mark="MARK$RANDOM$RANDOM"; echo "say $mark" >&3; for i in $(seq 1 100); do grep -q "$mark" "$W/server.log" && break; sleep 0.1; done; }
run_mode() {
    local mode=$1 nm=$2 gm=$3 x=$4
    MCPLAY_PROGRESS=1 MCPLAY_AUDIO=0 MCPLAY_CREATIVE=$([ "$mode" = creative ] && echo 1 || echo 0) MCPLAY_RUN_S=${MCPLAY_DIG_RUN_S:-150} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
        MCPLAY_QUIT_FILE="$W/quit-$nm" timeout 220 $RUN 127.0.0.1 "$PORT" "$nm" > "$W/$nm.out" 2>&1 &
    PP=$!
    wj "$nm" || { ERR=1; return; }
    for i in $(seq 1 1200); do [ "$(prog "$nm" pos)" = 1 ] || [ "$(prog "$nm" pos)" = true ] && [ "$(prog "$nm" chunks)" -gt 10 ] 2>/dev/null && break; sleep 0.1; done
    echo "gamemode $gm $nm" >&3
    for cmd in "forceload add $x 0" "fill $((x-3)) 119 -3 $((x+3)) 120 3 stone" "fill $((x-3)) 121 -3 $((x+3)) 128 3 air"; do echo "$cmd" >&3; done
    sleep 2
    echo "give $nm iron_pickaxe 1" >&3
    echo "tp $nm $x.5 121 0.5" >&3
    for i in $(seq 1 600); do [ "$(prog "$nm" ground)" != 0 ] && [ "$(prog "$nm" ground)" != false ] && [ "$(prog "$nm" y_mm)" = 121000 ] && break; sleep 0.1; done
    wait_ticks "$nm" 10
    xdotool mousemove 320 180 2>/dev/null
    xdotool mousemove_relative -- 0 350; xdotool mousemove_relative -- 0 350; wait_ticks "$nm" 10
    xdotool key 1; wait_ticks "$nm" 6
    server_ask
    local before; before=$(wc -l < "$W/server.log")
    if [ "$mode" = creative ]; then xdotool click 1; else xdotool mousedown 1; wait_ticks "$nm" 90; xdotool mouseup 1; fi
    wait_ticks "$nm" 20
    server_ask
    local dx dz
    for dx in -3 -2 -1 0 1 2 3; do for dz in -3 -2 -1 0 1 2 3; do echo "execute unless block $((x+dx)) 120 $dz minecraft:stone run say AIR_${dx}_${dz}" >&3; done; done
    echo "clear $nm minecraft:cobblestone 0" >&3
    server_ask
    sleep 1
    tail -n +"$before" "$W/server.log" > "$W/$nm.srv"
    local gone=$(grep -c "AIR_" "$W/$nm.srv") cobble
    cobble=$(grep -o "Found [0-9]* matching" "$W/$nm.srv" | grep -o "[0-9]*" | head -1)
    echo "  $mode: stone cells removed on the server = $gone ($(grep -o "AIR_[-0-9_]*" "$W/$nm.srv" | tr "\n" " ")), cobblestone in inventory = ${cobble:-0}"
    grep -E "dig=" "$W/$nm.out" | tail -1 | cut -c1-160 | sed "s/^/  $mode client: /"
    [ "$gone" -ge 1 ] || { echo "  FAIL  $mode: no block removed on the server"; ERR=1; }
    if [ "$mode" = survival ]; then [ "${cobble:-0}" -ge 1 ] || { echo "  FAIL  survival: no cobblestone in the inventory"; ERR=1; }; fi
    touch "$W/quit-$nm"; wait $PP 2>/dev/null; PP=""
}
run_mode survival DigSurv survival 0
run_mode creative DigCrea creative 40
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-dig: ok" || echo "RESULT mc-play-dig: FAIL"
exit $ERR
