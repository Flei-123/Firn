#!/usr/bin/env bash
# tools/mc/play/run_tp.sh -- mcplay WITHOUT fly against the official 1.20.1 server: /tp into the air above terrain, fall, land, walk (r197).
# Checks: tp count <= 3 (join + tp, no rubber band), landed <= 3 s after the tp, moved >= 1 m with W after landing, no "moved too quickly" / "moved wrongly" in server.log, on ground at the end.
# Env: FIRNC, TP_X/TP_Y/TP_Z (default 0.5 100 0.5), TP_RUN_S (35), TP_KEEP_DIR (copy of out + server.log, always). Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${TP_KEEP_DIR:-}" ] && { mkdir -p "$TP_KEEP_DIR"; cp "$W"/*.out "$W/server.log" "$TP_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-tp: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
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
level-seed=4242
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
( cd "$W/srv" && exec "${JAVA:-/usr/bin/java}" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; sleep 1; done; return 1; }
ERR=0
PN=PTp1
env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_POS_LOG=1 MCPLAY_RUN_S=${TP_RUN_S:-35} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 \
    "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/run.out" 2>&1 &
PP=$!
if wait_log "$PN joined the game" 90; then
    sleep 3
    xdotool mousemove 320 180 2>/dev/null
    NL=$(wc -l < "$W/server.log")
    echo "tp $PN ${TP_X:-0.5} ${TP_Y:-100} ${TP_Z:-0.5}" >&3
    wait_log "Teleported $PN to" 10 "$NL" || { echo "  FAIL  tp not confirmed"; ERR=1; }
    sleep 8   # fall + settle
    xdotool keydown w; sleep 3; xdotool keyup w
else
    echo "  FAIL  no join"; ERR=1
fi
wait $PP; RC=$?
[ $RC -eq 0 ] || { echo "  FAIL  exit code $RC"; ERR=1; }
grep -E "poslog" "$W/run.out" | awk 'NR%4==1' | head -30
grep -E "mcplay: (walk|pos)" "$W/run.out"
# walked_mm is the footstep SOUND meter (audio off here -> always 0): use the position log instead
VERDICT=$(python3 -I - "$W/run.out" <<'PY'
import re, sys
rows = []
for l in open(sys.argv[1], errors="replace"):
    m = re.search(r"poslog tick=(\d+) tp=(\d+) x=(-?\d+) y=(-?\d+) z=(-?\d+) vy=(-?\d+) ground=(\w+)", l)
    if m:
        rows.append(tuple(int(m.group(i)) for i in range(1, 7)) + (m.group(7) == "true",))
if not rows:
    print("no poslog"); sys.exit()
tp_rows = [r for r in rows if r[2] > 0 and abs(r[2]) >= 0]
last_tp = rows[-1][1]
print("teleports=%d" % last_tp)
# first row after the commanded tp (y jumps to ~100 m) and the first ground row after it
hi = next((i for i, r in enumerate(rows) if r[3] > 90000), None)
if hi is None:
    print("never above 90 m"); sys.exit()
gi = next((i for i in range(hi, len(rows)) if rows[i][6]), None)
if gi is None:
    print("never landed"); sys.exit()
print("land_ticks=%d" % (rows[gi][0] - rows[hi][0]))
print("moved_mm=%d" % abs(rows[-1][4] - rows[gi][4]))
PY
)
echo "  $VERDICT" | tr '\n' ' '; echo
TPC=$(echo "$VERDICT" | grep -o "teleports=[0-9]*" | cut -d= -f2)
LAND=$(echo "$VERDICT" | grep -o "land_ticks=[0-9]*" | cut -d= -f2)
MOVED=$(echo "$VERDICT" | grep -o "moved_mm=[0-9]*" | cut -d= -f2)
[ "${TPC:-99}" -le 3 ] || { echo "  FAIL  ${TPC:-?} Synchronize Player Position packets (join + tp = 2): rubber band"; ERR=1; }
[ "${LAND:-999}" -le 60 ] || { echo "  FAIL  landed after ${LAND:-?} ticks (> 60 = 3 s)"; ERR=1; }
[ "${MOVED:-0}" -ge 1000 ] || { echo "  FAIL  moved ${MOVED:-0} mm after landing (W held 3 s)"; ERR=1; }
if grep -qE "moved too quickly|moved wrongly" "$W/server.log"; then
    echo "  FAIL  server complains about movement:"; grep -E "moved too quickly|moved wrongly" "$W/server.log" | head -3; ERR=1
fi
LAST=$(grep poslog "$W/run.out" | tail -1)
case "$LAST" in *ground=true*) ;; *) echo "  FAIL  player not on ground at the end: $LAST"; ERR=1;; esac
[ $ERR -eq 0 ] && echo "RESULT mc-play-tp: ok" || echo "RESULT mc-play-tp: FAIL"
exit $ERR
