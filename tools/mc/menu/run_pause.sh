#!/usr/bin/env bash
# tools/mc/menu/run_pause.sh -- r384/r385/r389: in-game pause menu in mcplay under Xvfb against the official 1.20.1 server.
# Esc -> pause grid matches widgets.png -> Options (Done button) -> FOV slider to max -> Controls (rebind Walk Forwards to J, key file written) -> Done x2 -> Back to Game.
# Checks: opt_fov=110 persisted in the cfg, key.forward=36 in <cfg>_keys.cfg, second Esc + Disconnect exits mcplay with code 0.
# Needs MC_VANILLA_JAR / MC_VANILLA_ASSETS / MC_VANILLA_INDEX, java, Xvfb, xdotool, python3 + Pillow. Run via heavy. Prints `RESULT mc-pause: ok`.
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
fail() { echo "  FAIL  $1"; echo "RESULT mc-pause: FAIL"; exit 1; }
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
DN=$((90 + RANDOM % 100))
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

printf 'name=PauseP\nprotocol=763\nview=3\n' > "$W/p.cfg"
MCPLAY_CONFIG="$W/p.cfg" MCPLAY_RUN_S=120 "$W/mcplay" 127.0.0.1 $PORT PauseP > "$W/P.out" 2>&1 & PP=$!
wait_log "PauseP joined the game" 120 || { echo "  FAIL  no join"; ERR=1; }
WP=$(win) || { echo "  FAIL  no window"; ERR=1; WP=""; }
if [ -n "$WP" ]; then
  xdotool windowfocus "$WP" 2>/dev/null; sleep 10
  key "$WP" Escape
  check "$WP" pause
  click "$WP" 320 300
  check "$WP" options
  click "$WP" 408 76
  click "$WP" 586 172
  check "$WP" controls
  click "$WP" 440 132
  xdotool key --window "$WP" j; sleep 1
  check "$WP" controls
  click "$WP" 586 442
  click "$WP" 426 436
  click "$WP" 426 156
  sleep 2
  key "$WP" Escape
  check "$WP" pause
  click "$WP" 426 348
fi
wait $PP; [ $? -eq 0 ] || { echo "  FAIL  exit code"; ERR=1; }
grep -q '^opt_fov=110' "$W/p.cfg" || { echo "  FAIL  opt_fov=110 not in cfg"; grep opt_ "$W/p.cfg"; ERR=1; }
grep -q 'key.forward=36' "$W/p_keys.cfg" 2>/dev/null || { echo "  FAIL  key.forward=36 not in key file"; cat "$W/p_keys.cfg" 2>/dev/null | head -5; ERR=1; }
[ $ERR -eq 0 ] && echo "RESULT mc-pause: ok" || echo "RESULT mc-pause: FAIL"
exit $ERR
