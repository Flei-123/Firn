#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${SNOW_KEEP_DIR:-}" ] && { mkdir -p "$SNOW_KEEP_DIR"; cp "$W"/*.out "$W/server.log" "$SNOW_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-snow: FAIL"; exit 1; }
for t in Xvfb java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=${1:-1.20.1}
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
level-type=minecraft\:flat
view-distance=6
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
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "randomTickSpeed 0"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
PN=PSnow1
env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_SNOW_LOG=1 MCPLAY_WEATHER_LOG=1 MCPLAY_BIOME_LOG=1 MCPLAY_RUN_S=${SNOW_RUN_S:-120} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=4 \
    timeout 200 "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/run.out" 2>&1 &
PP=$!
wait_for() {
    local i
    for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/run.out" 2>/dev/null | grep -Eq "$1" && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 1; done
    return 1
}
last() { grep -E "mcplay: snowlog" "$W/run.out" | tail -1; }
field() { echo "$2" | grep -o "$1=-\?[0-9]*" | head -1 | cut -d= -f2; }
for i in $(seq 1 90); do grep -q "$PN joined the game" "$W/server.log" 2>/dev/null && break; sleep 1; done
sleep 4
echo "weather rain 6000" >&3
wait_for 'snowlog kind=1 rain=1000 ' 40 || { echo "  FAIL  plains rain never reached kind=1 rain=1000"; ERR=1; }
sleep 4
P1=$(last)
echo "  plains rain: $P1"
[ "$(field spawned "$P1")" = 0 ] || { echo "  FAIL  snowflakes spawned in plains rain"; ERR=1; }
echo "execute at $PN run fillbiome ~-14 ~-3 ~-14 ~14 ~3 ~14 minecraft:snowy_plains" >&3
wait_for 'mcplay: biomes updated chunks=[1-9]' 30 || { echo "  FAIL  fillbiome did not reach the client"; ERR=1; }
wait_for 'snowlog kind=2 rain=1000 live=[1-9]' 30 || { echo "  FAIL  no live snowflakes after switching to snowy_plains"; ERR=1; }
sleep 6
P2=$(last)
echo "  snowy plains: $P2"
[ "$(field kind "$P2")" = 2 ] || { echo "  FAIL  precip kind is not SNOW in snowy_plains"; ERR=1; }
[ "$(field spawned "$P2")" -ge 100 ] 2>/dev/null || { echo "  FAIL  fewer than 100 flakes spawned in 6 s of snow"; ERR=1; }
LV=$(field live "$P2")
{ [ "${LV:-0}" -ge 20 ] && [ "${LV:-999}" -le 256 ]; } || { echo "  FAIL  live flakes ${LV:-?} outside 20..256"; ERR=1; }
CL=$(wc -l < "$W/run.out")
echo "weather clear" >&3
wait_for 'snowlog kind=[0-9] rain=0 ' 60 "$CL" || { echo "  FAIL  rain level did not fall to 0"; ERR=1; }
wait_for 'snowlog kind=[0-9] rain=0 live=0 ' 30 "$CL" || { echo "  FAIL  flakes did not expire after weather clear"; ERR=1; }
S3=$(field spawned "$(last)")
sleep 4
S4=$(field spawned "$(last)")
[ "${S3:-1}" = "${S4:-2}" ] || { echo "  FAIL  flakes still spawn after weather clear ($S3 -> $S4)"; ERR=1; }
wait $PP; RC=$?
[ $RC -eq 0 ] || { echo "  FAIL  exit code $RC"; ERR=1; }
[ $ERR -eq 0 ] && echo "RESULT mc-play-snow: ok" || echo "RESULT mc-play-snow: FAIL"
exit $ERR
