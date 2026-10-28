#!/usr/bin/env bash
# tools/mc/play/run_depth.sh -- depth test of entities / item entities / own player model / first person hand against a wall, per back end.
# Per back end (cpu, gl = llvmpipe, vk = lavapipe) and scene two pictures are taken (layer on / off) and the differing pixels are counted:
#   ent_wall  mobs + dropped item behind a stone wall      -> 0 differing pixels (hidden)
#   ent_open  same mobs, no wall                           -> many differing pixels (visible)
#   self_wall (opt-in via DEPTH_SCENES, camera placement still unstable) third person camera behind a wall -> 0 differing pixels
#   hand_wall first person hand with a wall right in front -> many differing pixels (hand stays on top, like vanilla)
# Env: DEPTH_BACKENDS ("cpu gl vk"), DEPTH_SCENES, MC_VIEW (4), DEPTH_PNG_OUT=<prefix>. Start it via heavy. Prints `RESULT mc-depth: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "${MCPLAY_KEEP_DIR:-}" ] && { mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; }; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-depth: FAIL"; exit 1; }
for t in Xvfb java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=1.20.1; VIEW=${MC_VIEW:-4}
export MCPLAY_PROTOCOL=763
export MC_VANILLA_JAR=${MC_VANILLA_JAR:-/root/.w5/jars/client-$MCV.jar}
[ -f "$MC_VANILLA_JAR" ] || fail "missing $MC_VANILLA_JAR"
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
spawn-animals=true
spawn-npcs=false
enable-rcon=false
enable-query=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "${JAVA:-/usr/bin/java}" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "doMobLoot false" "doEntityDrops false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
NSHOT=0
wait_join() { for i in $(seq 1 180); do grep -q "$1 joined the game" "$W/server.log" && { sleep 2; return 0; }; sleep 0.5; done; return 1; }
echo "kill @e[type=!player]" >&3
sleep 1
for spec in "pig -3.5 121 7.5" "cow -1.0 121 7.5" "sheep 1.5 121 7.5" "zombie 3.5 121 7.5" "villager 0.5 121 9.5" "pig 0.5 121 -7.5" "cow 2.5 121 -7.5"; do
    set -- $spec
    echo "summon minecraft:$1 $2 $3 $4 {NoAI:1b,PersistenceRequired:1b,Invulnerable:1b,Silent:1b,Rotation:[180f,0f]}" >&3
done
for x in -2.5 -0.5 1.5; do echo "summon minecraft:item $x 121.2 5.5 {Item:{id:\"minecraft:diamond_block\",Count:1b},NoGravity:1b,Age:-32768,PickupDelay:32767}" >&3; done
sleep 2
gfx_env() { case $1 in cpu) echo "MCPLAY_GL=0";; gl) echo "MCPLAY_GL=1";; vk) echo "MCPLAY_GFX=vk";; esac; }
setup_cmds() {
    local scene=$1 who=$2
    echo "fill -30 119 -30 30 120 30 stone" >&3
    echo "fill -30 121 -30 30 126 30 air" >&3
    case $scene in
        ent_wall) echo "fill -30 121 3 30 126 3 stone" >&3; echo "tp $who 0.5 121 0.5 0 0" >&3 ;;
        ent_open) echo "tp $who 0.5 121 0.5 0 0" >&3 ;;
        self_wall) echo "fill -30 121 -1 30 126 -1 stone" >&3; echo "tp $who 0.5 121 0.5 0 0" >&3 ;;
        hand_wall) echo "fill -30 121 1 30 126 1 stone" >&3; echo "tp $who 0.5 121 0.8 0 0" >&3 ;;
    esac
}
shot() {
    local nm=$1 gfx=$2 scene=$3 layer=$4
    NSHOT=$((NSHOT+1)); local PN="D$NSHOT"
    local ex="MCPLAY_ENTITIES=1 MCPLAY_HAND=0 MCPLAY_CAM3P=0"
    case $scene in
        ent_*) [ "$layer" = 0 ] && ex="MCPLAY_ENTITIES=0 MCPLAY_HAND=0 MCPLAY_CAM3P=0" ;;
        self_wall) ex="MCPLAY_CAM3P=1 MCPLAY_ENTITIES=$layer" ;;
        hand_wall) ex="MCPLAY_ENTITIES=1 MCPLAY_HAND=$layer MCPLAY_CAM3P=0" ;;
    esac
    env $ex $(gfx_env $gfx) MCPLAY_VSYNC=0 MCPLAY_AUDIO=0 MCPLAY_RUN_S=${DEPTH_RUN_S:-14} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/$nm.png" \
        timeout 200 "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/$nm.out" 2>&1 &
    local pid=$!
    wait_join "$PN" || { echo "  FAIL  $nm never joined"; ERR=1; }
    setup_cmds "$scene" "$PN"
    sleep 1
    wait $pid; local r=$?
    [ $r -eq 0 ] || { echo "  FAIL  $nm run exit code $r"; tail -3 "$W/$nm.out" | sed 's/^/        /'; ERR=1; }
}
cat > "$W/cmp.py" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("RGB")).astype(int)
b = np.asarray(Image.open(sys.argv[2]).convert("RGB")).astype(int)
a[0:60, 0:200] = 0
b[0:60, 0:200] = 0
print(int((np.abs(a - b).sum(axis=2) > 30).sum()))
PY
for gfx in ${DEPTH_BACKENDS:-cpu gl vk}; do
    for scene in ${DEPTH_SCENES:-ent_wall ent_open hand_wall}; do
        shot "${gfx}_${scene}_on" $gfx $scene 1
        shot "${gfx}_${scene}_off" $gfx $scene 0
        if grep -q "gfx=cpu (vk requested" "$W/${gfx}_${scene}_on.out" 2>/dev/null; then echo "  $gfx: SKIP (no Vulkan device)"; break; fi
        n=$(python3 -I "$W/cmp.py" "$W/${gfx}_${scene}_on.png" "$W/${gfx}_${scene}_off.png" 2>&1 | tail -1)
        case $scene in
            ent_wall|self_wall) [ "${n:-9999}" -le 20 ] 2>/dev/null && v=ok || { v=FAIL; ERR=1; } ;;
            ent_open) [ "${n:-0}" -ge 2000 ] 2>/dev/null && v=ok || { v=FAIL; ERR=1; } ;;
            hand_wall) [ "${n:-0}" -ge 300 ] 2>/dev/null && v=ok || { v=FAIL; ERR=1; } ;;
        esac
        echo "  $gfx $scene: differing pixels $n -> $v"
        if [ -n "${DEPTH_PNG_OUT:-}" ]; then cp "$W/${gfx}_${scene}_on.png" "${DEPTH_PNG_OUT}_${gfx}_${scene}_on.png" 2>/dev/null; cp "$W/${gfx}_${scene}_off.png" "${DEPTH_PNG_OUT}_${gfx}_${scene}_off.png" 2>/dev/null; fi
    done
done
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-depth: ok" || echo "RESULT mc-depth: FAIL"
exit $ERR
