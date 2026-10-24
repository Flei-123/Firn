#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
MCV=${1:-1.20.1}; VIEW=${MC_VIEW:-4}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
[ "$PROTO" != 0 ] || { echo "unknown version $MCV (set MCPLAY_PROTOCOL)"; exit 2; }
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
W=$(mktemp -d); SRV_PID=""; XV_PID=""; PP=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$PP" ] && kill "$PP" 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; sleep 1; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-fluid: FAIL"; exit 1; }
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
gamemode=survival
force-gamemode=true
level-type=minecraft\:flat
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
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "drowningDamage false" "fallDamage false" "fireDamage false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((120 + RANDOM % 70))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
PN=FluidA
ERR=0
MCPLAY_AUDIO=0 FIRN_AUDIO=null MCPLAY_CREATIVE=0 MCPLAY_POS_LOG=1 MCPLAY_RUN_S=${FLUID_RUN_S:-260} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/a.out" 2>&1 &
PP=$!
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 1; done; return 1; }
srv_cmd() { local nl; nl=$(wc -l < "$W/server.log"); echo "$1" >&3; wait_log "$2" "${3:-15}" "$nl"; }
cur_tick() { grep -o 'poslog tick=[0-9]*' "$W/a.out" | tail -1 | cut -d= -f2; }
wait_ticks() { local t0 t i; t0=$(cur_tick); t0=${t0:-0}; for i in $(seq 1 120); do t=$(cur_tick); [ "${t:-0}" -ge $((t0 + $1)) ] && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 1; done; return 1; }
wait_log "$PN joined the game" 90 || fail "no join"
sleep 4
xdotool mousemove 320 180 2>/dev/null
srv_cmd "effect give $PN minecraft:fire_resistance 1000000 0 true" "Applied effect" || { echo "  FAIL  fire resistance not applied"; ERR=1; }
srv_cmd "fill 30 -60 5 38 -52 11 minecraft:stone" "Successfully filled" || fail "lava walls"
srv_cmd "fill 31 -59 6 35 -52 10 minecraft:lava" "Successfully filled" || fail "lava"
srv_cmd "fill 63 -60 7 65 -52 9 minecraft:stone" "Successfully filled" || fail "shaft walls"
srv_cmd "fill 64 -60 8 64 -52 8 minecraft:air" "Successfully filled" || fail "shaft air"
srv_cmd "setblock 64 -53 8 minecraft:water" "Changed the block" || fail "shaft source"
srv_cmd "fill -2 -60 41 2 -48 41 minecraft:stone" "Successfully filled" || fail "vine wall"
srv_cmd "fill 0 -60 40 0 -49 40 minecraft:vine[south=true]" "Successfully filled" || fail "vines"
srv_cmd "fill 8 -60 41 12 -50 41 minecraft:stone" "Successfully filled" || fail "scaffold wall"
srv_cmd "fill 10 -60 40 10 -55 40 minecraft:scaffolding[distance=0]" "Successfully filled" || fail "scaffolding"
srv_cmd "setblock 50 -60 20 minecraft:water" "Changed the block" || fail "flow source"
sleep 8
srv_cmd "tp $PN 33.5 -53 8.5 0 0" "Teleported $PN to" || { echo "  FAIL  lava tp not confirmed"; ERR=1; }
wait_ticks 20 || { echo "  FAIL  no poslog ticks"; ERR=1; }
LS0=$(cur_tick); wait_ticks 80; LS1=$(cur_tick)
srv_cmd "data get entity $PN Pos" "$PN has the following entity data" && grep "$PN has the following entity data" "$W/server.log" | tail -1 > "$W/lsink.pos"
xdotool keydown space
LH0=$(cur_tick); wait_ticks 40; LH1=$(cur_tick)
xdotool keyup space
srv_cmd "tp $PN 34.5 -51.5 8.5 -90 0" "Teleported $PN to" || { echo "  FAIL  lava exit tp not confirmed"; ERR=1; }
wait_ticks 3
xdotool keydown w; xdotool keydown space
LX0=$(cur_tick); wait_ticks 80; LX1=$(cur_tick)
xdotool keyup w; xdotool keyup space
wait_ticks 6
srv_cmd "data get entity $PN Pos" "$PN has the following entity data" && grep "$PN has the following entity data" "$W/server.log" | tail -1 > "$W/lexit.pos"
srv_cmd "tp $PN 53.5 -60 20.5 0 0" "Teleported $PN to" || { echo "  FAIL  flow tp not confirmed"; ERR=1; }
wait_ticks 4
FL0=$(cur_tick); wait_ticks 30; FL1=$(cur_tick)
srv_cmd "data get entity $PN Pos" "$PN has the following entity data" && grep "$PN has the following entity data" "$W/server.log" | tail -1 > "$W/flow.pos"
srv_cmd "tp $PN 64.5 -55.5 8.5 0 0" "Teleported $PN to" || { echo "  FAIL  shaft tp not confirmed"; ERR=1; }
wait_ticks 2
FF0=$(cur_tick); wait_ticks 30; FF1=$(cur_tick)
srv_cmd "tp $PN 0.5 -60 40.5 0 0" "Teleported $PN to" || { echo "  FAIL  vine tp not confirmed"; ERR=1; }
wait_ticks 30
xdotool keydown w
VC0=$(cur_tick); wait_ticks 60; VC1=$(cur_tick)
xdotool keyup w
wait_ticks 4
xdotool keydown shift
wait_ticks 10
VH0=$(cur_tick); wait_ticks 40; VH1=$(cur_tick)
xdotool keyup shift
VD0=$(cur_tick); wait_ticks 30; VD1=$(cur_tick)
srv_cmd "tp $PN 10.5 -60 40.5 0 0" "Teleported $PN to" || { echo "  FAIL  scaffold tp not confirmed"; ERR=1; }
wait_ticks 30
xdotool keydown w
SC0=$(cur_tick); wait_ticks 60; SC1=$(cur_tick)
xdotool keyup w
srv_cmd "data get entity $PN Pos" "$PN has the following entity data" && grep "$PN has the following entity data" "$W/server.log" | tail -1 > "$W/scaf.pos"
echo "marks lsink=$LS0-$LS1 lhold=$LH0-$LH1 lexit=$LX0-$LX1 flow=$FL0-$FL1 fall=$FF0-$FF1 vine=$VC0-$VC1 vhold=$VH0-$VH1 vdown=$VD0-$VD1 scaf=$SC0-$SC1"
python3 -I tools/mc/play/check_fluid.py "$W/a.out" "$W" "$LS0:$LS1" "$LH0:$LH1" "$LX0:$LX1" "$FL0:$FL1" "$FF0:$FF1" "$VC0:$VC1" "$VH0:$VH1" "$VD0:$VD1" "$SC0:$SC1" || ERR=1
if grep -qE "moved too quickly|moved wrongly" "$W/server.log"; then
    echo "  FAIL  server complains about movement:"; grep -E "moved too quickly|moved wrongly" "$W/server.log" | head -3; ERR=1
fi
grep -qiE "invalid|disconnect|kicked|illegal|protocol error" "$W/server.log" && { grep -iE "invalid|disconnect|kicked|illegal|protocol error" "$W/server.log" | head -3; ERR=1; }
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-fluid: ok" || echo "RESULT mc-play-fluid: FAIL"
exit $ERR
