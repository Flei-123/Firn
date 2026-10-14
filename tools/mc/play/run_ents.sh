#!/usr/bin/env bash
# tools/mc/play/run_ents.sh -- entities in the mcplay play path against the REAL 1.20.1 server (Xvfb, offline, creative).
# Stage 1 (CPU): mobs (cow, pig, sheep, armor stand) are summoned and walked around, a second client joins and leaves -> the end line
#   `mcplay: entities ...` must show spawns / moves|teleports / destroys / a joined player, bad=0, and boxes were drawn.
# Stage 2 (pictures): the mobs stand still; per back end (CPU, GL = llvmpipe, NOT a real GPU) one run with the boxes and one with
#   MCPLAY_ENTITIES=0 -> the pictures must differ (boxes visible), CPU-with-boxes vs GL-with-boxes PSNR must be >= MCPLAY_ENTS_PSNR_MIN.
# Env: MC_VIEW (4), FIRNC, MCPLAY_SKIP_GL=1, MCPLAY_ENTS_PNG_OUT=<prefix> (copies cpu_on/cpu_off/gl_on/gl_off PNGs). Start it via heavy.
# Prints `RESULT mc-ents: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-ents: FAIL"; exit 1; }
for t in Xvfb java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=1.20.1; VIEW=${MC_VIEW:-4}
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
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "doMobLoot false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
num() { grep "mcplay: entities" "$1" | grep -o " $2=[0-9]*" | head -1 | cut -d= -f2; }
# waits (max 90 s) until the server logged the join of <name>, then a short grace for the chunks / entity packets
wait_join() { for i in $(seq 1 180); do grep -q "$1 joined the game" "$W/server.log" && { sleep 2; return 0; }; sleep 0.5; done; return 1; }
# flat platform high above the terrain, free air above; the mobs stand in front of the player (+z) between 4 and 9 blocks away
platform() { for cmd in "fill -20 119 -20 20 120 20 stone" "fill -20 121 -20 20 128 20 air" "tp $1 0.5 121 0.5 0 0"; do echo "$cmd" >&3; done; }
# ---- stage 1: live packets (spawn / relative move / teleport / destroy / named player) --------------------------------------
MCPLAY_ENT_LIST=1 MCPLAY_AUDIO=0 MCPLAY_RUN_S=22 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/live.png" \
    "$W/mcplay" 127.0.0.1 "$PORT" Watcher > "$W/live.out" 2>&1 &
PP=$!
wait_join Watcher || fail "Watcher never joined"
platform Watcher
sleep 2
for cmd in "summon minecraft:cow 0.5 121 5.5 {NoAI:1b,Tags:[\"ent\"]}" "summon minecraft:pig 2.5 121 6.5 {NoAI:1b,Tags:[\"ent\"]}" "summon minecraft:sheep -1.5 121 6.5 {NoAI:1b,Tags:[\"ent\"]}" "summon minecraft:armor_stand -3.5 121 8.5 {Tags:[\"ent\"]}" "summon minecraft:cow 4.5 121 8.5 {NoAI:1b,Tags:[\"ent\"]}"; do echo "$cmd" >&3; done
sleep 1
echo "data get entity @e[tag=ent,type=minecraft:cow,limit=1] Pos" >&3; echo "data get entity @e[tag=ent,type=minecraft:cow,limit=1] Health" >&3
sleep 1
# walk two mobs: small steps = relative move packets, one long jump = teleport packet
for i in 1 2 3 4 5 6; do
    echo "execute as @e[tag=ent,type=minecraft:pig] at @s run tp @s ~0.3 ~ ~" >&3
    echo "execute as @e[tag=ent,type=minecraft:sheep] at @s run tp @s ~ ~ ~0.25" >&3
    sleep 0.35
done
echo "execute as @e[tag=ent,type=minecraft:armor_stand] at @s run tp @s ~12 ~ ~" >&3
sleep 1
# a second player joins (named spawn + player info) and leaves (destroy + player remove)
MCPLAY_AUDIO=0 MCPLAY_RUN_S=4 MCPLAY_VIEW=2 MCPLAY_FPS=10 MCPLAY_W=320 MCPLAY_H=180 "$W/mcplay" 127.0.0.1 "$PORT" Visitor > "$W/visitor.out" 2>&1
echo "kill @e[tag=ent,type=minecraft:sheep]" >&3
wait $PP; rc=$?
sed 's/^/  /' "$W/live.out" | grep -E "mcplay: (entities|end)" | head -4
[ $rc -eq 0 ] || { echo "  FAIL  live run exit code $rc"; ERR=1; }
grep -q "joined the game" "$W/server.log" || { echo "  FAIL  server never saw the join"; ERR=1; }
SP=$(num "$W/live.out" spawns); PK=$(num "$W/live.out" peak); MV=$(num "$W/live.out" moves); TP=$(num "$W/live.out" teleports)
DS=$(num "$W/live.out" destroys); BD=$(num "$W/live.out" bad); BX=$(num "$W/live.out" boxes); VM=$(num "$W/live.out" vis_max); PL=$(num "$W/live.out" players)
[ "${SP:-0}" -ge 5 ] || { echo "  FAIL  spawns ${SP:-0} < 5 (4 mobs + 1 armor stand + visitor expected)"; ERR=1; }
[ "${PK:-0}" -ge 5 ] || { echo "  FAIL  peak ${PK:-0} < 5"; ERR=1; }
[ "${MV:-0}" -ge 3 ] || { echo "  FAIL  moves ${MV:-0} < 3 (walking mobs send relative moves)"; ERR=1; }
[ "${TP:-0}" -ge 1 ] || { echo "  FAIL  teleports ${TP:-0} < 1 (the armor stand jumped 12 blocks)"; ERR=1; }
[ "${DS:-0}" -ge 1 ] || { echo "  FAIL  destroys ${DS:-0} < 1 (sheep killed / visitor left)"; ERR=1; }
[ "${BD:-1}" -eq 0 ] || { echo "  FAIL  bad entity packets ${BD:-?}"; ERR=1; }
[ "${BX:-0}" -gt 100 ] || { echo "  FAIL  boxes drawn ${BX:-0} <= 100"; ERR=1; }
[ "${VM:-0}" -ge 3 ] || { echo "  FAIL  visible entities max ${VM:-0} < 3"; ERR=1; }
echo "  live: spawns=${SP:-?} peak=${PK:-?} moves=${MV:-?} teleports=${TP:-?} destroys=${DS:-?} bad=${BD:-?} players_at_end=${PL:-?} boxes=${BX:-?} vis_max=${VM:-?}"
if [ "${MCPLAY_ENTS_ONLY_LIVE:-0}" = 1 ]; then grep -E "mcplay: (ent|cam)" "$W/live.out" | head -24; echo stop >&3; sleep 1; [ $ERR -eq 0 ] && echo "RESULT mc-ents: ok (live only)" || echo "RESULT mc-ents: FAIL"; exit $ERR; fi
# ---- stage 2: pictures with / without the boxes, CPU and GL ---------------------------------------------------------------
shot() { # <name> <gl 0|1> <entities 0|1>
    local nm=$1 gl=$2 en=$3
    MCPLAY_GL=$gl MCPLAY_VSYNC=0 MCPLAY_ENTITIES=$en MCPLAY_AUDIO=0 MCPLAY_RUN_S=12 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/$nm.png" \
        timeout 180 "$W/mcplay" 127.0.0.1 "$PORT" "P_$nm" > "$W/$nm.out" 2>&1 &
    local pid=$!
    wait_join "P_$nm" || { echo "  FAIL  $nm never joined"; ERR=1; }
    echo "tp P_$nm 0.5 121 0.5 0 0" >&3
    sleep 1
    wait $pid; local r=$?
    [ $r -eq 0 ] || { echo "  FAIL  $nm run exit code $r"; ERR=1; }
    echo "  $nm: $(grep -o 'frames=[0-9]*' "$W/$nm.out" | head -1) $(grep -o 'hash=[0-9a-f]*' "$W/$nm.out" | head -1) $(grep -o 'entities_draw boxes=[0-9]*' "$W/$nm.out" | head -1) $(grep -o 'in=[0-9]* culled=[0-9]*' "$W/$nm.out" | head -1) $(grep -o 'pos x=[-0-9]* y=[-0-9]* z=[-0-9]*' "$W/$nm.out" | head -1)"
}
# the stage 1 mobs are still there: re-place the survivors in a fixed pattern so every picture shows the same scene
for cmd in "kill @e[tag=ent,type=minecraft:armor_stand]" "kill @e[tag=ent,type=minecraft:pig]" "kill @e[tag=ent,type=minecraft:cow]" \
           "summon minecraft:cow 0.5 121 5.5 {NoAI:1b,Tags:[\"ent\"],Rotation:[0f,0f]}" "summon minecraft:pig 2.5 121 6.5 {NoAI:1b,Tags:[\"ent\"],Rotation:[0f,0f]}" \
           "summon minecraft:pig -1.5 121 6.5 {NoAI:1b,Tags:[\"ent\"],Rotation:[0f,0f]}" "summon minecraft:cow 3.5 121 9.5 {NoAI:1b,Tags:[\"ent\"],Rotation:[0f,0f]}"; do echo "$cmd" >&3; done
sleep 2
shot cpu_on 0 1
shot cpu_off 0 0
if [ "${MCPLAY_SKIP_GL:-0}" != 1 ]; then
    shot gl_on 1 1
    shot gl_off 1 0
fi
cat > "$W/cmp.py" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("RGB")).astype(int)
b = np.asarray(Image.open(sys.argv[2]).convert("RGB")).astype(int)
a[0:50, 0:140] = 0
b[0:50, 0:140] = 0
d = np.abs(a - b).sum(axis=2) > 30
print(int(d.sum()))
PY
CD=$(python3 -I "$W/cmp.py" "$W/cpu_on.png" "$W/cpu_off.png" 2>&1 | tail -1)
echo "  cpu on-vs-off differing pixels: $CD"
[ "${CD:-0}" -ge 300 ] 2>/dev/null || { echo "  FAIL  CPU: boxes not visible in the picture (${CD:-?} differing pixels < 300)"; ERR=1; }
CB=$(num "$W/cpu_on.out" boxes)
[ "${CB:-0}" -gt 0 ] || { echo "  FAIL  CPU on: no boxes drawn"; ERR=1; }
[ "$(num "$W/cpu_off.out" boxes)" = "" ] || [ "$(num "$W/cpu_off.out" boxes)" = 0 ] || { echo "  FAIL  CPU off: boxes were drawn"; ERR=1; }
if [ "${MCPLAY_SKIP_GL:-0}" != 1 ]; then
    GD=$(python3 -I "$W/cmp.py" "$W/gl_on.png" "$W/gl_off.png" 2>&1 | tail -1)
    echo "  gl on-vs-off differing pixels: $GD"
    [ "${GD:-0}" -ge 300 ] 2>/dev/null || { echo "  FAIL  GL: boxes not visible in the picture (${GD:-?} differing pixels < 300)"; ERR=1; }
    [ "$(num "$W/gl_on.out" boxes)" -gt 0 ] 2>/dev/null || { echo "  FAIL  GL on: no boxes drawn"; ERR=1; }
    python3 tools/mc/play/psnr.py "$W/cpu_on.png" "$W/gl_on.png" --min-db "${MCPLAY_ENTS_PSNR_MIN:-28}" --mask 0,0,140,50 --diff "$W/diff.png" || ERR=1
fi
if [ -n "${MCPLAY_ENTS_PNG_OUT:-}" ]; then for n in cpu_on cpu_off gl_on gl_off; do [ -s "$W/$n.png" ] && cp "$W/$n.png" "${MCPLAY_ENTS_PNG_OUT}_$n.png"; done; fi
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-ents: ok" || echo "RESULT mc-ents: FAIL"
exit $ERR
