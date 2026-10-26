#!/usr/bin/env bash
# tools/mc/play/vanref_shot.sh <out_dir> <mcplay_binary> -- same hotbar/inventory scene in real Vanilla 1.20.1 and mcplay, screenshots to <out_dir>/{van,mc}_{hotbar,inv}.png
set -uo pipefail
OUT=$1; BIN=$2
REF=${VANREF:-/root/.w5/vanref}
PORT=${VR_PORT:-25677}; RPORT=$((PORT+1)); DV=${VR_DISPLAY_VAN:-87}; DM=${VR_DISPLAY_MC:-88}
W=$(mktemp -d); mkdir -p "$OUT"
cleanup() { for p in ${PIDS:-}; do kill "$p" 2>/dev/null; done; sleep 1; for p in ${PIDS:-}; do kill -9 "$p" 2>/dev/null; done; rm -rf "$W"; }
trap cleanup EXIT
PIDS=""
cp -r "$REF/srv" "$W/srv"
sed -i "s/^server-port=.*/server-port=$PORT/;s/^rcon.port=.*/rcon.port=$RPORT/;s/^query.port=.*/query.port=$((PORT+2))/" "$W/srv/server.properties"
( cd "$W/srv" && exec java -Xmx1G -jar server.jar nogui >"$W/server.log" 2>&1 ) & PIDS="$PIDS $!"
for i in $(seq 90); do grep -q "Done (" "$W/server.log" 2>/dev/null && break; sleep 1; done
grep -q "Done (" "$W/server.log" || { echo "server did not start"; exit 1; }
rcon() { python3 -I "$REF/tools/rcon.py" $RPORT hb "$@" >/dev/null 2>&1; }
setup_scene() {
    rcon "gamerule sendCommandFeedback false" "gamerule announceAdvancements false" "op HB" "gamemode survival HB" "fill -8 -61 -8 8 -61 8 white_concrete" "tp HB 0 -60 0 0 90" "clear HB"
    local k=0
    while IFS= read -r it; do rcon "item replace entity HB hotbar.$k with $it"; k=$((k+1)); done <<< "$ITEMS"
    rcon "item replace entity HB weapon.offhand with shield{Damage:100}"; rcon "item replace entity HB inventory.0 with diamond_pickaxe{Damage:700}" "item replace entity HB inventory.1 with enchanted_golden_apple 3" "item replace entity HB inventory.2 with iron_sword{Damage:100,Enchantments:[{id:\"sharpness\",lvl:2}]}"
}
shot() { DISPLAY=:$1 xwd -root -silent -out "$W/t.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/t.xwd" "$W/t.ppm" && python3 -I -c "
from PIL import Image
Image.open('$W/t.ppm').save('$2')"; }
ITEMS=${VR_ITEMS:-$'diamond_pickaxe{Damage:200}\nenchanted_book{StoredEnchantments:[{id:"mending",lvl:1}]}\nbow{Damage:300,Enchantments:[{id:"power",lvl:1}]}\nenchanted_golden_apple 5\niron_chestplate{Damage:20}\nstone 64\nwooden_sword{Damage:58}\nshears{Damage:1}\nnether_star'}
NAT="$W/nat"; mkdir -p "$NAT"
for j in $(cd "$REF" && find libs -name "*natives-linux.jar"); do (cd "$NAT" && unzip -qo "$REF/$j" 2>/dev/null); done
CP=$(cat "$REF/cp.txt")
Xvfb :$DV -screen 0 1280x720x24 >/dev/null 2>&1 & PIDS="$PIDS $!"
Xvfb :$DM -screen 0 1280x720x24 >/dev/null 2>&1 & PIDS="$PIDS $!"
sleep 2
( cd "$REF" && DISPLAY=:$DV exec java -Xmx1G -Djava.library.path="$NAT" -Dorg.lwjgl.librarypath="$NAT" -cp "$CP:/root/.w5/jars/client-1.20.1.jar" net.minecraft.client.main.Main --username HB --version 1.20.1 --gameDir "$W/gd" --assetsDir "$REF/assets" --assetIndex 5 --accessToken 0 --uuid 0123456789abcdef0123456789abcdef --userType legacy --width 1280 --height 720 --quickPlayMultiplayer 127.0.0.1:$PORT >"$W/van.log" 2>&1 ) & VP=$!; PIDS="$PIDS $VP"
for i in $(seq 90); do grep -q "joined the game" "$W/srv/logs/latest.log" 2>/dev/null && break; sleep 1; done
sleep 8
setup_scene; sleep 4
shot $DV "$OUT/van_hotbar.png"
DISPLAY=:$DV xdotool key e; sleep 2
shot $DV "$OUT/van_inv.png"
kill $VP; sleep 3
rcon "kick HB"; sleep 2
export MC_VANILLA_JAR=/root/.w5/jars/client-1.20.1.jar MC_VANILLA_ASSETS="$REF/assets" MC_VANILLA_INDEX=5 MC_VANILLA=1 MCPLAY_W=1280 MCPLAY_H=720 MCPLAY_PROTOCOL=763
mkdir -p "$W/cfg"; export MCPLAY_CONFIG="$W/cfg"
DISPLAY=:$DM "$BIN" 127.0.0.1 $PORT HB >"$W/mc.log" 2>&1 & MP=$!; PIDS="$PIDS $MP"
for i in $(seq 60); do [ "$(grep -c "joined the game" "$W/srv/logs/latest.log")" -ge 2 ] && break; sleep 1; done
sleep 8
setup_scene; sleep 4
shot $DM "$OUT/mc_hotbar.png"
DISPLAY=:$DM xdotool key e; sleep 2
shot $DM "$OUT/mc_inv.png"
tail -3 "$W/mc.log"
echo done
