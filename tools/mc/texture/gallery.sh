#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
OUT=${GALLERY_OUT:?set GALLERY_OUT}
MCV=1.20.1
PROTO=763
export MC_VANILLA_JAR=${MC_VANILLA_JAR:-/root/.w5/jars/client-1.20.1.jar}
export MC_VANILLA_ASSETS=${MC_VANILLA_ASSETS:-/root/.w5/menu-assets}
export MC_VANILLA_INDEX=${MC_VANILLA_INDEX:-5}
export MC_VANILLA=1
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
mkdir -p "$OUT"
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; exit 1; }
python3 -I tools/mc/texture/gallery_cmds.py /root/firn/tools/mcserver/node_modules/minecraft-data/minecraft-data/data/pc/1.20/blocks.json "$W/cmds.txt" "$OUT/map.txt" >"$W/sheets"
SHEETS=$(cat "$W/sheets")
mkdir -p "$W/srv"
MCCLIENT_JAR_CACHE=/root/.w5/jars bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || exit 1
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=creative
level-type=minecraft\:flat
view-distance=4
simulation-distance=4
spawn-protection=0
difficulty=peaceful
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
enable-rcon=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec /usr/bin/java -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 240); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; exit 1; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "randomTickSpeed 0"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
while IFS= read -r line; do echo "$line" >&3; sleep 0.02; done < "$W/cmds.txt"
NSET=$(grep -c '^setblock' "$W/cmds.txt")
for i in $(seq 1 180); do [ "$(grep -c 'Changed the block' "$W/server.log")" -ge $((NSET * 8 / 10)) ] && break; sleep 1; done
sleep 5
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1280x720x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
export FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_FPS=30 MCPLAY_W=1280 MCPLAY_H=720 MCPLAY_CREATIVE=0
printf 'name=Gal\nprotocol=%s\nview=%s\n' "$PROTO" "${GALLERY_VIEW:-4}" > "$W/p.cfg"
PP=""
for att in 1 2 3; do
    env "${@}" MCPLAY_CONFIG="$W/p.cfg" MCPLAY_PROTOCOL=$PROTO MCPLAY_RUN_S=${GALLERY_RUN_S:-300} "$W/mcplay" 127.0.0.1 $PORT Gal > "$OUT/mcplay.out" 2>&1 &
    PP=$!
    for i in $(seq 1 90); do grep -q "Gal joined the game" "$W/server.log" && break; kill -0 $PP 2>/dev/null || break; sleep 1; done
    grep -q "Gal joined the game" "$W/server.log" && break
    kill $PP 2>/dev/null; wait $PP 2>/dev/null
done
WP=""
for i in $(seq 1 30); do WP=$(xdotool search --name mcplay 2>/dev/null | head -1); [ -n "$WP" ] && break; sleep 0.5; done
xdotool windowfocus "$WP" 2>/dev/null; sleep 5
xdotool key --window "$WP" F1; sleep 1
for s in $(seq 0 $((SHEETS - 1))); do
    cam=$(grep "^CAM $s " "$OUT/map.txt")
    set -- $cam
    echo "tp Gal $3.5 $4 $5.5 180 ${GALLERY_PITCH:-47}" >&3
    sleep ${GALLERY_WAIT:-9}
    xdotool mousemove --window "$WP" 5 5; sleep 1
    xwd -id "$WP" -silent -out "$W/s$s.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/s$s.xwd" "$W/s$s.ppm" && python3 -I -c "from PIL import Image; Image.open('$W/s$s.ppm').save('$OUT/sheet$s.png')"
done
echo "stop" >&3
kill $PP 2>/dev/null
wait $PP 2>/dev/null
ls "$OUT"
