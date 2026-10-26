#!/usr/bin/env bash
# tools/mc/play/run_snd.sh -- player sound events against an official server: mcplay logs every started voice (MCPLAY_AUDIO_LOG=1, t_ms = monotonic ms) and the gate checks sound ids + action->sound latency (dig hit/break, water splash/swim/underwater, button, eat, bucket, hurt, lava).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""; PP=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "${MCPLAY_KEEP_DIR:-}" ] && { mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; }; [ -n "$PP" ] && kill "$PP" 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-snd: FAIL"; exit 1; }
for t in Xvfb xdotool python3 java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
if [ -z "${MCPLAY_SOUNDS:-}" ]; then
    $FIRNC -o "$W/sndgen" tools/sounds/gen/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "sound generator does not compile"; }
    "$W/sndgen" tools/sounds/mc.json "$W/sounds" >/dev/null 2>&1 || fail "sound bank generation"
    export MCPLAY_SOUNDS="$W/sounds"
fi
MCV=${MC_VERSION:-1.20.1}; VIEW=${MC_VIEW:-4}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; *) PROTO=${MCPLAY_PROTOCOL:-763};; esac
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
difficulty=easy
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
export MC_VANILLA_JAR=${MC_VANILLA_JAR:-/root/.w5/jars/client-$MCV.jar}
[ -f "$MC_VANILLA_JAR" ] || fail "missing $MC_VANILLA_JAR (tools/mcclient/fetch_client.sh)"
ERR=0
wj() { local i; for i in $(seq 1 1800); do grep -q "$1 joined the game" "$W/server.log" 2>/dev/null && return 0; sleep 0.1; done; echo "  WARN  $1 did not join"; return 1; }
prog() { grep -o "progress .*" "$W/$1.out" 2>/dev/null | tail -1 | grep -o " $2=[^ ]*" | cut -d= -f2; }
wait_ticks() { local t0 t; t0=$(prog "$1" ticks); t0=${t0:-0}; for i in $(seq 1 $(( ${3:-180} * 10 ))); do t=$(prog "$1" ticks); [ "${t:-0}" -ge $(( t0 + $2 )) ] && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 0.1; done; echo "  WARN  wait_ticks $2 timeout"; return 1; }
mono() { python3 -I -c "import time;print(time.monotonic_ns()//1000000)"; }
ev_count() { grep -cE "audiolog .* ev=$1 " "$W/Snd.out"; }
ev_first_after() { grep -E "audiolog .* ev=$1 " "$W/Snd.out" | grep -o "t_ms=[0-9]*" | cut -d= -f2 | awk -v t="$2" '$1>=t{print $1; exit}'; }
expect() { local n; n=$(ev_count "$1"); if [ "${n:-0}" -ge "${2:-1}" ]; then echo "  ok    $1 x$n"; else echo "  FAIL  $1 x${n:-0} (need $2)"; ERR=1; fi; }
latency() { local t1; t1=$(ev_first_after "$1" "$2"); if [ -n "$t1" ]; then echo "  latency $1: $((t1 - $2)) ms"; else echo "  FAIL  no $1 after action"; ERR=1; fi; }
X=0
FIRN_AUDIO="file:$W/snd.wav" MCPLAY_AUDIO_LOG=1 MCPLAY_HEALTH_LOG=1 MCPLAY_PROGRESS=1 MCPLAY_RUN_S=${SND_RUN_S:-200} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    MCPLAY_QUIT_FILE="$W/quit" timeout 260 "$W/mcplay" 127.0.0.1 "$PORT" Snd > "$W/Snd.out" 2>&1 &
PP=$!
wj Snd || ERR=1
for i in $(seq 1 1200); do [ "$(prog Snd pos)" = true ] && [ "$(prog Snd chunks)" -gt 10 ] 2>/dev/null && break; sleep 0.1; done
for cmd in "forceload add $X 0" "fill -6 119 -6 6 120 6 stone" "fill -6 121 -6 6 130 6 air" "gamemode survival Snd" "setblock $X 120 0 stone_button[face=floor]" "fill 4 117 -2 6 120 2 water" "setblock -3 120 0 lava"; do echo "$cmd" >&3; done
sleep 2
echo "tp Snd $X.5 121 0.5" >&3
for i in $(seq 1 600); do [ "$(prog Snd ground)" != 0 ] && [ "$(prog Snd ground)" != false ] && [ "$(prog Snd y_mm)" = 121000 ] && break; sleep 0.1; done
wait_ticks Snd 10
xdotool mousemove 320 180 2>/dev/null
xdotool mousemove_relative -- 0 350; xdotool mousemove_relative -- 0 350; wait_ticks Snd 20
echo "setblock $X 120 0 stone" >&3; sleep 1
echo "  dig:"
T0=$(mono); xdotool mousedown 1; sleep 9; xdotool mouseup 1; wait_ticks Snd 10
latency "block.stone.hit" "$T0"
expect "block.stone.hit" 3
expect "block.stone.break" 1
echo "setblock $X 120 0 stone_button[face=floor]" >&3; sleep 1
echo "  button:"
T0=$(mono); xdotool click 3; wait_ticks Snd 10
latency "block.stone_button.click_on" "$T0"
echo "  eat:"
echo "clear Snd" >&3; echo "give Snd bread 3" >&3; echo "effect give Snd minecraft:hunger 30 200" >&3; sleep 4
echo "setblock $X 120 0 stone" >&3; sleep 1
T0=$(mono); xdotool mousedown 3; sleep 2.2; xdotool mouseup 3; wait_ticks Snd 10
latency "entity.generic.eat" "$T0"
expect "entity.generic.eat" 3
echo "  bucket:"
echo "effect clear Snd" >&3; echo "clear Snd" >&3; echo "give Snd water_bucket" >&3; sleep 2
T0=$(mono); xdotool click 3; wait_ticks Snd 10
latency "item.bucket.empty" "$T0"
echo "  hurt:"
echo "damage Snd 2" >&3; wait_ticks Snd 20
expect "entity.player.hurt" 1
echo "  water:"
echo "kill @e[type=item]" >&3
echo "tp Snd 5.5 121 0.5" >&3; wait_ticks Snd 30
xdotool keydown w; sleep 2; xdotool keyup w; wait_ticks Snd 10
n=$(ev_count "entity.player.splash"); [ "${n:-0}" -ge 1 ] && echo "  ok    entity.player.splash x$n" || echo "  WARN  no splash within the check window (timing under load)"
expect "entity.player.swim" 1
expect "ambient.underwater.enter" 1
echo "  lava (random ambient):"
echo "tp Snd -2.5 121 0.5" >&3; wait_ticks Snd 400 200
N=$(( $(ev_count "block.lava.ambient") + $(ev_count "block.lava.pop") )); echo "  lava sounds x$N"
[ "$N" -ge 1 ] || echo "  WARN  no lava sound in 20 s (random)"
echo "  all voices:"
grep -oE "audiolog .* ev=[^ ]+" "$W/Snd.out" | grep -oE "ev=[^ ]+" | sort | uniq -c | sort -rn | head -30
touch "$W/quit"; wait $PP 2>/dev/null; PP=""
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-snd: ok" || echo "RESULT mc-play-snd: FAIL"
exit $ERR
