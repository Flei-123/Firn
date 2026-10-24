#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
MCV=${1:-1.20.1}; VIEW=${MC_VIEW:-4}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
[ "$PROTO" != 0 ] || { echo "unknown version $MCV (set MCPLAY_PROTOCOL)"; exit 2; }
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
W=$(mktemp -d); SRV_PID=""; XV_PID=""; PP=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$PP" ] && kill "$PP" 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; sleep 1; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-swim: FAIL"; exit 1; }
for t in Xvfb xdotool; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
mkdir -p "$W/srv"
MCCLIENT_JAR_CACHE=${MCCLIENT_JAR_CACHE:-/root/.w5/jars} bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=survival
force-gamemode=true
level-type=minecraft\:flat
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
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "drowningDamage false" "fallDamage false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((120 + RANDOM % 70))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
PN=SwimA
ERR=0
MCPLAY_AUDIO=0 FIRN_AUDIO=null MCPLAY_CREATIVE=0 MCPLAY_POS_LOG=1 MCPLAY_RUN_S=${SWIM_RUN_S:-170} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/a.out" 2>&1 &
PP=$!
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 1; done; return 1; }
srv_cmd() { local nl; nl=$(wc -l < "$W/server.log"); echo "$1" >&3; wait_log "$2" "${3:-15}" "$nl"; }
cur_tick() { grep -o 'poslog tick=[0-9]*' "$W/a.out" | tail -1 | cut -d= -f2; }
wait_ticks() { local t0 t i; t0=$(cur_tick); t0=${t0:-0}; for i in $(seq 1 120); do t=$(cur_tick); [ "${t:-0}" -ge $((t0 + $1)) ] && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 1; done; return 1; }
wait_log "$PN joined the game" 90 || fail "no join"
sleep 4
xdotool mousemove 320 180 2>/dev/null
srv_cmd "fill 5 -60 5 11 -51 11 minecraft:stone" "Successfully filled" || fail "tower walls"
srv_cmd "fill 6 -59 6 10 -52 10 minecraft:water" "Successfully filled" || fail "tower water"
srv_cmd "fill 6 -51 6 10 -51 10 minecraft:air" "Successfully filled" || fail "tower top"
srv_cmd "fill -2 -60 21 2 -48 21 minecraft:stone" "Successfully filled" || fail "ladder wall"
srv_cmd "fill 0 -60 20 0 -49 20 minecraft:ladder[facing=north]" "Successfully filled" || fail "ladders"
sleep 2
srv_cmd "tp $PN 8.5 -53 8.5 0 0" "Teleported $PN to" || { echo "  FAIL  tower tp not confirmed"; ERR=1; }
wait_ticks 20 || { echo "  FAIL  no poslog ticks"; ERR=1; }
S0=$(cur_tick); wait_ticks 80; S1=$(cur_tick)
srv_cmd "data get entity $PN Pos" "$PN has the following entity data" && grep "$PN has the following entity data" "$W/server.log" | tail -1 > "$W/sink.pos"
xdotool keydown space
R0=$(cur_tick); wait_ticks 80; R1=$(cur_tick)
xdotool keyup space
srv_cmd "tp $PN 0.5 -60 20.5 0 0" "Teleported $PN to" || { echo "  FAIL  ladder tp not confirmed"; ERR=1; }
wait_ticks 30
xdotool keydown w
C0=$(cur_tick); wait_ticks 60; C1=$(cur_tick)
xdotool keyup w
wait_ticks 4
xdotool keydown shift
H0=$(cur_tick); wait_ticks 40; H1=$(cur_tick)
srv_cmd "data get entity $PN Pos" "$PN has the following entity data" && grep "$PN has the following entity data" "$W/server.log" | tail -1 > "$W/hold.pos"
xdotool keyup shift
D0=$(cur_tick); wait_ticks 40; D1=$(cur_tick)
echo "marks sink=$S0-$S1 rise=$R0-$R1 climb=$C0-$C1 hold=$H0-$H1 down=$D0-$D1"
python3 -I - "$W/a.out" "$W/sink.pos" "$W/hold.pos" "$S0:$S1" "$R0:$R1" "$C0:$C1" "$H0:$H1" "$D0:$D1" <<'PY' || ERR=1
import re, sys
rows = []
for l in open(sys.argv[1], errors="replace"):
    m = re.search(r"poslog tick=(\d+) tp=(\d+) x=(-?\d+) y=(-?\d+) z=(-?\d+) vy=(-?\d+) ground=(\w+)", l)
    if m:
        rows.append(tuple(int(m.group(i)) for i in range(1, 7)) + (m.group(7) == "true",))
def span(a):
    lo, hi = (int(v) for v in a.split(":"))
    return [r for r in rows if lo <= r[0] <= hi]
def srv(path):
    try:
        m = re.search(r"\[(-?[\d.]+)d, (-?[\d.]+)d, (-?[\d.]+)d\]", open(path).read())
        return [int(float(m.group(i)) * 1000) for i in (1, 2, 3)]
    except Exception:
        return None
bad = 0
def check(ok, msg):
    global bad
    if not ok:
        print("  FAIL  " + msg)
        bad += 1
sink, rise, climb, hold, down = (span(a) for a in sys.argv[4:9])
for n, s in (("sink", sink), ("rise", rise), ("climb", climb), ("hold", hold), ("down", down)):
    check(len(s) >= 5, "%s: only %d poslog rows" % (n, len(s)))
if bad:
    sys.exit(1)
dy = sink[0][3] - sink[-1][3]
print("  sink: y %d -> %d (drop %d mm) vy_end=%d" % (sink[0][3], sink[-1][3], dy, sink[-1][5]))
check(1000 <= dy <= 4500, "sink drop %d mm outside 1000..4500" % dy)
check(all(-80 <= r[5] <= 5 for r in sink[2:]), "sink vy outside -80..5 mm/tick: %s" % [r[5] for r in sink])
check(not any(r[6] for r in sink), "sink: on ground in the middle of the water")
sp = srv(sys.argv[2])
check(sp is not None, "no server position after the sink phase")
if sp:
    d = [abs(sink[-1][2] - sp[0]), abs(sink[-1][4] - sp[2])]
    print("  sink: client x/z %d/%d server %d/%d, y %d vs %d" % (sink[-1][2], sink[-1][4], sp[0], sp[2], sink[-1][3], sp[1]))
    check(max(d) <= 250 and abs(sink[-1][3] - sp[1]) <= 800, "sink: client and server positions differ")
rg = rise[-1][3] - rise[0][3]
top = max(r[3] for r in rise)
print("  rise: y %d -> %d, max %d (gain %d mm)" % (rise[0][3], rise[-1][3], top, rg))
check(rg >= 2500, "rise gain %d mm < 2500" % rg)
check(-52800 <= top <= -49500, "rise top y %d mm outside -52800..-49500 (surface ~ -51100)" % top)
check(max(r[5] for r in rise) <= 300, "rise vy above 300 mm/tick")
cg = climb[-1][3] - climb[0][3]
cv = max(r[5] for r in climb)
print("  climb: y %d -> %d (gain %d mm) vy_max=%d" % (climb[0][3], climb[-1][3], cg, cv))
check(4500 <= cg <= 9000, "climb gain %d mm outside 4500..9000" % cg)
check(90 <= cv <= 140, "climb vy max %d outside 90..140 mm/tick (vanilla 117.6)" % cv)
hd = max(r[3] for r in hold) - min(r[3] for r in hold)
print("  hold: y spread %d mm at y=%d" % (hd, hold[-1][3]))
check(hd <= 300, "hold spread %d mm > 300 (sneak must hold the ladder)" % hd)
hp = srv(sys.argv[3])
check(hp is not None, "no server position while holding the ladder")
if hp:
    print("  hold: client %d/%d/%d server %d/%d/%d" % (hold[-1][2], hold[-1][3], hold[-1][4], hp[0], hp[1], hp[2]))
    check(max(abs(hold[-1][2] - hp[0]), abs(hold[-1][4] - hp[2])) <= 250 and abs(hold[-1][3] - hp[1]) <= 250, "hold: client and server positions differ")
dd = down[0][3] - down[-1][3]
dv = min(r[5] for r in down)
print("  down: y %d -> %d (drop %d mm) vy_min=%d" % (down[0][3], down[-1][3], dd, dv))
check(dd >= 2500, "slide down only %d mm" % dd)
check(dv >= -170, "slide vy min %d below -170 mm/tick (clamp 150)" % dv)
sys.exit(1 if bad else 0)
PY
if grep -qE "moved too quickly|moved wrongly" "$W/server.log"; then
    echo "  FAIL  server complains about movement:"; grep -E "moved too quickly|moved wrongly" "$W/server.log" | head -3; ERR=1
fi
grep -qiE "invalid|disconnect|kicked|illegal|protocol error" "$W/server.log" && { grep -iE "invalid|disconnect|kicked|illegal|protocol error" "$W/server.log" | head -3; ERR=1; }
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-swim: ok" || echo "RESULT mc-play-swim: FAIL"
exit $ERR
