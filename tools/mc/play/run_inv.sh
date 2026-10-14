#!/usr/bin/env bash
# tools/mc/play/run_inv.sh [mc_version=1.21.4] -- mcplay inventory + survival dig against an official server of ANY version
# (data components from 1.20.5: mc_inv reads Window Items / Set Slot through the packet schema, mc_dig uses the version's tool table).
# One server, Xvfb, mcplay (CPU raster): /give an iron pickaxe + stone, look down, hold the left button.
# Prints `RESULT mc-play-inv: ok|FAIL`. Env: RUN_INV_CMDS (extra server commands in the survival stage), MCPLAY_RAW_INV=1 (rawinv lines = real packet bodies for test fixtures), FIRNC, JAVA (1.21+ needs Java 21+; default /root/tools/jdk25/bin/java when present), MC_VIEW (3), MCPLAY_KEEP_DIR.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
MCV=${1:-1.21.4}; VIEW=${MC_VIEW:-3}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
[ "$PROTO" != 0 ] || { echo "unknown version $MCV (set MCPLAY_PROTOCOL)"; exit 2; }
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; sleep 1; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-inv: FAIL"; exit 1; }
for t in Xvfb xdotool; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
SND=${MCPLAY_SOUNDS:-${XDG_CACHE_HOME:-$HOME/.cache}/mcplay/sounds}
export MCPLAY_SOUNDS="$SND"
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
sleep "${RUN_INV_WARMUP:-0}" # a cold 1.21.11 / 26.1 server answers the first login late
for gr in "doDaylightCycle false" "doMobSpawning false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((120 + RANDOM % 70))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
# ---- creative: Set Creative Slot (MCPLAY_GIVE) and the server's Window Items / Set Slot
MCPLAY_AUDIO=0 MCPLAY_GIVE="iron_pickaxe:1,stone:64" MCPLAY_RUN_S=8 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 120 "$W/mcplay" 127.0.0.1 "$PORT" Player > "$W/creative.out" 2>&1
CU=$(grep -o 'inv updates=[0-9]*' "$W/creative.out" | head -1 | cut -d= -f2); CB=$(grep -o ' bad=[0-9]*' "$W/creative.out" | head -1 | cut -d= -f2)
echo "  creative proto=$PROTO inv_updates=${CU:-?} bad=${CB:-?} $(grep -o 'hotbar=[^ ]*' "$W/creative.out" | head -1)"
[ "${CU:-0}" -gt 0 ] && [ "${CB:-1}" -eq 0 ] || { echo "  FAIL  creative: inventory updates=${CU:-0} bad=${CB:-?}"; ERR=1; }
# ---- survival: /give iron pickaxe, look down, hold the left button -> a block is dug with the table time (mc_dig)
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_RUN_S=14 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 120 "$W/mcplay" 127.0.0.1 "$PORT" Player6 > "$W/surv.out" 2>&1 &
PP=$!
sleep 5
echo "gamemode survival Player6" >&3; echo "give Player6 iron_pickaxe 1" >&3; echo "give Player6 stone 32" >&3
[ -n "${RUN_INV_CMDS:-}" ] && printf '%s\n' "$RUN_INV_CMDS" >&3 # extra console commands, one per line (e.g. a give with components)
sleep 2
xdotool mousemove 320 180 2>/dev/null; xdotool mousemove_relative -- 0 300; xdotool key 1; sleep 0.5
xdotool mousedown 1; sleep 4; xdotool mouseup 1; sleep 1
wait $PP; src=$?
[ $src -eq 0 ] || { echo "  FAIL  survival run exit code $src"; ERR=1; }
SD=$(grep -o ' dig=[0-9]*' "$W/surv.out" | head -1 | cut -d= -f2); SMS=$(grep -o 'dig_ms=[0-9]*' "$W/surv.out" | head -1 | cut -d= -f2)
SU=$(grep -o 'inv updates=[0-9]*' "$W/surv.out" | head -1 | cut -d= -f2); SB=$(grep -o ' bad=[0-9]*' "$W/surv.out" | head -1 | cut -d= -f2)
echo "  survival dig=${SD:-?} dig_ms=${SMS:-?} inv_updates=${SU:-?} bad=${SB:-?} $(grep -o 'hotbar=[^ ]*' "$W/surv.out" | head -1)"
[ "${SD:-0}" -ge 1 ] || { echo "  FAIL  survival: nothing dug"; ERR=1; }
[ "${SU:-0}" -gt 0 ] && [ "${SB:-1}" -eq 0 ] || { echo "  FAIL  survival: inventory updates=${SU:-0} bad=${SB:-?}"; ERR=1; }
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-inv: ok" || echo "RESULT mc-play-inv: FAIL"
exit $ERR
