#!/usr/bin/env bash
# tools/mc/play/run_view.sh -- Render Distance slider (Options > Video Settings) takes effect live in mcplay against the vanilla 1.20.1 server:
# slider up -> `view applied` log + more stream sections (1.20.1 servers ignore the client view, the loaded column count stays), slider down -> fewer sections, view= persisted in the cfg, next start loads it.
# Needs MC_VANILLA_JAR / MC_VANILLA_ASSETS / MC_VANILLA_INDEX, java, Xvfb, xdotool. Run via heavy. Prints `RESULT mc-play-view: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
export MC_VANILLA_JAR=${MC_VANILLA_JAR:-/root/.w5/jars/client-1.20.1.jar}
export MC_VANILLA_ASSETS=${MC_VANILLA_ASSETS:-/root/.w5/menu-assets}
export MC_VANILLA_INDEX=${MC_VANILLA_INDEX:-5}
export MC_VANILLA=${MC_VANILLA_FORCE:-1}
W=$(mktemp -d); SRV_PID=""; XV_PID=""; PP=""
cleanup() { [ -n "${VIEW_KEEP_DIR:-}" ] && { mkdir -p "$VIEW_KEEP_DIR"; cp "$W"/*.out "$W"/*.cfg "$W"/server.log "$VIEW_KEEP_DIR"/ 2>/dev/null; }; exec 3>&- 2>/dev/null; [ -n "$PP" ] && kill "$PP" 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-view: FAIL"; exit 1; }
for t in Xvfb java xdotool; do command -v $t >/dev/null || fail "missing tool: $t"; done
[ -f "$MC_VANILLA_JAR" ] || fail "missing $MC_VANILLA_JAR"
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
mkdir -p "$W/srv"
MCCLIENT_JAR_CACHE=${MCCLIENT_JAR_CACHE:-/root/.w5/jars} bash tools/mcclient/fetch_server.sh "$W/srv" 1.20.1 || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=creative
level-type=minecraft\:flat
view-distance=16
simulation-distance=3
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
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
export MCPLAY_LOG=- FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_FPS=30 MCPLAY_W=854 MCPLAY_H=480 MCPLAY_CREATIVE=0 MCPLAY_PROGRESS=1 MCPLAY_CONFIG="$W/v.cfg"
unset MCPLAY_VIEW MCPLAY_LOD
ERR=0
prog() { grep -o "progress .*" "$W/$1.out" 2>/dev/null | tail -1 | grep -o " $2=[^ ]*" | cut -d= -f2; }
win() { local i w; for i in $(seq 1 30); do w=$(xdotool search --name mcplay 2>/dev/null | head -1); [ -n "$w" ] && { echo "$w"; return 0; }; sleep 0.5; done; return 1; }
click() { xdotool mousemove --window "$1" "$2" "$3"; sleep 0.25; xdotool mousedown 1; sleep 0.3; xdotool mouseup 1; sleep 0.8; }
wait_cols() {
    local nm=$1 cmp=$2 ref=$3 i c
    for i in $(seq 1 ${4:-300}); do c=$(prog "$nm" ssec); c=${c:-0}; case "$cmp" in gt) [ "$c" -gt "$ref" ] && { echo "$c"; return 0; };; lt) [ "$c" -lt "$ref" ] && [ "$c" -gt 0 ] && { echo "$c"; return 0; };; esac; sleep 0.2; done
    echo "$c"; return 1
}
slider_px() { python3 -c "print(2*(62+($1-2)*142//30))"; }
open_video() { xdotool key --window "$1" Escape; sleep 1; click "$1" 320 300; click "$1" 266 172; }
printf 'name=ViewV\nprotocol=763\n' > "$W/v.cfg"
"$W/mcplay" 127.0.0.1 "$PORT" ViewV > "$W/A.out" 2>&1 & PP=$!
for i in $(seq 1 1200); do grep -q "ViewV joined the game" "$W/server.log" 2>/dev/null && break; sleep 0.1; done
grep -q "ViewV joined the game" "$W/server.log" || fail "no join"
WA=$(win) || fail "no window"
xdotool windowfocus "$WA" 2>/dev/null
sleep 12
echo "  start: view=$(prog A view) fsview=$(prog A fsview) ssec=$(prog A ssec)"
[ "$(prog A view)" = 6 ] || { echo "  FAIL  start view is not 6"; ERR=1; }
set_view() {
    local v=$1 pat=$2 t
    for t in 1 2 3; do
        click "$WA" "$(slider_px "$v")" 76
        sleep 1
        grep -q "$pat" "$W/A.out" && return 0
        sleep 2
    done
    return 1
}
open_video "$WA"
set_view 12 "view applied requested=12 view=8 lod=12 wire=12" || { echo "  FAIL  no 'view applied requested=12' in the client log"; grep "view applied" "$W/A.out"; ERR=1; }
sleep 3
echo "  view 12: view=$(prog A view) lod=$(prog A lod) fsview=$(prog A fsview) ssec=$(prog A ssec)"
[ "$(prog A view)" = 8 ] && [ "$(prog A lod)" = 12 ] && [ "$(prog A fsview)" = 12 ] || { echo "  FAIL  progress line does not show view=8 lod=12 fsview=12"; ERR=1; }
grep -q '^view=12' "$W/v.cfg" || { echo "  FAIL  view=12 not persisted in the cfg"; cat "$W/v.cfg"; ERR=1; }
set_view 3 "view applied requested=3 view=3 lod=0 wire=3" || { echo "  FAIL  no 'view applied requested=3' in the client log"; grep "view applied" "$W/A.out"; ERR=1; }
sleep 3
echo "  view 3: view=$(prog A view) lod=$(prog A lod) fsview=$(prog A fsview) ssec=$(prog A ssec)"
[ "$(prog A view)" = 3 ] && [ "$(prog A lod)" = 0 ] && [ "$(prog A fsview)" = 3 ] || { echo "  FAIL  progress line does not show view=3 lod=0 fsview=3"; ERR=1; }
grep -q '^view=3' "$W/v.cfg" || { echo "  FAIL  view=3 not persisted in the cfg"; cat "$W/v.cfg"; ERR=1; }
kill "$PP" 2>/dev/null; wait "$PP" 2>/dev/null; PP=""
sleep 2
"$W/mcplay" 127.0.0.1 "$PORT" ViewV > "$W/B.out" 2>&1 & PP=$!
for i in $(seq 1 600); do grep -q "session start" "$W/B.out" 2>/dev/null && break; sleep 0.1; done
sleep 8
grep -q "session start view=3 lod=0" "$W/B.out" || { echo "  FAIL  restart did not load view=3"; grep "session start" "$W/B.out"; ERR=1; }
echo "  restart: $(grep -o 'session start[^\\]*' "$W/B.out" | head -1) ssec=$(prog B ssec)"
kill "$PP" 2>/dev/null; wait "$PP" 2>/dev/null; PP=""
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-view: ok" || echo "RESULT mc-play-view: FAIL"
exit $ERR
