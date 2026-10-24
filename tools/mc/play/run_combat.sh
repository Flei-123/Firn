#!/usr/bin/env bash
# tools/mc/play/run_combat.sh [mc_version=1.20.1] -- combat against an official server: entity pick under the crosshair -> attack (cow loses 2 hp), interact, use item (snowball is consumed), release

# Also: Q drops one snowball, F swaps it into the offhand, holding LMB repeats the attack (cooldown), hit feedback from the damage event.
# Prints `RESULT mc-play-combat: ok|FAIL`. Env as run_inv.sh (FIRNC, JAVA, MC_VIEW, MCPLAY_KEEP_DIR).
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
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-combat: FAIL"; exit 1; }
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
gamemode=survival
force-gamemode=true
level-seed=4242
view-distance=$VIEW
simulation-distance=$VIEW
spawn-protection=0
difficulty=easy
spawn-monsters=false
spawn-animals=true
spawn-npcs=false
enable-rcon=false
enable-query=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
sleep "${RUN_INV_WARMUP:-0}"
for gr in "doDaylightCycle false" "doMobSpawning false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((120 + RANDOM % 70))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_COMBAT_TEST=1 MCPLAY_COMBAT_LOG=1 MCPLAY_RUN_S=${COMBAT_RUN_S:-90} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 200 "$W/mcplay" 127.0.0.1 "$PORT" CombatA > "$W/a.out" 2>&1 &
PP=$!
wait_for() { # pattern, seconds
    local i
    for i in $(seq 1 "$2"); do grep -Eq "$1" "$W/a.out" 2>/dev/null && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 1; done
    return 1
}
wait_srv() { # pattern, seconds
    local i
    for i in $(seq 1 "$2"); do grep -Eq "$1" "$W/server.log" 2>/dev/null && return 0; sleep 1; done
    return 1
}
for i in $(seq 1 90); do grep -q "CombatA joined the game" "$W/server.log" 2>/dev/null && break; sleep 1; done
sleep 6
echo "give CombatA minecraft:snowball 5" >&3
echo 'execute at CombatA run fill ~-3 ~-1 ~-3 ~3 ~-1 ~3 minecraft:stone' >&3
echo 'execute at CombatA run fill ~-3 ~ ~-3 ~3 ~3 ~3 minecraft:air' >&3
sleep 2
echo 'execute at CombatA run summon minecraft:cow ~ ~ ~2 {NoAI:1b,Tags:["tgt"]}' >&3
sleep 3
wait_for 'combat_test done' 45 || { echo "  FAIL  combat script did not finish"; ERR=1; }
[ "$(grep -c 'combat attack target=.* sent=true' "$W/a.out")" -ge 2 ] || { echo "  FAIL  fewer than 2 attacks sent"; ERR=1; }
grep -q 'combat interact target=.* sent=true' "$W/a.out" || { echo "  FAIL  interact not sent"; ERR=1; }
grep -q 'combat use sent=true' "$W/a.out" || { echo "  FAIL  use item not sent"; ERR=1; }
grep -q 'combat release sent=true' "$W/a.out" || { echo "  FAIL  release not sent"; ERR=1; }
sleep 1
echo 'data get entity @e[tag=tgt,limit=1] Health' >&3
wait_srv 'Cow has the following entity data: 8\.0f' 15 || { echo "  FAIL  cow health is not 8.0 (two attacks)"; grep 'entity data' "$W/server.log" | tail -2; ERR=1; }
echo "clear CombatA minecraft:snowball 0" >&3
wait_srv 'Found 4 matching item\(s\) on player CombatA' 15 || { echo "  FAIL  snowball was not consumed by use item"; grep -E 'matching|No items' "$W/server.log" | tail -2; ERR=1; }
xdotool key q; sleep 1
echo "clear CombatA minecraft:snowball 0" >&3
wait_srv 'Found 3 matching item\(s\) on player CombatA' 15 || { echo "  FAIL  Q did not drop one snowball"; grep -E 'matching|No items' "$W/server.log" | tail -2; ERR=1; }
grep -q 'combat drop all=false sent=true' "$W/a.out" || { echo "  FAIL  drop packet not sent"; ERR=1; }
xdotool key f; sleep 1
echo 'data get entity CombatA Inventory' >&3
wait_srv 'Slot: -106b.{0,60}snowball|snowball.{0,60}Slot: -106b' 15 || { echo "  FAIL  F did not swap the snowballs into the offhand"; grep 'entity data' "$W/server.log" | tail -1 | cut -c1-300; ERR=1; }
grep -q 'combat swap sent=true' "$W/a.out" || { echo "  FAIL  swap packet not sent"; ERR=1; }
xdotool mousedown 1; sleep 1.7; xdotool mouseup 1; sleep 1
echo 'data get entity @e[tag=tgt,limit=1] Health' >&3
wait_srv 'Cow has the following entity data: [0-5]\.0f' 15 || { echo "  FAIL  holding LMB did not repeat the attack (cow health still >= 6)"; grep 'entity data' "$W/server.log" | tail -2; ERR=1; }
[ "$(grep -c 'combat attack target=.* sent=true' "$W/a.out")" -ge 5 ] || { echo "  FAIL  hold-to-repeat sent fewer than 3 extra attacks"; ERR=1; }
grep -Eq 'combat fx hurts=[0-9]+ hits=[1-9]' "$W/a.out" || { echo "  FAIL  no confirmed hit feedback (damage event / status) for the attacked cow"; ERR=1; }
grep -E 'combat fx' "$W/a.out" | tail -1
grep -qiE "invalid|disconnect|kicked|illegal|protocol error" "$W/server.log" && { grep -iE "invalid|disconnect|kicked|illegal|protocol error" "$W/server.log" | head -3; ERR=1; }
grep 'mcplay: combat' "$W/a.out" | grep -v 'combat attack' | head -12
echo stop >&3; sleep 1
kill "$PP" 2>/dev/null
[ $ERR -eq 0 ] && echo "RESULT mc-play-combat: ok" || echo "RESULT mc-play-combat: FAIL"
exit $ERR
