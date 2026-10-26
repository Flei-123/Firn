#!/usr/bin/env bash
# tools/mc/play/run_crack.sh -- player model: third person self + remote player, first person hand, screenshots.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""; PP=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "${BOT_PID:-}" ] && kill "$BOT_PID" 2>/dev/null; [ -n "${XV2_PID:-}" ] && kill "$XV2_PID" 2>/dev/null; [ "${HAND_WINE:-0}" = 1 ] && WINEPREFIX="$W/wp" wineserver -k 2>/dev/null; [ -n "${MCPLAY_KEEP_DIR:-}" ] && { mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; }; [ -n "$PP" ] && kill "$PP" 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-hand: FAIL"; exit 1; }
for t in Xvfb xdotool xwd python3 java; do command -v $t >/dev/null || fail "missing tool: $t"; done
RUN="$W/mcplay"
if [ "${HAND_WINE:-0}" = 1 ]; then
    command -v wine >/dev/null || fail "missing tool: wine"
    $FIRNC --target=x86_64-windows --opt-level=dev-fast -o "$W/mcplay.exe" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not cross-compile"; }
    RUN="wine $W/mcplay.exe"
    export MCPLAY_GL=0
else
    $FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
fi
MCV=${MC_VERSION:-1.20.1}; VIEW=${MC_VIEW:-4}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; *) PROTO=${MCPLAY_PROTOCOL:-763};; esac
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
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
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
if [ "${HAND_WINE:-0}" = 1 ]; then export WINEPREFIX="$W/wp" WINEDEBUG=-all WINEARCH=win64; wine wineboot -i >/dev/null 2>&1; fi
export MC_VANILLA_JAR=${MC_VANILLA_JAR:-/root/.w5/jars/client-$MCV.jar}
[ -f "$MC_VANILLA_JAR" ] || fail "missing $MC_VANILLA_JAR (tools/mcclient/fetch_client.sh)"
ERR=0
BOT_PID=""
BDN=$(bash tools/mc/pick_display.sh 200 100)
Xvfb :$BDN -screen 0 640x400x24 >"$W/xvfb2.log" 2>&1 & XV2_PID=$!
sleep 1
wj() { local i; for i in $(seq 1 1800); do grep -q "$1 joined the game" "$W/server.log" 2>/dev/null && return 0; sleep 0.1; done; echo "  WARN  $1 did not join"; return 1; }
prog() { grep -o "progress .*" "$W/$1.out" 2>/dev/null | tail -1 | grep -o " $2=[^ ]*" | cut -d= -f2; }
wait_ticks() { local t0 t; t0=$(prog "$1" ticks); t0=${t0:-0}; for i in $(seq 1 $(( ${3:-180} * 10 ))); do t=$(prog "$1" ticks); [ "${t:-0}" -ge $(( t0 + $2 )) ] && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 0.1; done; echo "  WARN  wait_ticks $2 timeout"; return 1; }
win() { local i w; for i in $(seq 1 60); do w=$(xdotool search --name mcplay 2>/dev/null | head -1); [ -n "$w" ] && { echo "$w"; return 0; }; sleep 0.5; done; return 1; }
shot() { xwd -id "$WIN" -silent -out "$W/$1.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/$1.xwd" "$W/$1.ppm"; }
keep() { [ -n "${MCPLAY_KEEP_DIR:-}" ] && python3 -I -c "from PIL import Image; Image.open('$W/$1.ppm').save('$W/$1.png')"; }
MCPLAY_PROGRESS=1 MCPLAY_AUDIO=0 MCPLAY_RUN_S=${HAND_RUN_S:-120} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_CAM3P=${HAND_CAM:-1} \
    MCPLAY_QUIT_FILE="$W/quit-V" timeout 200 $RUN 127.0.0.1 "$PORT" HandV > "$W/HandV.out" 2>&1 &
PP=$!
wj HandV || ERR=1
for i in $(seq 1 1200); do [ "$(prog HandV pos)" = true ] && [ "$(prog HandV chunks)" -gt 10 ] 2>/dev/null && break; sleep 0.1; done
WIN=$(win) || { echo "  FAIL  no window"; ERR=1; }
for cmd in "forceload add 0 0" "fill -6 119 -6 6 120 6 stone" "fill -6 121 -6 6 128 6 air" "gamemode survival HandV" "give HandV minecraft:diamond_sword" "give HandV minecraft:apple" "give HandV minecraft:grass_block 8"; do echo "$cmd" >&3; done
sleep 2
echo "tp HandV 0.5 121 0.5 0 5" >&3
MCPLAY_PROGRESS=1 MCPLAY_AUDIO=0 MCPLAY_RUN_S=100 MCPLAY_VIEW=$VIEW MCPLAY_FPS=10 MCPLAY_W=320 MCPLAY_H=200 \
    MCPLAY_QUIT_FILE="$W/quit-B" DISPLAY=:$BDN timeout 150 $RUN 127.0.0.1 "$PORT" HandB > "$W/HandB.out" 2>&1 &
BOT_PID=$!
wj HandB || ERR=1
echo "gamemode survival HandB" >&3
echo "tp HandB 3.5 121 -2.5 180 0" >&3
wait_ticks HandV 40
shot self; keep self
echo "tp HandV 0.5 121 0.5 -35 10" >&3
wait_ticks HandV 30
shot angle; keep angle
xdotool mousedown 1; sleep 0.15; shot swing; keep swing; xdotool mouseup 1
[ -f "$W/self.ppm" ] || ERR=1
touch "$W/quit-V" "$W/quit-B"
if [ $ERR = 0 ]; then echo "RESULT mc-play-hand: ok"; else echo "RESULT mc-play-hand: FAIL"; exit 1; fi
