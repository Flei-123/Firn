#!/usr/bin/env bash
# tools/mc/play/run_cont.sh [mc_version=1.20.1] -- container windows against an official server: title from Open Window, hopper (5) / dispenser (3x3) layout, sneak + right click on a chest places the held block

# Prints `RESULT mc-play-cont: ok|FAIL`. Env as run_inv.sh (FIRNC, JAVA, MC_VIEW, MCPLAY_KEEP_DIR).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
MCV=${1:-1.20.1}; VIEW=${MC_VIEW:-3}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
[ "$PROTO" != 0 ] || { echo "unknown version $MCV (set MCPLAY_PROTOCOL)"; exit 2; }
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; sleep 1; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-cont: FAIL"; exit 1; }
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
# one mcplay run per scenario (MCPLAY_SNEAK_TEST): $1 = player, $2 = block to set next to the player, $3 = force sneaking (0/1), $4 = output file.
# The script waits for the given stone, right clicks the top face of the block and prints one `sneaktest` line.
run_case() {
    MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_SNEAK_TEST=1 MCPLAY_FORCE_SNEAK=$3 MCPLAY_RUN_S=18 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
        timeout 120 "$W/mcplay" 127.0.0.1 "$PORT" "$1" > "$4" 2>&1 &
    PP=$!
    sleep 5
    echo "gamemode survival $1" >&3; echo "give $1 stone 32" >&3
    echo "execute at $1 run setblock ~1 ~ ~ minecraft:$2" >&3
    sleep 1
    echo "execute at $1 run item replace block ~1 ~ ~ container.0 with minecraft:dirt 3" >&3
    wait $PP
}
check() { # $1 = label, $2 = output file, $3.. = substrings the sneaktest line must contain
    local lbl=$1 f=$2 line; shift 2
    line=$(grep 'mcplay: sneaktest' "$f" | head -1)
    echo "  $lbl: $line"
    [ -n "$line" ] || { echo "  FAIL  $lbl: no sneaktest line"; ERR=1; return; }
    for pat in "$@"; do case "$line" in *"$pat"*) ;; *) echo "  FAIL  $lbl: missing '$pat'"; ERR=1;; esac; done
}
run_case ContA chest 1 "$W/a.out"
run_case ContB chest 0 "$W/b.out"
run_case ContC hopper 0 "$W/c.out"
run_case ContD dispenser 0 "$W/d.out"
# Sneak state: Entity Action up to 1.21.5, Player Input (shift) from 1.21.6 (+ both on 1.21.2 - 1.21.5, like the vanilla client)
check "sneak+click (block placed, chest stays closed)" "$W/a.out" "sneak=true win=0 " "sneak_packets=1"
case "$(grep 'mcplay: sneaktest' "$W/a.out" | head -1)" in *"placed=0 "*) echo "  FAIL  sneak: no block appeared above the chest"; ERR=1;; esac
check "click (chest opens)" "$W/b.out" "sneak=false " "slots=27 cols=9 title=Chest " "placed=0 "
check "hopper" "$W/c.out" "slots=5 cols=5 title=Item Hopper "
check "dispenser" "$W/d.out" "slots=9 cols=3 title=Dispenser "
for f in a b c d; do
    BAD=$(grep -o 'bad=[0-9]*' "$W/$f.out" | head -1)
    case "$(grep 'mcplay: sneaktest' "$W/$f.out" | head -1)" in *"win=0 "*) [ $f = a ] || { echo "  FAIL  $f: no window"; ERR=1; };; esac
done
grep -qiE "invalid|disconnect|kicked" "$W/server.log" && grep -iE "invalid|disconnect|kicked" "$W/server.log" | head -3
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-cont: ok" || echo "RESULT mc-play-cont: FAIL"
exit $ERR
