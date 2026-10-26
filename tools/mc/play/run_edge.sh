#!/usr/bin/env bash
# tools/mc/play/run_edge.sh -- sneak edge (vanilla maybeBackOffFromEdge) in mcplay against the official server (MC_VERSION, default 1.20.1).
# A 5x5 stone platform 5 blocks above the flat ground. Session S: sneaking (MCPLAY_FORCE_SNEAK=1) holds W into the edge: the player must stop with
#   the box still over the platform (z <= 5.30) and never fall. Session D: sneaking W+D (diagonal into a corner): z <= 5.30 and x >= -2.30, no fall.
#   Session C (control, no sneak, W): walks off and falls (proves the platform and the walk are real).
# All sessions: client and server end position agree (<= 150 mm) and server.log has no "moved wrongly" / "moved too quickly".
# Env: MC_VERSION (1.20.1|1.21.1|1.21.4|1.21.8|1.21.11|26.1), FIRNC, EDGE_RUN_S (30), EDGE_KEEP_DIR. Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${EDGE_KEEP_DIR:-}" ] && { mkdir -p "$EDGE_KEEP_DIR"; cp "$W"/*.out "$W"/*.pos "$W/server.log" "$EDGE_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-edge: FAIL"; exit 1; }
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
# session NAME SNEAK(0|1) KEYS(w|wd) HOLD_S
session() {
    local PN=$1 SN=$2 KEYS=$3 HOLD=$4 NL
    env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_FORCE_SNEAK=$SN MCPLAY_POS_LOG=1 MCPLAY_RUN_S=${EDGE_RUN_S:-30} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 \
        "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/$PN.out" 2>&1 &
    local PP=$!
    if wait_log "$PN joined the game" 90; then
        sleep 3
        xdotool mousemove 320 180 2>/dev/null
        echo "gamemode survival $PN" >&3
        NL=$(wc -l < "$W/server.log")
        echo "fill -2 -56 0 2 -56 4 minecraft:stone" >&3   # platform, top face at y = -55; edges: z = 5.0, x = -2.0 / 3.0
        wait_log "Successfully filled|No blocks were filled" 10 "$NL" || { echo "  FAIL  $PN: fill failed"; ERR=1; }
        NL=$(wc -l < "$W/server.log")
        echo "tp $PN 0.5 -55 0.5 0 0" >&3
        wait_log "Teleported $PN to" 10 "$NL" || { echo "  FAIL  $PN: tp not confirmed"; ERR=1; }
        sleep 3
        for k in $(echo "$KEYS" | sed 's/./& /g'); do xdotool keydown $k; done
        sleep "$HOLD"
        for k in $(echo "$KEYS" | sed 's/./& /g'); do xdotool keyup $k; done
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
session PEdS 1 w 7
session PEdD 1 wd 8
session PEdC 0 w 5
dim_session() {
    local PN=PEdN NL
    env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_POS_LOG=1 MCPLAY_RUN_S=${EDGE_DIM_RUN_S:-24} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 \
        "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/$PN.out" 2>&1 &
    local PP=$!
    if wait_log "$PN joined the game" 90; then
        sleep 4
        NL=$(wc -l < "$W/server.log")
        echo "execute in minecraft:the_nether run tp $PN 0.5 100 0.5 0 0" >&3
        wait_log "Teleported $PN to" 10 "$NL" || { echo "  FAIL  $PN: nether tp not confirmed"; ERR=1; }
    else
        echo "  FAIL  $PN: no join"; ERR=1
    fi
    wait $PP; local RC=$?
    [ $RC -eq 0 ] || { echo "  FAIL  $PN: exit code $RC"; ERR=1; }
}
case "$PROTO" in 769|77*|78*) dim_session
    LD=$(grep -o "loaded=[0-9]*" "$W/PEdN.out" | tail -1 | cut -d= -f2)
    echo "  N (dimension change): Player Loaded sent ${LD:-none}x"
    [ "${LD:-0}" -ge 2 ] || { echo "  FAIL  N: Player Loaded not sent again after the dimension change (sent ${LD:-none}x)"; ERR=1; } ;;
esac
# verdict: poslog rows on the platform level (y within 100 mm of -55000) give the extremes; min_y over ALL rows after the tp shows a fall
verdict() {
python3 -I - "$W/$1.out" "$W/$1.pos" <<'PY'
import re, sys
rows = []
for l in open(sys.argv[1], errors="replace"):
    m = re.search(r"poslog tick=(\d+) tp=(\d+) x=(-?\d+) y=(-?\d+) z=(-?\d+) vy=(-?\d+) ground=(\w+)", l)
    if m:
        rows.append(tuple(int(m.group(i)) for i in range(1, 7)))
# rows after the platform tp: the first row with y > -55500
start = next((i for i, r in enumerate(rows) if r[3] > -55500), None)
if start is None:
    print("norows=1"); sys.exit()
rows = rows[start:]
print("max_z=%d min_x=%d min_y=%d" % (max(r[4] for r in rows), min(r[2] for r in rows), min(r[3] for r in rows)))
print("end_x=%d end_y=%d end_z=%d" % (rows[-1][2], rows[-1][3], rows[-1][4]))
try:
    m = re.search(r"\[(-?[\d.]+)d, (-?[\d.]+)d, (-?[\d.]+)d\]", open(sys.argv[2]).read())
    print("srv_x=%d srv_y=%d srv_z=%d" % tuple(int(float(m.group(i)) * 1000) for i in (1, 2, 3)))
except Exception:
    print("srv=none")
PY
}
get() { echo "$2" | grep -o "$1=-\?[0-9]*" | head -1 | cut -d= -f2; }
VS=$(verdict PEdS); VD=$(verdict PEdD); VC=$(verdict PEdC)
echo "  S (sneak, W):      $(echo $VS)"
echo "  D (sneak, W+D):    $(echo $VD)"
echo "  C (control, W):    $(echo $VC)"
SZ=$(get max_z "$VS"); SY=$(get min_y "$VS")
{ [ "${SZ:-0}" -ge 5200 ] && [ "${SZ:-99999}" -le 5330 ]; } || { echo "  FAIL  S: max z=$SZ mm, expected 5200..5330 (box stops over the edge at 5.0 + 0.3)"; ERR=1; }
[ "${SY:--99999}" -ge -55100 ] || { echo "  FAIL  S: fell (min y=$SY mm)"; ERR=1; }
DZ=$(get max_z "$VD"); DX=$(get min_x "$VD"); DY=$(get min_y "$VD")
{ [ "${DZ:-0}" -ge 5200 ] && [ "${DZ:-99999}" -le 5330 ]; } || { echo "  FAIL  D: max z=$DZ mm, expected 5200..5330"; ERR=1; }
{ [ "${DX:-0}" -le -2200 ] && [ "${DX:-0}" -ge -2330 ]; } || { echo "  FAIL  D: min x=$DX mm, expected -2330..-2200"; ERR=1; }
[ "${DY:--99999}" -ge -55100 ] || { echo "  FAIL  D: fell (min y=$DY mm)"; ERR=1; }
CY=$(get min_y "$VC"); [ "${CY:-0}" -le -57000 ] || { echo "  FAIL  C: control did not fall (min y=$CY mm)"; ERR=1; }
for pair in "S:$VS" "D:$VD" "C:$VC"; do
    N=${pair%%:*}; V=${pair#*:}
    for ax in x z; do
        c=$(get end_$ax "$V"); s=$(get srv_$ax "$V")
        if [ -z "$s" ] || [ -z "$c" ]; then echo "  FAIL  $N: no $ax to compare (client=$c server=$s)"; ERR=1; continue; fi
        d=$(( c - s )); [ $d -lt 0 ] && d=$(( -d ))
        [ $d -le 150 ] || { echo "  FAIL  $N: $ax client=$c mm server=$s mm"; ERR=1; }
    done
done
if grep -qE "moved too quickly|moved wrongly" "$W/server.log"; then
    echo "  FAIL  server complains about movement:"; grep -E "moved too quickly|moved wrongly" "$W/server.log" | head -3; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-play-edge: ok" || echo "RESULT mc-play-edge: FAIL"
exit $ERR
