#!/usr/bin/env bash
# tools/mc/play/run_aj.sh -- mcplay Auto-Jump + relative position flags against the official server (MC_VERSION, default 1.20.1; 1.21.4 = u32 flags + delta, protocol 769) (flat world, creative, offline).
# Session A (MCPLAY_AUTOJUMP unset): walks (W) against a 1 block stone step and must STAY below it.
# Session B (MCPLAY_AUTOJUMP=1): first `execute as <p> at @s run tp @s ~2 ~6 ~` (the server sends relative flags): the client must end at x+2
#   (server `data get entity` agrees with the client position), then it walks (W) against the step and must climb onto it.
# Both: no "moved too quickly" / "moved wrongly" in server.log, at most 4 Synchronize Player Position packets (join + tp + tp rel + slack).
# Env: MC_VERSION (1.20.1|1.21.1|1.21.4|1.21.8|1.21.11|26.1), FIRNC, AJ_RUN_S (50), AJ_KEEP_DIR (copy of outs + server.log, always). Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${AJ_KEEP_DIR:-}" ] && { mkdir -p "$AJ_KEEP_DIR"; cp "$W"/*.out "$W"/*.pos "$W/server.log" "$AJ_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-aj: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=${MC_VERSION:-1.20.1}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
[ "$PROTO" != 0 ] || fail "unknown version $MCV (set MCPLAY_PROTOCOL)"
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
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
spawn-animals=true
spawn-npcs=false
enable-rcon=false
enable-query=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; sleep 1; done; return 1; }
ERR=0
# session NAME AUTOJUMP(0|1) REL(0|1)
session() {
    local PN=$1 AJ=$2 REL=$3 NL
    local AJENV="MCPLAY_AUTOJUMP=0"
    [ "$AJ" = 1 ] && AJENV="MCPLAY_AUTOJUMP=1"
    env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_POS_LOG=1 MCPLAY_RUN_S=${AJ_RUN_S:-50} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 $AJENV \
        "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/$PN.out" 2>&1 &
    local PP=$!
    if wait_log "$PN joined the game" 90; then
        sleep 3
        xdotool mousemove 320 180 2>/dev/null
        NL=$(wc -l < "$W/server.log")
        echo "tp $PN 0.5 -60 0.5 0 0" >&3
        wait_log "Teleported $PN to" 10 "$NL" || { echo "  FAIL  $PN: tp not confirmed"; ERR=1; }
        sleep 1
        NL=$(wc -l < "$W/server.log")
        echo "fill -6 -60 4 8 -60 40 minecraft:stone" >&3   # the 1 block step: stone top at y = -59 for z >= 4
        wait_log "Successfully filled|No blocks were filled" 10 "$NL" || { echo "  FAIL  $PN: fill failed"; ERR=1; }
        sleep 2
        if [ "$REL" = 1 ]; then
            NL=$(wc -l < "$W/server.log")
            echo "execute as $PN at @s run tp @s ~2 ~6 ~" >&3
            wait_log "Teleported $PN to" 10 "$NL" || { echo "  FAIL  $PN: relative tp not confirmed"; ERR=1; }
            sleep 5   # fall + settle
        fi
        xdotool keydown w; sleep 4; xdotool keyup w
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
session PAj0 0 0
session PAj1 1 1
for PN in PAj0 PAj1; do grep -E "poslog sync" "$W/$PN.out" | head -6; done
# verdict per session: python reads the poslog rows and the server Pos and prints key=value
verdict() {
python3 -I - "$W/$1.out" "$W/$1.pos" <<'PY'
import re, sys
rows = []
for l in open(sys.argv[1], errors="replace"):
    m = re.search(r"poslog tick=(\d+) tp=(\d+) x=(-?\d+) y=(-?\d+) z=(-?\d+) vy=(-?\d+) ground=(\w+)", l)
    if m:
        rows.append(tuple(int(m.group(i)) for i in range(1, 7)) + (m.group(7) == "true",))
if not rows:
    print("norows=1"); sys.exit()
last = rows[-1]
print("teleports=%d" % last[1])
print("end_x=%d end_y=%d end_z=%d" % (last[2], last[3], last[4]))
try:
    pos = open(sys.argv[2]).read()
    m = re.search(r"\[(-?[\d.]+)d, (-?[\d.]+)d, (-?[\d.]+)d\]", pos)
    print("srv_x=%d srv_y=%d srv_z=%d" % tuple(int(float(m.group(i)) * 1000) for i in (1, 2, 3)))
except Exception:
    print("srv=none")
PY
}
V0=$(verdict PAj0); V1=$(verdict PAj1)
echo "  A (autojump off): $(echo $V0)"
echo "  B (autojump on, rel tp): $(echo $V1)"
get() { echo "$2" | grep -o "$1=-\?[0-9]*" | head -1 | cut -d= -f2; }
A_Z=$(get end_z "$V0"); A_Y=$(get end_y "$V0")
[ "${A_Z:-99999}" -le 3800 ] || { echo "  FAIL  A: walked past the step without autojump (end z=$A_Z mm)"; ERR=1; }
[ "${A_Y:-0}" -le -59900 ] || { echo "  FAIL  A: y=$A_Y mm, expected -60000"; ERR=1; }
# B: x ended at ~2.5 m (relative x), climbed the step (y ~ -59 m, z > 4.3 m)
BX=$(get end_x "$V1"); BY=$(get end_y "$V1"); BZ=$(get end_z "$V1")
{ [ "${BX:-0}" -ge 2400 ] && [ "${BX:-0}" -le 2600 ]; } || { echo "  FAIL  B: end x=$BX mm, expected ~2500 (relative tp ~2)"; ERR=1; }
[ "${BZ:-0}" -ge 4300 ] || { echo "  FAIL  B: end z=$BZ mm, expected >= 4300 (on the step)"; ERR=1; }
{ [ "${BY:-0}" -ge -59100 ] && [ "${BY:-0}" -le -58900 ]; } || { echo "  FAIL  B: end y=$BY mm, expected ~ -59000"; ERR=1; }
# client and server agree at the end (no desync from the relative flags)
for pair in "A:$V0" "B:$V1"; do
    N=${pair%%:*}; V=${pair#*:}
    for ax in x z; do
        c=$(get end_$ax "$V"); s=$(get srv_$ax "$V")
        if [ -z "$s" ] || [ -z "$c" ]; then echo "  FAIL  $N: no $ax to compare (client=$c server=$s)"; ERR=1; continue; fi
        d=$(( c - s )); [ $d -lt 0 ] && d=$(( -d ))
        [ $d -le 150 ] || { echo "  FAIL  $N: $ax client=$c mm server=$s mm"; ERR=1; }
    done
done
TP1=$(get teleports "$V1"); [ "${TP1:-99}" -le 4 ] || { echo "  FAIL  B: $TP1 Synchronize Player Position packets (join + tp + rel tp = 3): rubber band"; ERR=1; }
if grep -qE "moved too quickly|moved wrongly" "$W/server.log"; then
    echo "  FAIL  server complains about movement:"; grep -E "moved too quickly|moved wrongly" "$W/server.log" | head -3; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-play-aj: ok" || echo "RESULT mc-play-aj: FAIL"
exit $ERR
