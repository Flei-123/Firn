#!/usr/bin/env bash
# tools/mc/play/run_hud.sh -- vanilla HUD in mcplay under Xvfb against the official 1.20.1 server (survival): bars vs widgets/icons.png, F3 debug, F1 hide, F2 PNG screenshot, container panels (chest/furnace/crafting table, HUD_CONT=1, default with HUD_WINE=1) pixel-checked. Prints `RESULT mc-hud: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
MCV=${1:-1.20.1}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
PLAIN=0; [ "$MCV" = 1.20.1 ] || PLAIN=1
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
if [ $PLAIN -eq 0 ]; then
  export MC_VANILLA_JAR=${MC_VANILLA_JAR:-/root/.w5/jars/client-1.20.1.jar}
  export MC_VANILLA_ASSETS=${MC_VANILLA_ASSETS:-/root/.w5/menu-assets}
  export MC_VANILLA_INDEX=${MC_VANILLA_INDEX:-5}
  export MC_VANILLA=${MC_VANILLA_FORCE:-1}
else
  export MC_VANILLA_JAR=/nonexistent
fi
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { [ -n "${HUD_KEEP_DIR:-}" ] && { mkdir -p "$HUD_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/server.log "$W"/*.cfg "$HUD_KEEP_DIR"/ 2>/dev/null; }; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-hud: FAIL"; exit 1; }
for t in Xvfb java xdotool xwd python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
[ $PLAIN -eq 1 ] || [ -f "$MC_VANILLA_JAR" ] || fail "missing $MC_VANILLA_JAR"
ERR=0
WINE_RUN=${HUD_WINE:-0}
if [ "$WINE_RUN" = 1 ]; then
  command -v wine >/dev/null || fail "missing tool: wine"
  $FIRNC --target=x86_64-windows --opt-level=dev-fast -o "$W/mcplay.exe" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not cross-compile"; }
else
  $FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
fi
mkdir -p "$W/srv"
MCCLIENT_JAR_CACHE=${MCCLIENT_JAR_CACHE:-/root/.w5/jars} bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=survival
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
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 ${HUD_XVFB:-1024x600x24} >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
export FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_FPS=30 MCPLAY_W=854 MCPLAY_H=${HUD_WIN_H:-480} MCPLAY_LOG=- MCPLAY_CREATIVE=0
CHECK=tools/mc/play/hud_check.py
wait_log() { local i; for i in $(seq 1 "$2"); do grep -qE "$1" "$W/server.log" && return 0; sleep 1; done; return 1; }
win() { local i w; for i in $(seq 1 30); do w=$(xdotool search --name mcplay 2>/dev/null | head -1); [ -n "$w" ] && { echo "$w"; return 0; }; sleep 0.5; done; return 1; }
click() { xdotool mousemove --window "$1" "$2" "$3"; sleep 0.25; xdotool mousedown 1; sleep 0.2; xdotool mouseup 1; sleep 0.6; }
shot() { xwd -id "$1" -silent -out "$W/$2.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/$2.xwd" "$W/$2.ppm"; }
check() { xdotool mousemove --window "$1" 5 5; sleep 0.8; if shot "$1" "$2"; then python3 -I "$CHECK" "$W/$2.ppm" "$MC_VANILLA_JAR" "$2" || { ERR=1; python3 -I -c "from PIL import Image; Image.open('$W/$2.ppm').save('$W/$2.png')"; }; else echo "  FAIL  screenshot $2"; ERR=1; fi; }
key() { if [ "$WINE_RUN" = 1 ]; then xdotool windowfocus "$1" 2>/dev/null; xdotool key "$2"; else xdotool key --window "$1" "$2"; fi; sleep 0.8; }

printf 'name=HudP\nprotocol=%s\nview=3\n' "$PROTO" > "$W/p.cfg"
if [ "$WINE_RUN" = 1 ]; then
  export WINEPREFIX="$W/wp" WINEDEBUG=-all WINEARCH=win64
  wine wineboot -i >/dev/null 2>&1
  [ $PLAIN -eq 1 ] || export MC_VANILLA_JAR="Z:$MC_VANILLA_JAR"
  MCPLAY_CONFIG="Z:$W/p.cfg" MCPLAY_PROTOCOL=$PROTO MCPLAY_RUN_S=90 wine "$W/mcplay.exe" 127.0.0.1 $PORT HudP > "$W/P.out" 2>&1 & PP=$!
  MC_VANILLA_JAR=${MC_VANILLA_JAR#Z:}
else
  MCPLAY_CONFIG="$W/p.cfg" MCPLAY_PROTOCOL=$PROTO MCPLAY_RUN_S=90 "$W/mcplay" 127.0.0.1 $PORT HudP > "$W/P.out" 2>&1 & PP=$!
fi
wait_log "HudP joined the game" 120 || { echo "  FAIL  no join"; ERR=1; }
WP=$(win) || { echo "  FAIL  no window"; ERR=1; WP=""; }
hudcheck() { xdotool mousemove --window "$1" 5 5; sleep 0.8; shot "$1" "$2" && python3 -I "$CHECK" "$3" "$W/$2.ppm" "$MC_VANILLA_JAR" "$W/bars.ppm" || { ERR=1; python3 -I -c "from PIL import Image; Image.open('$W/$2.ppm').save('$W/$2.png')" 2>/dev/null; }; }
if [ -n "$WP" ]; then
  xdotool windowfocus "$WP" 2>/dev/null; sleep 12
  hudcheck "$WP" bars bars
  key "$WP" F3
  hudcheck "$WP" debug debug
  key "$WP" F3
  key "$WP" F2
  sleep 2
  key "$WP" F1
  hudcheck "$WP" hidden hidden
  key "$WP" F1
  key "$WP" Escape
  xdotool mousemove --window "$WP" 5 5; sleep 0.8
  shot "$WP" pause && { python3 -I tools/mc/menu/pause_check.py "$W/pause.ppm" "$MC_VANILLA_JAR" pause || { ERR=1; python3 -I -c "from PIL import Image; Image.open('$W/pause.ppm').save('$W/pause.png')"; }; } || { echo "  FAIL  screenshot pause"; ERR=1; }
  key "$WP" Escape
  sleep 1
  key "$WP" e
  hudcheck "$WP" inv inv
  key "$WP" e
  sleep 1
  hudcheck "$WP" bars2 bars
  xdotool windowfocus "$WP" 2>/dev/null
  printf '%s\n' 'bossbar add hudtest {"text":"Boss"}' 'bossbar set minecraft:hudtest players @a' 'bossbar set minecraft:hudtest color red' 'bossbar set minecraft:hudtest value 100' 'scoreboard objectives add hudobj dummy {"text":"Sidebar"}' 'scoreboard objectives setdisplay sidebar hudobj' 'scoreboard players set Alpha hudobj 7' 'title @a title {"text":"HELLO"}' >&3
  sleep 1
  hudcheck "$WP" title title
  hudcheck "$WP" boss boss
  xdotool mousemove --window "$WP" 5 5; sleep 0.5
  shot "$WP" side && { python3 -I "$CHECK" side "$W/side.ppm" "$MC_VANILLA_JAR" "$W/bars.ppm" || ERR=1; }
fi
sleep 1
kill $PP 2>/dev/null; wait $PP 2>/dev/null
if [ "${HUD_CONT:-$WINE_RUN}" = 1 ] && [ $PLAIN -eq 0 ]; then
  . tools/mc/play/hud_cont_stage.sh
  [ "$WINE_RUN" = 1 ] && wineserver -k 2>/dev/null
  run_cont_stage
fi
SH=$(ls "$W"/screenshots/*.png 2>/dev/null | head -1)
if [ -n "$SH" ]; then
  python3 -I - "$SH" "$W/shot.ppm" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
open(sys.argv[2], "wb").write(b"P6\n%d %d\n255\n" % im.size + im.tobytes())
PY
  python3 -I "$CHECK" png "$W/shot.ppm" "$MC_VANILLA_JAR" 854 480 || ERR=1
else
  grep -E "screenshot" "$W/P.out" | head -3
  echo "  FAIL  no screenshot file"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-hud: ok" || echo "RESULT mc-hud: FAIL"
exit $ERR
