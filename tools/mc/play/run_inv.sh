#!/usr/bin/env bash
# tools/mc/play/run_inv.sh [mc_version=1.21.4] -- mcplay inventory + survival dig against an official server of ANY version
# (data components from 1.20.5: mc_inv reads Window Items / Set Slot through the packet schema, mc_dig uses the version's tool table).
# One server, Xvfb, mcplay (CPU raster): /give an iron pickaxe + stone, look down, hold the left button.
# Prints `RESULT mc-play-inv: ok|FAIL`. Env: RUN_INV_CMDS (extra server commands in the survival stage), MCPLAY_RAW_INV=1 (rawinv lines = real packet bodies for test fixtures), FIRNC, JAVA (1.21+ needs Java 21+; default /root/tools/jdk25/bin/java when present), MC_VIEW (3), MCPLAY_KEEP_DIR, RUN_INV_ONLY=cont (only the anvil/enchanting stages, needs RUN_INV_CONT=1).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
MCV=${1:-1.21.4}; VIEW=${MC_VIEW:-3}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
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
DN=$(bash tools/mc/pick_display.sh 120 70)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
if [ "${RUN_INV_ONLY:-}" != cont ]; then
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
# ---- efficiency + haste + inventory clicks: enchanted pickaxe, Haste III, Click Container script (MCPLAY_CLICK_TEST) checked against the server's inventory
case "$PROTO" in 763) ENCH='{Enchantments:[{id:"minecraft:efficiency",lvl:5}]}';; 767|769) ENCH='[enchantments={levels:{"minecraft:efficiency":5}}]';; *) ENCH='[enchantments={"minecraft:efficiency":5}]';; esac
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CLICK_TEST=1 MCPLAY_RUN_S=24 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player8 > "$W/fx.out" 2>&1 &
PP=$!
sleep 5
echo "gamemode survival Player8" >&3; echo "give Player8 diamond_pickaxe$ENCH 1" >&3; echo "give Player8 stone 32" >&3; echo "effect give Player8 minecraft:haste 300 2" >&3
sleep 8
echo "data get entity Player8 Inventory" >&3
sleep 2
xdotool mousemove 320 180 2>/dev/null; xdotool mousemove_relative -- 0 300; xdotool key 1; sleep 0.5
xdotool mousedown 1; sleep 4; xdotool mouseup 1; sleep 1
wait $PP
FXL=$(grep 'mcplay: fx ' "$W/fx.out" | head -1); CTL=$(grep 'mcplay: clicktest' "$W/fx.out" | head -1)
echo "  $FXL"; echo "  $CTL"
case "$FXL" in *"haste=3"*"eff=5"*"eff0=5"*) ;; *) echo "  FAIL  fx: expected haste=3 eff=5"; ERR=1;; esac
FT=$(echo "$FXL" | grep -o 'fx_ticks=[-0-9]*' | cut -d= -f2); PT=$(echo "$FXL" | grep -o 'plain_ticks=[-0-9]*' | cut -d= -f2)
[ "${FT:--9}" != "-9" ] && [ "${FT:--1}" -ge 0 ] && [ "${FT}" -le "${PT:-0}" ] || { echo "  FAIL  fx: break ticks ${FT:-?} not <= plain ${PT:-?}"; ERR=1; }
case "$CTL" in *"slot10=1x15"*"slot11=1x16"*) ;; *) case "$CTL" in *"x15"*"x16"*) ;; *) echo "  FAIL  clicks: local result $CTL"; ERR=1;; esac;; esac
INVL=$(grep 'Player8 has the following entity data' "$W/server.log" | tail -1)
python3 - "$INVL" <<'PY' || ERR=1
import re,sys
t=sys.argv[1]
def cnt(slot):
    for m in re.finditer(r'\{[^{}]*Slot: %db[^{}]*\}' % slot, t):
        c=re.search(r'(?:Count: |count: )(\d+)', m.group(0))
        if c and 'stone' in m.group(0): return int(c.group(1))
    return 0
a,b=cnt(10),cnt(11)
print("  server inventory: slot10=%d slot11=%d" % (a,b))
sys.exit(0 if (a,b)==(15,16) else 1)
PY
# ---- shift click + drag + double click (MCPLAY_CLICK_TEST2): 64 + 40 stone and 10 dirt given; the prediction must end up exactly where the server is
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CLICK_TEST2=1 MCPLAY_RUN_S=22 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player9 > "$W/ct2.out" 2>&1 &
PP=$!
sleep 5
echo "gamemode survival Player9" >&3; echo "give Player9 stone 64" >&3; echo "give Player9 stone 40" >&3; echo "give Player9 dirt 10" >&3
sleep 12
echo "data get entity Player9 Inventory" >&3
sleep 2
wait $PP
C2L=$(grep 'mcplay: clicktest2' "$W/ct2.out" | head -1)
echo "  $C2L"
C2BAD=$(grep -o ' bad=[0-9]*' "$W/ct2.out" | head -1 | cut -d= -f2)
[ "${C2BAD:-1}" -eq 0 ] || { echo "  FAIL  clicks2: inv bad=${C2BAD:-?}"; ERR=1; }
case "$C2L" in *"x40"*"x10"*"x64"*"s12=0"*"s15=0"*"cursor=-1x0"*) ;; *) echo "  FAIL  clicks2: local result $C2L"; ERR=1;; esac
INV2=$(grep 'Player9 has the following entity data' "$W/server.log" | tail -1)
python3 - "$INV2" <<'PY' || ERR=1
import re,sys
t=sys.argv[1]
def slot(n):
    for m in re.finditer(r'\{[^{}]*Slot: %db[^{}]*\}' % n, t):
        c=re.search(r'(?:Count: |count: )(\d+)', m.group(0))
        i=re.search(r'(?:id: |id:)"minecraft:(\w+)"', m.group(0))
        if c and i: return (i.group(1), int(c.group(1)))
    return None
got={n: slot(n) for n in (9,10,11,12,13,14,15,36,37,38)}
print("  server inventory: " + " ".join("%d=%s" % (n, got[n]) for n in sorted(got)))
want={9:("stone",40),10:("dirt",10),11:("stone",64)}
ok=all(got[n]==want[n] for n in want) and all(got[n] is None for n in (12,13,14,15,36,37,38))
sys.exit(0 if ok else 1)
PY
# ---- craft prediction (MCPLAY_CRAFT_TEST, r158): 20 planks spread over crafting grid slots 1 and 3 with "one each" drags -> stick x4 is predicted, take the result,
# shift click it twice more; the client's result slot, grid and inventory must equal the server's (slot 20 = 12 sticks, source slot = 14 planks, grid empty)
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CRAFT_TEST=1 MCPLAY_RUN_S=22 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player12 > "$W/ct4.out" 2>&1 &
PP=$!
sleep 5
echo "gamemode survival Player12" >&3; echo "give Player12 oak_planks 20" >&3
sleep 13
echo "data get entity Player12 Inventory" >&3
sleep 2
wait $PP
C4P=$(grep 'mcplay: craftpre' "$W/ct4.out" | head -1)
C4L=$(grep 'mcplay: crafttest' "$W/ct4.out" | head -1)
echo "  $C4P"
echo "  $C4L"
C4BAD=$(grep -o ' bad=[0-9]*' "$W/ct4.out" | head -1 | cut -d= -f2)
[ "${C4BAD:-1}" -eq 0 ] || { echo "  FAIL  craft: inv bad=${C4BAD:-?}"; ERR=1; }
case "$C4P" in *"x4 g1=3 g3=3 "*) ;; *) echo "  FAIL  craft: predicted result after 3 drags $C4P"; ERR=1;; esac
case "$C4L" in *"res=-1x0 "*"s20="*"x12 "*"x14 g1=0 g3=0 cursor=0 "*) ;; *) echo "  FAIL  craft: local result $C4L"; ERR=1;; esac
# up to 1.20.2 the server stays silent when the prediction is exact (srv=0); from 1.21.x it re-sends the result slot after every grid change by itself (srv is info only there)
if [ "$PROTO" -le 764 ]; then case "$C4L" in *" srv=0 "*) ;; *) echo "  FAIL  craft: the server had to correct the prediction ($C4L)"; ERR=1;; esac; fi
INV4=$(grep 'Player12 has the following entity data' "$W/server.log" | tail -1)
python3 - "$INV4" <<'PY' || ERR=1
import re,sys
t=sys.argv[1]
tot={}
at20=None
for m in re.finditer(r'\{[^{}]*Slot: (\d+)b[^{}]*\}', t):
    c=re.search(r'(?:Count: |count: )(\d+)', m.group(0))
    i=re.search(r'(?:id: |id:)"minecraft:(\w+)"', m.group(0))
    if c and i:
        tot[i.group(1)]=tot.get(i.group(1),0)+int(c.group(1))
        if int(m.group(1))==20: at20=(i.group(1),int(c.group(1)))
print("  server inventory: totals=%s slot20=%s" % (tot, at20))
sys.exit(0 if tot.get("stick")==12 and tot.get("oak_planks")==14 and at20==("stick",12) else 1)
PY
# ---- crafting table 3x3 (MCPLAY_TABLE_TEST, r158 round 2): table next to the player; 12 planks -> 6 in grid slots 1 and 4 -> stick x4, RIGHT click takes it, shift click crafts
# the rest (12 sticks, 6 planks left); then a cake (3 milk buckets, 2 sugar, egg, 3 wheat): right click on the result, the 3 buckets stay in the grid. After the window closes
# the server returns the grid, so its inventory must hold stick 12, oak_planks 6, cake 1, bucket 3 and no milk / sugar / egg / wheat.
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_TABLE_TEST=1 MCPLAY_RUN_S=34 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player13 > "$W/tt.out" 2>&1 &
PP=$!
sleep 5
echo "gamemode survival Player13" >&3
for g in "oak_planks 12" "milk_bucket 3" "sugar 2" "egg 1" "wheat 3"; do echo "give Player13 $g" >&3; done
echo "execute at Player13 run setblock ~1 ~ ~ minecraft:crafting_table" >&3
sleep 24
echo "data get entity Player13 Inventory" >&3
sleep 2
wait $PP
TTO=$(grep 'mcplay: tabletest open' "$W/tt.out" | head -1)
TTP=$(grep 'mcplay: tablepre ' "$W/tt.out" | head -1)
TTP2=$(grep 'mcplay: tablepre2' "$W/tt.out" | head -1)
TTL=$(grep 'mcplay: tabletest res=' "$W/tt.out" | head -1)
echo "  $TTO"; echo "  $TTP"; echo "  $TTP2"; echo "  $TTL"
TTBAD=$(grep -o ' bad=[0-9]*' "$W/tt.out" | head -1 | cut -d= -f2)
[ "${TTBAD:-1}" -eq 0 ] || { echo "  FAIL  table: inv bad=${TTBAD:-?}"; ERR=1; }
case "$TTP" in *"x4 "*"g1=3 g4=3 cursor=0"*) ;; *) echo "  FAIL  table: predicted stick result $TTP"; ERR=1;; esac
case "$TTP2" in *"res="*"x1 "*) ;; *) echo "  FAIL  table: no cake predicted $TTP2"; ERR=1;; esac
case "$TTL" in *"res=-1x0 stick=12 planks=6 cake=1 bucket=3 milk=0 sugar=0 egg=0 wheat=0 "*"cursor=0 "*) ;; *) echo "  FAIL  table: local result $TTL"; ERR=1;; esac
# up to 1.20.2 the server stays silent when the prediction is exact; from 1.21.x it re-sends slots after every change (srv is info only there)
if [ "$PROTO" -le 764 ]; then case "$TTL" in *" srv=0") ;; *) echo "  FAIL  table: the server had to correct the prediction ($TTL)"; ERR=1;; esac; fi
INV5=$(grep 'Player13 has the following entity data' "$W/server.log" | tail -1)
python3 - "$INV5" <<'PY' || ERR=1
import re,sys
t=sys.argv[1]
tot={}
for m in re.finditer(r'\{[^{}]*Slot: (\d+)b[^{}]*\}', t):
    c=re.search(r'(?:Count: |count: )(\d+)', m.group(0))
    i=re.search(r'(?:id: |id:)"minecraft:(\w+)"', m.group(0))
    if c and i: tot[i.group(1)]=tot.get(i.group(1),0)+int(c.group(1))
print("  server inventory (table): totals=%s" % tot)
want={"stick":12,"oak_planks":6,"cake":1,"bucket":3}
ok=all(tot.get(k)==v for k,v in want.items()) and not any(k in tot for k in ("milk_bucket","sugar","egg","wheat"))
sys.exit(0 if ok else 1)
PY
# ---- crafting table 3x3, DROP / SWAP on the result and a remainder from an ingredient STACK (MCPLAY_TABLE_TEST2, r234): 2 planks -> stick x4, Q drops it; 8 honey bottles
# (2 in each of 4 grid slots) -> honey block: left click (4 glass bottles into the inventory), hotbar swap (single bottles: they stay in the grid). After the window closes the
# server returns the grid: honey_block 2, glass_bottle 8, no honey / planks; the dropped sticks may have been picked up again (0 or 4).
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_TABLE_TEST2=1 MCPLAY_RUN_S=30 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player14 > "$W/tt2.out" 2>&1 &
PP=$!
sleep 5
echo "gamemode survival Player14" >&3
for g in "honey_bottle 8" "oak_planks 2"; do echo "give Player14 $g" >&3; done
echo "execute at Player14 run setblock ~1 ~ ~ minecraft:crafting_table" >&3
sleep 21
echo "data get entity Player14 Inventory" >&3
sleep 2
wait $PP
T2O=$(grep 'mcplay: tabletest2 open' "$W/tt2.out" | head -1)
T2P=$(grep 'mcplay: tablepre3' "$W/tt2.out" | head -1)
T2T=$(grep 'mcplay: tablethrow' "$W/tt2.out" | head -1)
T2Q=$(grep 'mcplay: tablepre4' "$W/tt2.out" | head -1)
T2L=$(grep 'mcplay: tabletest2 res=' "$W/tt2.out" | head -1)
echo "  $T2O"; echo "  $T2P"; echo "  $T2T"; echo "  $T2Q"; echo "  $T2L"
T2BAD=$(grep -o ' bad=[0-9]*' "$W/tt2.out" | head -1 | cut -d= -f2)
[ "${T2BAD:-1}" -eq 0 ] || { echo "  FAIL  table2: inv bad=${T2BAD:-?}"; ERR=1; }
case "$T2P" in *"x4 "*"cursor=0"*) ;; *) echo "  FAIL  table2: stick result not predicted $T2P"; ERR=1;; esac
case "$T2T" in *"res=-1x0 stick=0 planks=0 cursor=-1"*) ;; *) echo "  FAIL  table2: Q on the result $T2T"; ERR=1;; esac
case "$T2Q" in *"x1 "*"g1=2 g5=2 cursor=0"*) ;; *) echo "  FAIL  table2: honey block not predicted $T2Q"; ERR=1;; esac
case "$T2L" in *"res=-1x0 hblock=2 gbottle=8 honey=0 stick=0 planks=0 "*"cursor=-1 "*) ;; *) echo "  FAIL  table2: local result $T2L"; ERR=1;; esac
if [ "$PROTO" -le 764 ]; then case "$T2L" in *" srv=0") ;; *) echo "  FAIL  table2: the server had to correct the prediction ($T2L)"; ERR=1;; esac; fi
INV6=$(grep 'Player14 has the following entity data' "$W/server.log" | tail -1)
python3 - "$INV6" <<'PY' || ERR=1
import re,sys
t=sys.argv[1]
tot={}
for m in re.finditer(r'\{[^{}]*Slot: (\d+)b[^{}]*\}', t):
    c=re.search(r'(?:Count: |count: )(\d+)', m.group(0))
    i=re.search(r'(?:id: |id:)"minecraft:(\w+)"', m.group(0))
    if c and i: tot[i.group(1)]=tot.get(i.group(1),0)+int(c.group(1))
print("  server inventory (table2): totals=%s" % tot)
ok=tot.get("honey_block")==2 and tot.get("glass_bottle")==8 and "honey_bottle" not in tot and "oak_planks" not in tot and tot.get("stick",0) in (0,4)
sys.exit(0 if ok else 1)
PY
# ---- chest window (MCPLAY_CHEST_TEST): a chest next to the player (10 stone, 6 dirt, 64 stone); Open Window -> Window Items -> shift click both ways, pick up,
# drag over four container slots, close; the chest content and the player's hotbar on the server must equal the client's prediction
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CHEST_TEST=1 MCPLAY_RUN_S=28 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player10 > "$W/ct3.out" 2>&1 &
PP=$!
sleep 5
echo "kill @e[type=minecraft:item]" >&3
echo "gamemode survival Player10" >&3; echo "give Player10 stone 20" >&3; echo "give Player10 dirt 5" >&3
echo "execute at Player10 run setblock ~1 ~ ~ minecraft:chest" >&3
sleep 1
echo "execute at Player10 run item replace block ~1 ~ ~ container.0 with minecraft:stone 10" >&3
echo "execute at Player10 run item replace block ~1 ~ ~ container.1 with minecraft:dirt 6" >&3
echo "execute at Player10 run item replace block ~1 ~ ~ container.26 with minecraft:stone 64" >&3
sleep 17
echo "execute at Player10 run data get block ~1 ~ ~ Items" >&3
echo "data get entity Player10 Inventory" >&3
sleep 2
wait $PP
C3L=$(grep 'mcplay: chesttest c1=' "$W/ct3.out" | head -1)
echo "  $(grep 'mcplay: chesttest open' "$W/ct3.out" | head -1)"; echo "  $C3L"
C3BAD=$(grep -o ' bad=[0-9]*' "$W/ct3.out" | head -1 | cut -d= -f2)
[ "${C3BAD:-1}" -eq 0 ] || { echo "  FAIL  chest: inv bad=${C3BAD:-?}"; ERR=1; }
case "$C3L" in *"c1="*"x11"*"c5="*"x64"*"c10=7 c13=7"*"cursor=-1x0"*) ;; *) echo "  FAIL  chest: local result $C3L"; ERR=1;; esac
CH=$(grep 'has the following block data' "$W/server.log" | tail -1); PI=$(grep 'Player10 has the following entity data' "$W/server.log" | tail -1)
python3 - "$CH" "$PI" <<'PY' || ERR=1
import re,sys
def slot(t, n):
    for m in re.finditer(r'\{[^{}]*Slot: %db[^{}]*\}' % n, t):
        c=re.search(r'(?:Count: |count: )(\d+)', m.group(0))
        i=re.search(r'(?:id: |id:)"minecraft:(\w+)"', m.group(0))
        if c and i: return (i.group(1), int(c.group(1)))
    return None
ch, pi = sys.argv[1], sys.argv[2]
got={n: slot(ch, n) for n in (0,1,5,10,11,12,13,26)}
hot={n: slot(pi, n) for n in (0,1)}
print("  server chest: " + " ".join("%d=%s" % (n, got[n]) for n in sorted(got)) + " | hotbar: " + " ".join("%d=%s" % (n, hot[n]) for n in sorted(hot)))
want={1:("dirt",11),5:("stone",64),10:("stone",7),11:("stone",7),12:("stone",7),13:("stone",7)}
ok=all(got[n]==want[n] for n in want) and got[0] is None and got[26] is None and hot[0]==("stone",2) and hot[1] is None
sys.exit(0 if ok else 1)
PY
# ---- chest UI (MCPLAY_CHEST_UI): the real panel. mcplay opens the chest with a right click packet and prints the cell centres; xdotool then clicks:
# shift click container 0 (10 stone) -> hotbar, pick up 64 stone of slot 26 and put it into slot 5, shift click the hotbar dirt -> container slot 1, pick the 64 up again and drag it over slots 6-8 (21 each, 1 back to slot 5), Esc closes
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CHEST_TEST=1 MCPLAY_CHEST_UI=1 MCPLAY_RUN_S=45 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player11 > "$W/ct4.out" 2>&1 &
PP=$!
sleep 5
echo "kill @e[type=minecraft:item]" >&3
echo "execute at Player11 run setblock ~1 ~ ~ minecraft:air" >&3
echo "gamemode survival Player11" >&3; echo "give Player11 stone 20" >&3; echo "give Player11 dirt 5" >&3
echo "execute at Player11 run setblock ~1 ~ ~ minecraft:chest" >&3
sleep 1
echo "execute at Player11 run item replace block ~1 ~ ~ container.0 with minecraft:stone 10" >&3
echo "execute at Player11 run item replace block ~1 ~ ~ container.1 with minecraft:dirt 6" >&3
echo "execute at Player11 run item replace block ~1 ~ ~ container.26 with minecraft:stone 64" >&3
for i in $(seq 1 40); do grep -q 'chestui cell ws=55 ' "$W/ct4.out" 2>/dev/null && break; sleep 1; done
cell() { grep "chestui cell ws=$1 " "$W/ct4.out" | head -1 | sed -n 's/.* x=\([0-9]*\) y=\([0-9]*\).*/\1 \2/p'; }
uiclick() { local xy; xy=$(cell "$1"); [ -n "$xy" ] || { echo "  FAIL  chest ui: no cell $1"; ERR=1; return; }; xdotool mousemove $xy; sleep 0.4; xdotool mousedown 1; sleep 0.25; xdotool mouseup 1; sleep 0.6; }
uishift() { xdotool keydown Shift_L; sleep 0.3; uiclick "$1"; xdotool keyup Shift_L; sleep 0.4; }
if grep -q 'chestui cell ws=55 ' "$W/ct4.out"; then
    sleep 1
    uishift 0; uiclick 26; uiclick 5; uishift 55; sleep 1
    uiclick 5
    xy6=$(cell 6); xy7=$(cell 7); xy8=$(cell 8)
    xdotool mousemove $xy6; sleep 0.4; xdotool mousedown 1; sleep 0.3; xdotool mousemove $xy7; sleep 0.3; xdotool mousemove $xy8; sleep 0.3; xdotool mouseup 1; sleep 0.8
    uiclick 5
    xdotool key Escape; sleep 1.5
    echo "execute at Player11 run data get block ~1 ~ ~ Items" >&3
    echo "data get entity Player11 Inventory" >&3
    sleep 2
else
    echo "  FAIL  chest ui: chest did not open"; ERR=1
fi
wait $PP
C4S=$(grep 'mcplay: chestui state' "$W/ct4.out" | head -1); C4E=$(grep 'mcplay: chestui end' "$W/ct4.out" | head -1)
echo "  $C4S"; echo "  $C4E"
case "$C4S" in *"c0=-1"*"c1="*"x11"*"c5="*"x1 "*"c26=-1"*"h0="*"x30"*"cursor=-1x0"*) ;; *) echo "  FAIL  chest ui: local state $C4S"; ERR=1;; esac
case "$C4E" in *"show_inv=false"*"cont_ui=false"*"win=0"*"grab=true"*"bad=0"*) ;; *) echo "  FAIL  chest ui: panel did not close / mouse not captured again: $C4E"; ERR=1;; esac
CH=$(grep 'has the following block data' "$W/server.log" | tail -1); PI=$(grep 'Player11 has the following entity data' "$W/server.log" | tail -1)
python3 - "$CH" "$PI" <<'PY' || ERR=1
import re,sys
def slot(t, n):
    for m in re.finditer(r'\{[^{}]*Slot: %db[^{}]*\}' % n, t):
        c=re.search(r'(?:Count: |count: )(\d+)', m.group(0))
        i=re.search(r'(?:id: |id:)"minecraft:(\w+)"', m.group(0))
        if c and i: return (i.group(1), int(c.group(1)))
    return None
ch, pi = sys.argv[1], sys.argv[2]
got={n: slot(ch, n) for n in (0,1,5,6,7,8,26)}
hot={n: slot(pi, n) for n in (0,1)}
print("  server chest (ui): " + " ".join("%d=%s" % (n, got[n]) for n in sorted(got)) + " | hotbar: " + " ".join("%d=%s" % (n, hot[n]) for n in sorted(hot)))
ok=got[0] is None and got[26] is None and got[1]==("dirt",11) and got[5]==("stone",1) and got[6]==("stone",21) and got[7]==("stone",21) and got[8]==("stone",21) and hot[0]==("stone",30) and hot[1] is None
sys.exit(0 if ok else 1)
PY
fi
# ---- anvil + enchanting table UI (MCPLAY_CONT_UI=1|2): the block is set next to the player, mcplay opens it with a right click packet and prints the cell centres.
# anvil: pickaxe into the input slot, type "Bo e1" into the name field (E and the digit must not close the panel / swap), the server's result slot must fill (rename accepted).
# enchanting table: pickaxe + 3 lapis in, click the first option; the server must enchant the pickaxe and take one lapis
if [ "${RUN_INV_CONT:-0}" = 1 ]; then
CJD="$W/cj"; CJAR=""
MCCLIENT_JAR_CACHE=${MCCLIENT_JAR_CACHE:-/root/.w5/jars} bash tools/mcclient/fetch_client.sh "$CJD" "$MCV" >/dev/null 2>&1 && CJAR="$CJD/client.jar"
CJENV=(MC_VANILLA=1 "MC_VANILLA_JAR=$CJAR" "MC_VANILLA_ASSETS=${MC_VANILLA_ASSETS:-/root/.w5/menu-assets}" MC_VANILLA_INDEX=5)
[ -n "$CJAR" ] || { echo "  FAIL  cont ui: no client jar for $MCV"; ERR=1; }
ccell() { grep "contui cell $2 " "$W/$1.out" | head -1 | sed -n 's/.* x=\([0-9]*\) y=\([0-9]*\).*/\1 \2/p'; }
cclick() { local xy; xy=$(ccell "$1" "$2"); [ -n "$xy" ] || { echo "  FAIL  cont ui: no cell $2"; ERR=1; return; }; xdotool mousemove $xy; sleep 0.4; xdotool mousedown 1; sleep 0.25; xdotool mouseup 1; sleep 0.8; }
env "${CJENV[@]}" MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CONT_UI=1 MCPLAY_CONT_LOG=1 MCPLAY_RUN_S=45 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player16 > "$W/an.out" 2>&1 &
PP=$!
sleep 5
echo "kill @e[type=minecraft:item]" >&3
echo "execute at Player16 run setblock ~1 ~ ~ minecraft:air" >&3
echo "gamemode survival Player16" >&3; echo "give Player16 iron_pickaxe 1" >&3; echo "give Player16 stone 4" >&3
echo "execute at Player16 run setblock ~1 ~ ~ minecraft:anvil" >&3
for i in $(seq 1 40); do grep -q 'contui cell hot1 ' "$W/an.out" 2>/dev/null && break; sleep 1; done
if grep -q 'contui cell hot1 ' "$W/an.out"; then
    sleep 1
    cclick an hot0; cclick an ws=0
    sleep 1; xdotool keydown ctrl; sleep 0.3; xdotool key a; sleep 0.3; xdotool keyup ctrl; sleep 0.5
    xdotool type --delay 250 "Bo e1"; sleep 2.5
    xdotool key Escape; sleep 1.5
else
    echo "  FAIL  anvil ui: anvil did not open"; ERR=1
fi
wait $PP
ANS=$(grep 'mcplay: contui state' "$W/an.out" | head -1); ANE=$(grep 'mcplay: contui end' "$W/an.out" | head -1)
echo "  $ANS"; echo "  $ANE"
grep -q 'mcplay: anvil prefill n=12' "$W/an.out" || { echo "  FAIL  anvil ui: name field not prefilled with the item name"; ERR=1; }
case "$ANS" in *"kind=5"*) ;; *) echo "  FAIL  anvil ui: no anvil state: $ANS"; ERR=1;; esac
python3 - "$ANS" <<'PY' || { echo "  FAIL  anvil ui: local state $ANS"; ERR=1; }
import re,sys
t=sys.argv[1]
def num(k):
    m=re.search(k+r'=(-?\d+)', t); return int(m.group(1)) if m else None
w0=re.search(r'w0=(-?\d+)x(\d+)', t); w2=re.search(r'w2=(-?\d+)x(\d+)', t)
ok=bool(w0) and int(w0.group(1))>=0 and int(w0.group(2))==1 and bool(w2) and int(w2.group(1))>=0 and (num('cost') or 0)>0 and num('name_n')==5 and (num('sent_name') or 0)>=5
sys.exit(0 if ok else 1)
PY
case "$ANE" in *"show_inv=false"*"cont_ui=false"*"win=0"*"grab=true"*"bad=0"*) ;; *) echo "  FAIL  anvil ui: panel did not close / mouse not captured again: $ANE"; ERR=1;; esac
env "${CJENV[@]}" MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CONT_UI=2 MCPLAY_CONT_LOG=1 MCPLAY_RUN_S=45 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player17 > "$W/en.out" 2>&1 &
PP=$!
sleep 5
echo "kill @e[type=minecraft:item]" >&3
echo "execute at Player17 run setblock ~1 ~ ~ minecraft:air" >&3
echo "gamemode survival Player17" >&3; echo "xp set Player17 30 levels" >&3; echo "give Player17 iron_pickaxe 1" >&3; echo "give Player17 lapis_lazuli 3" >&3
echo "execute at Player17 run setblock ~1 ~ ~ minecraft:enchanting_table" >&3
for i in $(seq 1 40); do grep -q 'contui cell hot1 ' "$W/en.out" 2>/dev/null && break; sleep 1; done
if grep -q 'contui cell hot1 ' "$W/en.out"; then
    sleep 1
    cclick en hot0; cclick en ws=0; cclick en hot1; cclick en ws=1
    sleep 2
    cclick en opt0; sleep 2
    xdotool key Escape; sleep 1.5
    echo "data get entity Player17 Inventory" >&3
    sleep 2
else
    echo "  FAIL  enchant ui: table did not open"; ERR=1
fi
wait $PP
ENS=$(grep 'mcplay: contui state' "$W/en.out" | head -1); ENE=$(grep 'mcplay: contui end' "$W/en.out" | head -1)
echo "  $ENS"; echo "  $(grep 'mcplay: enchant click' "$W/en.out" | head -2 | tr '\n' ' ')"
case "$ENS" in *"kind=6"*"w1="*"x2 "*"sent_button=1"*) ;; *) echo "  FAIL  enchant ui: local state $ENS"; ERR=1;; esac
case "$ENE" in *"show_inv=false"*"cont_ui=false"*"win=0"*"grab=true"*"bad=0"*) ;; *) echo "  FAIL  enchant ui: panel did not close / mouse not captured again: $ENE"; ERR=1;; esac
EI=$(grep 'Player17 has the following entity data' "$W/server.log" | tail -1)
python3 - "$EI" <<'PY' || ERR=1
import re,sys
t=sys.argv[1]
ench='iron_pickaxe' in t and 'nchantments' in t
lap=re.search(r'\{[^{}]*lapis_lazuli[^{}]*\}', t)
cnt=re.search(r'(?:Count: |count: )(\d+)', lap.group(0)) if lap else None
print("  server: pickaxe enchanted=%s lapis=%s" % (ench, cnt.group(1) if cnt else None))
sys.exit(0 if ench and cnt and int(cnt.group(1))==2 else 1)
PY
fi
if [ "${RUN_INV_ONLY:-}" != cont ]; then
# ---- creative screen (MCPLAY_CREATIVE_LOG): E opens the tabbed item list, xdotool switches tabs, takes stacks from the list and puts them into the hotbar (Set Creative Slot)
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=1 MCPLAY_CREATIVE_LOG=1 MCPLAY_RUN_S=40 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Player15 > "$W/cr.out" 2>&1 &
PP=$!
sleep 6
echo "gamemode survival Player15" >&3; sleep 1; echo "gamemode creative Player15" >&3
echo "clear Player15" >&3
sleep 3
crcell() { grep "crui cell $1 " "$W/cr.out" | head -1 | sed -n 's/.* x=\([0-9]*\) y=\([0-9]*\).*/\1 \2/p'; }
crclick() { local xy; xy=$(crcell "$1"); [ -n "$xy" ] || { echo "  FAIL  creative ui: no cell $1"; ERR=1; return; }; xdotool mousemove $xy; sleep 0.4; xdotool mousedown 1; sleep 0.25; xdotool mouseup 1; sleep 0.5; }
xdotool key e; sleep 1.5
for i in $(seq 1 30); do grep -q 'crui cell delete ' "$W/cr.out" 2>/dev/null && break; sleep 1; done
if grep -q 'crui cell delete ' "$W/cr.out"; then
    crclick tab=2; crclick tab=0
    crclick pick=0; crclick ws=36
    crclick pick=1; crclick ws=37
    xdotool key Escape; sleep 1.5
    echo "data get entity Player15 Inventory" >&3
    sleep 2
else
    echo "  FAIL  creative ui: screen did not open"; ERR=1
fi
wait $PP
CRS=$(grep 'mcplay: crui state' "$W/cr.out" | head -1)
echo "  $CRS"
case "$CRS" in *"tab=0 n="*"h0="*"x1 h1="*"x1 cursor=-1x0"*) ;; *) echo "  FAIL  creative ui: local state $CRS"; ERR=1;; esac
CRN=$(echo "$CRS" | sed -n 's/.* n=\([0-9]*\) .*/\1/p'); [ "${CRN:-0}" -gt 200 ] || { echo "  FAIL  creative ui: building tab list too short ($CRN)"; ERR=1; }
CI=$(grep 'Player15 has the following entity data' "$W/server.log" | tail -1)
python3 - "$CI" <<'PY' || ERR=1
import re,sys
def slot(t, n):
    for m in re.finditer(r'\{[^{}]*Slot: %db[^{}]*\}' % n, t):
        c=re.search(r'(?:Count: |count: )(\d+)', m.group(0))
        i=re.search(r'(?:id: |id:)"minecraft:(\w+)"', m.group(0))
        if c and i: return (i.group(1), int(c.group(1)))
    return None
t=sys.argv[1]
a,b=slot(t,0),slot(t,1)
print("  server creative hotbar: 0=%s 1=%s" % (a,b))
sys.exit(0 if a and b and a[1]==1 and b[1]==1 and a[0]!=b[0] else 1)
PY
fi
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-inv: ok" || echo "RESULT mc-play-inv: FAIL"
exit $ERR
