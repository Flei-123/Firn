#!/usr/bin/env bash
# tools/mc/play/run_vk.sh -- mcplay with the Vulkan back end (MCPLAY_GFX=vk, lib/mc/vk_mc.fi) against the REAL 1.20.1 server (Xvfb, offline, creative).
# lavapipe (Mesa CPU Vulkan) = NO real GPU: this proves wiring + picture parity with the CPU raster, NOT GPU speed. SKIP (exit 0) without a Vulkan device.
# Same scene (platform + mobs, fixed camera) four times: CPU raster (reference), VK (swapchain present, r195), VK with a SIMULATED device loss
# (MCPLAY_VK_LOST_AT; the swapchain is re-created after the recovery), VK with MCPLAY_VK_PRESENT=readback (old path).
# Checks: no crash / exit 0, `gfx=vk` line, frame counters (mcplay end frames + vk frames), boxes drawn, PSNR(VK, CPU) >= MCPLAY_VK_PSNR_MIN (34),
# lost run: lost=1 recovered=1 and the same picture afterwards. Swapchain (vk + vklost): `present=swapchain`, presents >= 30, and at exit the X window
# content (XGetImage) == the read back colour image of the last frame (window_diff=0, octets); readback run: `present=readback`. Env: MC_VIEW (4), FIRNC, MCPLAY_VK_PNG_OUT=<prefix>. Start it via heavy.
# Prints `RESULT mc-vk: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libvulkan.so.1 ] || [ -e /usr/lib64/libvulkan.so.1 ] || { echo "  SKIP: no libvulkan.so.1"; echo "RESULT mc-vk: skip"; exit 0; }
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-vk: FAIL"; exit 1; }
for t in Xvfb java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=1.20.1; VIEW=${MC_VIEW:-4}; PMIN=${MCPLAY_VK_PSNR_MIN:-34}
mkdir -p "$W/srv"
bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
# spawn-animals must be true (false: 1.20.1 discards summoned animals within a tick); natural spawning off through the gamerule.
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
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "doMobLoot false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
wait_join() { for i in $(seq 1 180); do grep -q "$1 joined the game" "$W/server.log" && { sleep 2; return 0; }; sleep 0.5; done; return 1; }
for cmd in "fill -20 119 -20 20 120 20 stone" "fill -20 121 -20 20 128 20 air" \
           "summon minecraft:cow 0.5 121 5.5 {NoAI:1b,PersistenceRequired:1b,Tags:[\"ent\"],Rotation:[0f,0f]}" "summon minecraft:pig 2.5 121 6.5 {NoAI:1b,PersistenceRequired:1b,Tags:[\"ent\"],Rotation:[0f,0f]}" \
           "summon minecraft:pig -1.5 121 6.5 {NoAI:1b,PersistenceRequired:1b,Tags:[\"ent\"],Rotation:[0f,0f]}" "summon minecraft:cow 3.5 121 9.5 {NoAI:1b,PersistenceRequired:1b,Tags:[\"ent\"],Rotation:[0f,0f]}"; do echo "$cmd" >&3; done
sleep 2
LOST_AT=${MCPLAY_VK_LOST_AT:-60}
shot() { # <name> <gfx cpu|vk> <lost_at> [extra VAR=value]
    local nm=$1 gfx=$2 la=$3 ex=${4:-}
    env $ex MCPLAY_GFX=$gfx MCPLAY_VK_LOST_AT=$la MCPLAY_AUDIO=0 MCPLAY_RUN_S=12 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/$nm.png" \
        timeout 240 "$W/mcplay" 127.0.0.1 "$PORT" "P_$nm" > "$W/$nm.out" 2>&1 &
    local pid=$!
    wait_join "P_$nm" || { echo "  FAIL  $nm never joined"; ERR=1; }
    echo "tp P_$nm 0.5 121 0.5 0 0" >&3
    sleep 1
    wait $pid; local r=$?
    [ $r -eq 0 ] || { echo "  FAIL  $nm run exit code $r"; tail -5 "$W/$nm.out" | sed 's/^/        /'; ERR=1; }
    echo "  $nm: $(grep -o 'end frames=[0-9]*' "$W/$nm.out" | head -1) $(grep -o 'hash=[0-9a-f]*' "$W/$nm.out" | head -1) $(grep -o 'entities_draw boxes=[0-9]*' "$W/$nm.out" | head -1) $(grep -o 'pos x=[-0-9]* y=[-0-9]* z=[-0-9]*' "$W/$nm.out" | head -1)"
}
shot cpu cpu 0
shot vk vk 0
if grep -q "gfx=cpu (vk requested" "$W/vk.out"; then echo "  SKIP: no Vulkan device (mcplay fell back to the CPU path)"; echo "RESULT mc-vk: skip"; echo stop >&3; exit 0; fi
shot vklost vk "$LOST_AT"
shot vkrb vk 0 MCPLAY_VK_PRESENT=readback
num() { grep "$2" "$1" | grep -o " $3=[0-9]*" | head -1 | cut -d= -f2; }
grep -q "mcplay: gfx=vk device=" "$W/vk.out" || { echo "  FAIL  vk run: no gfx=vk line"; ERR=1; }
grep "mcplay: gfx=vk" "$W/vk.out" | head -1 | sed 's/^/  /'
VF=$(num "$W/vk.out" "mcplay: vk frames" frames); EF=$(num "$W/vk.out" "mcplay: end" frames)
[ "${VF:-0}" -ge 30 ] || { echo "  FAIL  vk frames ${VF:-0} < 30"; ERR=1; }
[ "${EF:-0}" -ge 30 ] || { echo "  FAIL  mcplay frames ${EF:-0} < 30"; ERR=1; }
[ "$(num "$W/vk.out" "mcplay: vk frames" lost)" = 0 ] || { echo "  FAIL  vk run lost a device"; ERR=1; }
[ "$(num "$W/vk.out" "mcplay: vk frames" ubo_over)" = 0 ] || { echo "  FAIL  UBO slices overflowed"; ERR=1; }
BX=$(num "$W/vk.out" "mcplay: entities_draw" boxes)
[ "${BX:-0}" -gt 0 ] || { echo "  FAIL  vk: no entity boxes drawn"; ERR=1; }
LV=$(num "$W/vklost.out" "mcplay: vk frames" lost); LR=$(num "$W/vklost.out" "mcplay: vk frames" recovered); LF=$(num "$W/vklost.out" "mcplay: end" frames)
[ "${LV:-0}" = 1 ] && [ "${LR:-0}" = 1 ] || { echo "  FAIL  lost run: lost=${LV:-?} recovered=${LR:-?} (expected 1 / 1)"; ERR=1; }
[ "${LF:-0}" -gt $((LOST_AT + 30)) ] || { echo "  FAIL  lost run: only ${LF:-0} frames (needs > $((LOST_AT + 30)))"; ERR=1; }
# swapchain present (r195): window == readback at exit, also after the simulated loss (new surface + swapchain on the same window)
for n in vk vklost; do
    grep -q "mcplay: vk present=swapchain" "$W/$n.out" || { if grep -q "mcplay: vk present=readback" "$W/$n.out"; then echo "  SKIP-SC $n: no swapchain on this device (readback present)"; else echo "  FAIL  $n: no 'vk present=' line"; ERR=1; fi; continue; }
    PR=$(num "$W/$n.out" "mcplay: vk present=swapchain" presents); WD=$(num "$W/$n.out" "mcplay: vk present=swapchain" window_diff)
    [ "${PR:-0}" -ge 30 ] || { echo "  FAIL  $n: swapchain presents ${PR:-0} < 30"; ERR=1; }
    [ "${WD:-x}" = 0 ] || { echo "  FAIL  $n: window vs readback differ (window_diff=${WD:-?}; -1 = no window pixels)"; ERR=1; }
    echo "  $n: $(grep -o 'vk present=swapchain.*' "$W/$n.out" | head -1)"
done
grep -q "mcplay: vk present=readback" "$W/vkrb.out" || { echo "  FAIL  vkrb: not the readback present path"; ERR=1; }
echo "  vk: frames=${VF:-?} boxes=${BX:-?}; lost run: lost=${LV:-?} recovered=${LR:-?} frames=${LF:-?}"
python3 tools/mc/play/psnr.py "$W/cpu.png" "$W/vk.png" --min-db "$PMIN" --mask 0,0,140,50 --diff "$W/diff.png" || ERR=1
python3 tools/mc/play/psnr.py "$W/cpu.png" "$W/vklost.png" --min-db "$PMIN" --mask 0,0,140,50 || ERR=1
python3 tools/mc/play/psnr.py "$W/cpu.png" "$W/vkrb.png" --min-db "$PMIN" --mask 0,0,140,50 || ERR=1
if [ -n "${MCPLAY_VK_PNG_OUT:-}" ]; then for n in cpu vk vklost vkrb; do [ -s "$W/$n.png" ] && cp "$W/$n.png" "${MCPLAY_VK_PNG_OUT}_$n.png"; done; fi
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-vk: ok" || echo "RESULT mc-vk: FAIL"
exit $ERR
