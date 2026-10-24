#!/usr/bin/env bash
# tools/mc/play/run_health.sh [mc_version=1.20.1] -- health / hunger / death screen / respawn against an official server: `damage` lowers hp, `kill` shows the death screen, key R respawns

# Prints `RESULT mc-play-health: ok|FAIL`. Env as run_inv.sh (FIRNC, JAVA, MC_VIEW, MCPLAY_KEEP_DIR).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
MCV=${1:-1.20.1}; VIEW=${MC_VIEW:-3}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
[ "$PROTO" != 0 ] || { echo "unknown version $MCV (set MCPLAY_PROTOCOL)"; exit 2; }
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; sleep 1; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-health: FAIL"; exit 1; }
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
gamemode=survival
force-gamemode=true
level-seed=4242
view-distance=$VIEW
simulation-distance=$VIEW
spawn-protection=0
difficulty=easy
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
DN=$((120 + RANDOM % 70))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
MCPLAY_AUDIO=0 MCPLAY_HEALTH_LOG=1 MCPLAY_RUN_S=${HEALTH_RUN_S:-90} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 200 "$W/mcplay" 127.0.0.1 "$PORT" HealthA > "$W/a.out" 2>&1 &
PP=$!
wait_for() { # pattern, seconds
    local i
    for i in $(seq 1 "$2"); do grep -Eq "$1" "$W/a.out" 2>/dev/null && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 1; done
    return 1
}
for i in $(seq 1 90); do grep -q "HealthA joined the game" "$W/server.log" 2>/dev/null && break; sleep 1; done
wait_for 'health hp=20 food=20 dead=false' 30 || { echo "  FAIL  no initial health line"; ERR=1; }
sleep 4
echo "damage HealthA 6" >&3
wait_for 'health hp=14 ' 20 || { echo "  FAIL  hp did not drop to 14"; ERR=1; }
echo "kill HealthA" >&3
wait_for 'dead=true deaths=1 .*msg=.+' 25 || { echo "  FAIL  no death screen state with a message"; ERR=1; }
export DISPLAY=:$DN
xdotool mousemove 320 180 2>/dev/null
for i in 1 2 3 4 5 6; do
    grep -Eq 'dead=false deaths=1 alive=1' "$W/a.out" && break
    xdotool key r 2>/dev/null; sleep 2
done
wait_for 'dead=false deaths=1 alive=1' 15 || { echo "  FAIL  not alive again after respawn key"; ERR=1; }
grep -Eq 'respawn_sent=[1-9]' "$W/a.out" || { echo "  FAIL  respawn command not sent"; ERR=1; }
sleep 2
echo "damage HealthA 4" >&3
wait_for 'health hp=16 .*dead=false deaths=1 alive=1' 20 || { echo "  FAIL  hp after respawn not 16"; ERR=1; }
wait $PP
grep -qiE "invalid|disconnect|kicked|illegal" "$W/server.log" && grep -iE "invalid|disconnect|kicked|illegal" "$W/server.log" | head -3
grep 'health hp=' "$W/a.out" | head -8
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-health: ok" || echo "RESULT mc-play-health: FAIL"
exit $ERR
