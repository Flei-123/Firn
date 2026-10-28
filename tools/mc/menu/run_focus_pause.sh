#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
export MC_VANILLA_JAR=${MC_VANILLA_JAR:-/root/.w5/jars/client-1.20.1.jar}
export MC_VANILLA_ASSETS=${MC_VANILLA_ASSETS:-/root/.w5/menu-assets}
export MC_VANILLA_INDEX=${MC_VANILLA_INDEX:-5}
export MC_VANILLA=${MC_VANILLA_FORCE:-1}
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { [ -n "${MENU_KEEP_DIR:-}" ] && { mkdir -p "$MENU_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/server.log "$W"/*.cfg "$MENU_KEEP_DIR"/ 2>/dev/null; }; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-focus-pause: FAIL"; exit 1; }
for t in Xvfb java xdotool xwd python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
[ -f "$MC_VANILLA_JAR" ] || fail "missing $MC_VANILLA_JAR"
ERR=0
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
view-distance=3
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
export FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_FPS=30 MCPLAY_W=854 MCPLAY_H=480 MCPLAY_LOG=- MCPLAY_CREATIVE=0
CHECK=tools/mc/menu/pause_check.py
wait_log() { local i; for i in $(seq 1 "$2"); do grep -qE "$1" "$W/server.log" && return 0; sleep 1; done; return 1; }
win() { local i w; for i in $(seq 1 30); do w=$(xdotool search --name mcplay 2>/dev/null | head -1); [ -n "$w" ] && { echo "$w"; return 0; }; sleep 0.5; done; return 1; }
click() { xdotool mousemove --window "$1" "$2" "$3"; sleep 0.25; xdotool mousedown 1; sleep 0.2; xdotool mouseup 1; sleep 0.6; }
shot() { xwd -id "$1" -silent -out "$W/$2.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/$2.xwd" "$W/$2.ppm"; }
check() { xdotool mousemove --window "$1" 5 5; sleep 0.8; if shot "$1" "$2"; then python3 -I "$CHECK" "$W/$2.ppm" "$MC_VANILLA_JAR" "$2" || { ERR=1; python3 -I -c "from PIL import Image; Image.open('$W/$2.ppm').save('$W/$2.png')"; }; else echo "  FAIL  screenshot $2"; ERR=1; fi; }
key() { xdotool key --window "$1" "$2"; sleep 0.8; }
$FIRNC -o "$W/other" tools/input/focus_main.fi 2>"$W/err2" || { head -10 "$W/err2"; fail "focus probe does not compile"; }
printf 'name=FocusF\nprotocol=763\nview=3\n' > "$W/p.cfg"
MCPLAY_CONFIG="$W/p.cfg" MCPLAY_RUN_S=120 "$W/mcplay" 127.0.0.1 $PORT FocusF > "$W/P.out" 2>&1 & PP=$!
wait_log "FocusF joined the game" 120 || { echo "  FAIL  no join"; ERR=1; }
WP=$(win) || { echo "  FAIL  no window"; ERR=1; WP=""; }
if [ -n "$WP" ]; then
  xdotool windowfocus "$WP" 2>/dev/null; sleep 10
  "$W/other" raw plain > "$W/other.log" 2>&1 & OP=$!
  for i in $(seq 1 50); do grep -q READY "$W/other.log" 2>/dev/null && break; sleep 0.2; done
  WO=$(xdotool search --name mffocus_b | head -1)
  xdotool windowfocus "$WO"; sleep 2
  check "$WP" pause
  xdotool windowfocus "$WP" 2>/dev/null; sleep 1
  xdotool mousemove --window "$WP" 5 5; sleep 0.5
  xdotool key --window "$WO" q 2>/dev/null
  kill $OP 2>/dev/null
  click "$WP" 426 348
fi
wait $PP; [ $? -eq 0 ] || { echo "  FAIL  exit code"; ERR=1; }
grep -q '^mcplay: input grab=false' "$W/P.out" || { echo "  FAIL  grab flag still set at the end"; grep "input grab" "$W/P.out" | head -2; ERR=1; }
[ $ERR -eq 0 ] && echo "RESULT mc-focus-pause: ok" || echo "RESULT mc-focus-pause: FAIL"
exit $ERR
