#!/usr/bin/env bash
# tools/mc/play/run_slow.sh -- movement-effect blocks in mcplay against the official server (MC_VERSION, default 1.20.1).
# Sessions walk W for SLOW_HOLD_S seconds on a stone platform: P control, S soul sand floor, H honey block floor, W cobweb corridor
#   (distance must be 0.45..0.7 / 0.45..0.7 / < 0.35 of the control), B bamboo at cell (6,6) and D pointed dripstone at cell (6,6)
#   (XZ block offsets: the stop position must follow the offset collision box). Every session: client and server end position agree.
# Env: MC_VERSION, FIRNC, SLOW_HOLD_S (4), SLOW_RUN_S (22). Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${SLOW_KEEP_DIR:-}" ] && { mkdir -p "$SLOW_KEEP_DIR"; cp "$W"/*.out "$W"/*.pos "$W/server.log" "$SLOW_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-slow: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=${MC_VERSION:-1.20.1}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
[ "$PROTO" != 0 ] || fail "unknown version $MCV (set MCPLAY_PROTOCOL)"
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
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
level-type=minecraft\:flat
view-distance=4
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
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "fallDamage false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; sleep 1; done; return 1; }
ERR=0

# session NAME SETUP_COMMAND TPX
session() {
    local PN=$1 SETUP=$2 TPX=$3 NL
    env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_POS_LOG=1 MCPLAY_RUN_S=${SLOW_RUN_S:-22} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 \
        "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/$PN.out" 2>&1 &
    local PP=$!
    if wait_log "$PN joined the game" 90; then
        sleep 3
        xdotool mousemove 320 180 2>/dev/null
        echo "gamemode survival $PN" >&3
        for cmd in "fill -8 -56 -1 8 -54 31 minecraft:air" "fill -8 -56 -1 8 -56 31 minecraft:stone"; do echo "$cmd" >&3; done
        sleep 1
        while IFS= read -r cmd; do [ -n "$cmd" ] && echo "$cmd" >&3; done <<< "$SETUP"
        sleep 2
        NL=$(wc -l < "$W/server.log")
        echo "tp $PN $TPX -55 0.5 0 0" >&3
        wait_log "Teleported $PN to" 10 "$NL" || { echo "  FAIL  $PN: tp not confirmed"; ERR=1; }
        sleep 3
        xdotool keydown w
        sleep "${SLOW_HOLD_S:-4}"
        xdotool keyup w
        sleep 2
        NL=$(wc -l < "$W/server.log")
        echo "data get entity $PN Pos" >&3
        wait_log "$PN has the following entity data" 10 "$NL" || { echo "  FAIL  $PN: no Pos from the server"; ERR=1; }
        grep "$PN has the following entity data" "$W/server.log" | tail -1 > "$W/$PN.pos"
    else
        echo "  FAIL  $PN: no join"; ERR=1
    fi
    wait $PP; local RC=$?
    [ $RC -eq 0 ] || { echo "  FAIL  $PN: exit code $RC"; ERR=1; }
}
session PSlP "" 0.5
session PSlS "fill -1 -56 0 1 -56 31 minecraft:soul_sand" 0.5
session PSlH "fill -1 -56 0 1 -56 31 minecraft:honey_block" 0.5
session PSlW "fill -1 -55 1 1 -54 31 minecraft:cobweb" 0.5
session PSlB "setblock 6 -56 6 minecraft:dirt
setblock 6 -55 6 minecraft:bamboo" 6.5
session PSlD "setblock 6 -55 6 minecraft:pointed_dripstone" 6.5
verdict() {
python3 -I - "$W/$1.out" "$W/$1.pos" <<'PY'
import re, sys
rows = []
for l in open(sys.argv[1], errors="replace"):
    m = re.search(r"poslog tick=(\d+) tp=(\d+) x=(-?\d+) y=(-?\d+) z=(-?\d+) vy=(-?\d+) ground=(\w+)", l)
    if m:
        rows.append(tuple(int(m.group(i)) for i in range(1, 7)))
start = next((i for i, r in enumerate(rows) if r[3] > -55500), None)
if start is None:
    print("norows=1"); sys.exit()
rows = rows[start:]
print("end_x=%d end_y=%d end_z=%d" % (rows[-1][2], rows[-1][3], rows[-1][4]))
try:
    m = re.search(r"\[(-?[\d.]+)d, (-?[\d.]+)d, (-?[\d.]+)d\]", open(sys.argv[2]).read())
    print("srv_x=%d srv_y=%d srv_z=%d" % tuple(int(float(m.group(i)) * 1000) for i in (1, 2, 3)))
except Exception:
    print("srv=none")
PY
}
get() { echo "$2" | grep -o "$1=-\?[0-9]*" | head -1 | cut -d= -f2; }
declare -A V
for n in P S H W B D; do V[$n]=$(verdict PSl$n); echo "  $n: $(echo ${V[$n]})"; done
PZ=$(get end_z "${V[P]}"); PZ=$(( ${PZ:-0} - 500 ))
[ "$PZ" -ge 8000 ] || { echo "  FAIL  P: control walked only $PZ mm"; ERR=1; }
for pair in "S:450:700" "H:450:700" "W:0:350"; do
    IFS=: read -r N LO HI <<< "$pair"
    ez=$(get end_z "${V[$N]}"); r=$(( ( ${ez:-0} - 500 ) * 1000 / PZ ))
    { [ $r -ge $LO ] && [ $r -le $HI ]; } || { echo "  FAIL  $N: distance ratio ${r}/1000, expected $LO..$HI"; ERR=1; }
done
check_offset() {
    local N=$1 UNOFF=$2 sz cz d e
    sz=$(get srv_z "${V[$N]}"); cz=$(get end_z "${V[$N]}")
    d=$(( ${sz:-0} - UNOFF )); [ $d -lt 0 ] && d=$(( -d ))
    [ $d -ge 100 ] || { echo "  FAIL  $N: server stop z=${sz} mm is within 100 mm of the un-offset stop ${UNOFF} mm (offset not in effect)"; ERR=1; }
    e=$(( ${cz:-0} - ${sz:-0} )); [ $e -lt 0 ] && e=$(( -e ))
    [ $e -le 60 ] || { echo "  FAIL  $N: client stop z=${cz} mm differs from server ${sz} mm by more than 60 mm"; ERR=1; }
}
check_offset B 6106
check_offset D 6012
for N in P S H W B D; do
    for ax in x z; do
        c=$(get end_$ax "${V[$N]}"); s=$(get srv_$ax "${V[$N]}")
        if [ -z "$s" ] || [ -z "$c" ]; then echo "  FAIL  $N: no $ax to compare (client=$c server=$s)"; ERR=1; continue; fi
        d=$(( c - s )); [ $d -lt 0 ] && d=$(( -d ))
        [ $d -le 150 ] || { echo "  FAIL  $N: $ax client=$c mm server=$s mm"; ERR=1; }
    done
done
if grep -qE "moved too quickly|moved wrongly" "$W/server.log"; then
    echo "  FAIL  server complains about movement:"; grep -E "moved too quickly|moved wrongly" "$W/server.log" | head -3; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-play-slow: ok" || echo "RESULT mc-play-slow: FAIL"
exit $ERR
