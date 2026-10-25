#!/usr/bin/env bash
# tools/mc/play/run_hash.sh [mc_version=1.21.5] -- hashed item stacks (1.21.5+) of the Click Container packet against an official server
# (mc_hash + mc_inv: a stack with six data components and a plain stack, hashed Click Container; docs/MC-VERSIONS.md "Hashed item stacks").
# Prints `RESULT mc-play-hash: ok|FAIL`. Env: RUN_INV_CMDS (extra server commands in the survival stage), MCPLAY_RAW_INV=1 (rawinv lines = real packet bodies for test fixtures), FIRNC, JAVA (1.21+ needs Java 21+; default /root/tools/jdk25/bin/java when present), MC_VIEW (3), MCPLAY_KEEP_DIR.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
MCV=${1:-1.21.5}; VIEW=${MC_VIEW:-3}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
[ "$PROTO" != 0 ] || { echo "unknown version $MCV (set MCPLAY_PROTOCOL)"; exit 2; }
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; sleep 1; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-hash: FAIL"; exit 1; }
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
DN=$(bash tools/mc/pick_display.sh 120 70)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
# ---- a pickaxe with six data components (damage, name, unbreakable, enchantments, rarity, glint) and a plain stack; mcplay picks each up and puts it
# back (Click Container with hashed stacks) and counts the packets the server answers with. A hash it does not recognise makes it re-send the slot:
# run 1 (hashes on) must see 0 re-sends, run 2 (MCPLAY_HASH_OFF=1, empty component hashes) must see some -> the check really measures something.
COMPS='damage=5,custom_name="Hi",enchantments={"minecraft:efficiency":5,"minecraft:unbreaking":2},rarity=epic,enchantment_glint_override=true,unbreakable={}'
# set B: the NBT based components (custom_data with every tag type, lore with a plain and a styled line, a styled custom_name, a plain item_name)
COMPS_B='custom_data={mod:1b,tag:"x",n:[1,2],f:1.5f,d:2.5d,l:7L,s:3s,o:{k:"v"}},lore=["L1",{text:"L2",bold:true,italic:false}],custom_name={text:"N",color:"red",bold:true},item_name="Plain"'
COMPS_CUR=$COMPS
# r217: the first login on a cold server (26.1) can take longer than a fixed sleep; commands sent before the player joined ("No entity was found") leave
# the inventory empty and mcplay never prints a hashtest line. So wait for the join line of the server log instead of a blind sleep.
wait_join() { # $1 = player name
    local i
    for i in $(seq 1 90); do grep -q "$1 joined the game" "$W/server.log" 2>/dev/null && return 0; sleep 1; done
    echo "  WARN  $1 did not join within 90 s"; return 1
}
run_hash() { # $1 = player name, $2 = HASH_OFF, $3 = output file
    MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_HASH_TEST=1 MCPLAY_HASH_OFF=$2 MCPLAY_RUN_S=${HASH_RUN_S:-34} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
        timeout 190 "$W/mcplay" 127.0.0.1 "$PORT" "$1" > "$3" 2>&1 &
    PP=$!
    wait_join "$1"
    sleep 3
    echo "gamemode survival $1" >&3; echo "give $1 diamond_pickaxe[$COMPS_CUR] 1" >&3; echo "give $1 stone 32" >&3
    sleep 1
    echo "data get entity $1 Inventory" >&3
    wait $PP
}
run_hash HashOn 0 "$W/h_on.out"
run_hash HashOff 1 "$W/h_off.out"
# set C: food (can_always_eat), tool (tag rule + 2 block ids + 1 block id, speeds, drops, non-default values), attribute_modifiers (mainhand add_value, any add_multiplied_base,
# the LAST registry ids to prove the attribute table order)
COMPS_C='food={nutrition:4,saturation:2.5f,can_always_eat:true},tool={rules:[{blocks:"#minecraft:mineable/pickaxe",speed:8f,correct_for_drops:true},{blocks:["minecraft:dirt","minecraft:sand"],speed:2f},{blocks:"minecraft:stone"}],default_mining_speed:1.5f,damage_per_block:2,can_destroy_blocks_in_creative:false},attribute_modifiers=[{type:"minecraft:attack_damage",slot:"mainhand",id:"test:a",amount:7.5,operation:"add_value"},{type:"minecraft:water_movement_efficiency",id:"test:b",amount:0.25,operation:"add_multiplied_base"}]'
# set D: the omitted defaults (can_always_eat false, default speed / damage / creative flag, slot any, no speed), other slots and operations
ATTR_D='{type:"minecraft:max_health",slot:"head",id:"test:c",amount:-2.0,operation:"add_multiplied_total"},{type:"minecraft:armor",id:"test:d",amount:1.0,operation:"add_value"}'
case "$PROTO" in 770) ;; *) ATTR_D="$ATTR_D"',{type:"minecraft:armor_toughness",id:"test:e",amount:3.0,operation:"add_value",display:{type:"hidden"}}';; esac
COMPS_D='food={nutrition:1,saturation:0f},tool={rules:[{blocks:"minecraft:dirt"}],default_mining_speed:1f,damage_per_block:1},attribute_modifiers=['"$ATTR_D"']'
# set E (r207): NBT byte / int / long arrays in custom_data, an EMPTY block list in a tool rule, and (1.21.6+) an attribute display override with a styled text
ATTR_E='{type:"minecraft:attack_damage",slot:"mainhand",id:"test:o",amount:2.5,operation:"add_value"}'
case "$PROTO" in 770) ;; *) ATTR_E='{type:"minecraft:attack_damage",slot:"mainhand",id:"test:o",amount:2.5,operation:"add_value",display:{type:"override",value:{text:"Over",bold:true,color:"gold"}}},{type:"minecraft:armor",id:"test:p",amount:1.0,operation:"add_value",display:{type:"override",value:"Plain"}}';; esac
COMPS_E='custom_data={ba:[B;1b,-2b,3b],ia:[I;1,-2,300000],la:[L;1L,-2L],eb:[B;],o:{k:[I;7]}},tool={rules:[{blocks:[],speed:3f},{blocks:["minecraft:dirt"]}]},attribute_modifiers=['"$ATTR_E"']'
# set F (r225): a tool with 10 rules, one of them with 10 block ids (the old limit was 8 / 8)
COMPS_F='tool={rules:[{blocks:"#minecraft:mineable/pickaxe",speed:8f,correct_for_drops:true},{blocks:"#minecraft:mineable/axe",speed:6f},{blocks:"#minecraft:mineable/shovel",speed:5f},{blocks:"#minecraft:mineable/hoe",speed:4f},{blocks:"#minecraft:leaves",speed:3f},{blocks:"#minecraft:wool",speed:2f},{blocks:"#minecraft:logs",speed:1.5f},{blocks:"#minecraft:planks",speed:7f},{blocks:"#minecraft:sand",speed:9f},{blocks:["minecraft:dirt","minecraft:sand","minecraft:gravel","minecraft:stone","minecraft:cobblestone","minecraft:granite","minecraft:diorite","minecraft:andesite","minecraft:clay","minecraft:glass"],speed:2f,correct_for_drops:true}],default_mining_speed:1.25f}'
for SET in B C D E F; do
    case $SET in B) COMPS_CUR=$COMPS_B; LBL="NBT comps (custom_data / lore / styled names)";; C) COMPS_CUR=$COMPS_C; LBL="food / tool / attribute_modifiers";; D) COMPS_CUR=$COMPS_D; LBL="omitted defaults (food / tool / attribute_modifiers)";; E) COMPS_CUR=$COMPS_E; LBL="NBT arrays / empty block list / display override";; F) COMPS_CUR=$COMPS_F; LBL="tool with 10 rules / 10 block ids";; esac
    run_hash "Hash$SET" 0 "$W/h_$SET.out"
    L3=$(grep 'mcplay: hashtest' "$W/h_$SET.out" | head -1)
    echo "  set $SET    : $L3"
    case "$L3" in *"comp_pick=0 comp_put=0 plain_pick=0 plain_put=0"*) ;; *) echo "  FAIL  set $SET $LBL: the server re-sent slots"; ERR=1;; esac
    if [ $SET != B ]; then # control: the same stack without hashes must be re-sent (the new components really are on the stack)
        run_hash "HashOff$SET" 1 "$W/h_off$SET.out"
        L4=$(grep 'mcplay: hashtest' "$W/h_off$SET.out" | head -1)
        case "$L4" in *"comp_pick=0 comp_put=0 "*|"") echo "  FAIL  set $SET control: no re-send without hashes ($L4)"; ERR=1;; esac
    fi
done
L1=$(grep 'mcplay: hashtest' "$W/h_on.out" | head -1); L2=$(grep 'mcplay: hashtest' "$W/h_off.out" | head -1)
echo "  hashes on : $L1"; echo "  hashes off: $L2"
[ -n "$L1" ] || { echo "  FAIL  no hashtest line (hashes on)"; ERR=1; }
case "$L1" in *"comp_pick=0 comp_put=0 plain_pick=0 plain_put=0"*) ;; *) echo "  FAIL  hashed stacks: the server re-sent slots"; ERR=1;; esac
case "$L2" in *"comp_pick=0 comp_put=0 "*) echo "  FAIL  control: no re-send without hashes (the check measures nothing)"; ERR=1;; esac
BAD=$(grep -o ' bad=[0-9]*' "$W/h_on.out" | head -1 | cut -d= -f2)
[ "${BAD:-1}" -eq 0 ] || { echo "  FAIL  inventory bad=${BAD:-?}"; ERR=1; }
grep -qiE "invalid|disconnect|kicked" "$W/server.log" && grep -iE "invalid|disconnect|kicked" "$W/server.log" | head -3
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-hash: ok" || echo "RESULT mc-play-hash: FAIL"
exit $ERR
