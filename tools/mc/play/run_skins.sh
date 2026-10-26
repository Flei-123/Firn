#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "${MCPLAY_KEEP_DIR:-}" ] && { mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; }; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-skins: FAIL"; exit 1; }
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
difficulty=easy
spawn-monsters=true
spawn-animals=true
spawn-npcs=true
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
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
wait_join() { for i in $(seq 1 180); do grep -q "$1 joined the game" "$W/server.log" && { sleep 2; return 0; }; sleep 0.5; done; return 1; }
shot() {
    local nm=$1 skins=$2
    MCPLAY_ENTITY_SKINS=$skins MCPLAY_GL=${SKIN_GL:-0} MCPLAY_VSYNC=0 MCPLAY_AUDIO=0 MCPLAY_RUN_S=16 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=960 MCPLAY_H=540 MCPLAY_SHOT="$W/$nm.png" \
        timeout 180 "$W/mcplay" 127.0.0.1 "$PORT" "P_$nm" > "$W/$nm.out" 2>&1 &
    local pid=$!
    wait_join "P_$nm" || { echo "  FAIL  $nm never joined"; ERR=1; }
    for cmd in "fill -20 119 -20 20 120 20 stone" "fill -20 121 -20 20 126 20 air" "fill -20 127 -20 20 127 20 stone" "tp P_$nm 0.5 121 0.5 0 0"; do echo "$cmd" >&3; done
    sleep 1
    wait $pid; local r=$?
    [ $r -eq 0 ] || { echo "  FAIL  $nm run exit code $r"; ERR=1; }
    echo "  $nm: $(grep -o 'hash=[0-9a-f]*' "$W/$nm.out" | head -1) $(grep -o 'entities_draw boxes=[0-9]*' "$W/$nm.out" | head -1)"
}
echo "kill @e[type=!player]" >&3
sleep 1
i=0
for mob in zombie skeleton creeper pig cow sheep chicken villager spider; do
    x=$(python3 -c "print(-6.0 + $i * 1.5)")
    echo "summon minecraft:$mob $x 121 7.5 {NoAI:1b,PersistenceRequired:1b,Invulnerable:1b,Silent:1b,Rotation:[180f,0f]}" >&3
    i=$((i+1))
done
sleep 2
shot skins_on 1
shot skins_off 0
cat > "$W/cmp.py" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("RGB")).astype(int)
b = np.asarray(Image.open(sys.argv[2]).convert("RGB")).astype(int)
a[0:60, 0:200] = 0
b[0:60, 0:200] = 0
d = np.abs(a - b).sum(axis=2) > 30
print(int(d.sum()))
PY
CD=$(python3 -I "$W/cmp.py" "$W/skins_on.png" "$W/skins_off.png" 2>&1 | tail -1)
echo "  skins on-vs-off differing pixels: $CD"
[ "${CD:-0}" -ge 3000 ] 2>/dev/null || { echo "  FAIL  skinned mobs not visible (${CD:-?} differing pixels < 3000)"; ERR=1; }
if [ -n "${SKINS_PNG_OUT:-}" ]; then cp "$W/skins_on.png" "${SKINS_PNG_OUT}_on.png"; cp "$W/skins_off.png" "${SKINS_PNG_OUT}_off.png"; fi
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-skins: ok" || echo "RESULT mc-skins: FAIL"
exit $ERR
