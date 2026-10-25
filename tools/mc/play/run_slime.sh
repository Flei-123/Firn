#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${SLIME_KEEP_DIR:-}" ] && { mkdir -p "$SLIME_KEEP_DIR"; cp "$W"/*.out "$W/server.log" "$SLIME_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-slime: FAIL"; exit 1; }
for t in Xvfb xdotool java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=1.20.1
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
level-type=minecraft\:flat
view-distance=6
simulation-distance=4
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
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "randomTickSpeed 0"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; sleep 1; done; return 1; }
srv() { local NL; NL=$(wc -l < "$W/server.log"); echo "$1" >&3; wait_log "$2" 10 "$NL" || { echo "  FAIL  server command not confirmed: $1"; ERR=1; }; }
ERR=0
PN=PSlime1
env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_POS_LOG=1 MCPLAY_RUN_S=${SLIME_RUN_S:-60} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 \
    "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/run.out" 2>&1 &
PP=$!
if wait_log "$PN joined the game" 90; then
    sleep 3
    xdotool mousemove 320 180 2>/dev/null
    srv "setblock 0 -61 0 minecraft:slime_block" "Changed the block|Could not set"
    sleep 1
    srv "tp $PN 3.5 -60 3.5" "Teleported $PN to"
    sleep 2
    srv "tp $PN 0.5 -40 0.5" "Teleported $PN to"
    sleep 7
    srv "tp $PN 6.5 -40 0.5" "Teleported $PN to"
    sleep 7
    xdotool keydown shift
    sleep 1
    srv "tp $PN 0.5 -40 0.5" "Teleported $PN to"
    sleep 7
    xdotool keyup shift
else
    echo "  FAIL  no join"; ERR=1
fi
wait $PP; RC=$?
[ $RC -eq 0 ] || { echo "  FAIL  exit code $RC"; ERR=1; }
VERDICT=$(python3 -I - "$W/run.out" <<'PY'
import re, sys
rows = []
for l in open(sys.argv[1], errors="replace"):
    m = re.search(r"poslog tick=(\d+) tp=(\d+) x=(-?\d+) y=(-?\d+) z=(-?\d+) vy=(-?\d+) ground=(\w+)", l)
    if m:
        rows.append(tuple(int(m.group(i)) for i in range(1, 7)) + (m.group(7) == "true",))
if not rows:
    print("norows=1"); sys.exit()
print("teleports=%d" % rows[-1][1])
for name, tp in (("a", 3), ("b", 4), ("c", 5)):
    ph = [r for r in rows if r[1] == tp]
    if not ph:
        print("%s_rows=0" % name); continue
    vmin = min(range(len(ph)), key=lambda i: ph[i][5])
    after = ph[vmin:]
    print("%s_vmin=%d" % (name, ph[vmin][5]))
    print("%s_vup=%d" % (name, max(r[5] for r in after)))
    print("%s_rise=%d" % (name, max([r[3] for r in after[1:]] or [min(r[3] for r in ph)]) - min(r[3] for r in ph)))
    print("%s_end_y=%d" % (name, ph[-1][3]))
PY
)
echo "  $(echo $VERDICT)"
get() { echo "$VERDICT" | grep -o "$1=-\?[0-9]*" | head -1 | cut -d= -f2; }
[ "$(get norows)" != 1 ] || { echo "  FAIL  no poslog rows"; ERR=1; }
TPC=$(get teleports)
[ "${TPC:-99}" -le 5 ] || { echo "  FAIL  ${TPC:-?} position syncs (join + 4 tp = 5 max): rubber band"; ERR=1; }
[ "${TPC:-0}" -ge 5 ] || { echo "  FAIL  only ${TPC:-0} position syncs, expected 5"; ERR=1; }
AU=$(get a_vup); AR=$(get a_rise); AM=$(get a_vmin)
{ [ "${AM:-0}" -le -1000 ]; } || { echo "  FAIL  A: fall speed ${AM:-?} mm/tick, expected <= -1000"; ERR=1; }
{ [ "${AU:-0}" -ge 800 ]; } || { echo "  FAIL  A: no bounce, max vy after impact ${AU:-?} mm/tick (need >= 800)"; ERR=1; }
{ [ "${AR:-0}" -ge 4000 ]; } || { echo "  FAIL  A: bounce rose only ${AR:-?} mm (need >= 4000)"; ERR=1; }
BU=$(get b_vup); BR=$(get b_rise)
{ [ "${BU:-9999}" -le 200 ] && [ "${BR:-9999}" -le 500 ]; } || { echo "  FAIL  B: bounce on grass (vy ${BU:-?}, rise ${BR:-?})"; ERR=1; }
CU=$(get c_vup); CR=$(get c_rise)
{ [ "${CU:-9999}" -le 200 ] && [ "${CR:-9999}" -le 500 ]; } || { echo "  FAIL  C: bounce while sneaking (vy ${CU:-?}, rise ${CR:-?})"; ERR=1; }
if grep -qE "moved too quickly|moved wrongly" "$W/server.log"; then
    echo "  FAIL  server complains about movement:"; grep -E "moved too quickly|moved wrongly" "$W/server.log" | head -3; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-play-slime: ok" || echo "RESULT mc-play-slime: FAIL"
exit $ERR
