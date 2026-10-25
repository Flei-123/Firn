#!/usr/bin/env bash
# tools/mc/menu/run_menu.sh -- r363: vanilla main menu in mcplay.
# A: layout gate: menu_dump renders the title at 3 sizes; layout_check.py compares the button borders and the language icon against widgets.png from the launcher jar (+ shifted counter probe).
# B: window gate (Xvfb): the window pixels match the same layout, click Multiplayer -> Add Server -> type address -> Done -> select -> Join Server against the official 1.20.1 server; cfg keeps the server entry.
# B also checks that the server row shows the pinged player count (worker thread ping).
# C: click Quit Game exits 0 without connecting.
# Needs MC_VANILLA_JAR / MC_VANILLA_ASSETS / MC_VANILLA_INDEX (default /root/.w5/...), java, Xvfb, xdotool, python3 + Pillow. Run via heavy. Prints `RESULT mc-menu: ok`.
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
cleanup() { [ -n "${MENU_KEEP_DIR:-}" ] && { mkdir -p "$MENU_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/server.log "$MENU_KEEP_DIR"/ 2>/dev/null; }; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-menu: FAIL"; exit 1; }
for t in Xvfb java xdotool xwd python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
[ -f "$MC_VANILLA_JAR" ] || fail "missing $MC_VANILLA_JAR"
ERR=0
CHECK=tools/mc/menu/layout_check.py
$FIRNC -o "$W/menu_dump" tools/mc/menu/menu_dump.fi 2>"$W/err" || { head -10 "$W/err"; fail "menu_dump does not compile"; }
for sz in 854x480 640x360 1280x720; do
  MENU_DUMP_W=${sz%x*} MENU_DUMP_H=${sz#*x} MENU_DUMP_OUT="$W/t_$sz.ppm" "$W/menu_dump" || { echo "  FAIL  menu_dump $sz"; ERR=1; continue; }
  python3 -I "$CHECK" "$W/t_$sz.ppm" "$MC_VANILLA_JAR" || ERR=1
done
python3 -I "$CHECK" "$W/t_854x480.ppm" "$MC_VANILLA_JAR" 1 >/dev/null 2>&1 && { echo "  FAIL  counter probe: shifted layout must not match"; ERR=1; }
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
wait_log() { local i; for i in $(seq 1 "$2"); do grep -qE "$1" "$W/server.log" && return 0; sleep 1; done; return 1; }
win() { local i w; for i in $(seq 1 30); do w=$(xdotool search --name mcplay 2>/dev/null | head -1); [ -n "$w" ] && { echo "$w"; return 0; }; sleep 0.5; done; return 1; }
click() { xdotool mousemove --window "$1" "$2" "$3"; sleep 0.25; xdotool mousedown 1; sleep 0.2; xdotool mouseup 1; sleep 0.6; }
shot() { xwd -id "$1" -silent -out "$W/$2.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/$2.xwd" "$W/$2.ppm"; }

printf 'name=MenuJ\nprotocol=763\nview=3\n' > "$W/m.cfg"
MCPLAY_CONFIG="$W/m.cfg" MCPLAY_RUN_S=60 "$W/mcplay" > "$W/B.out" 2>&1 & PB=$!
WB=$(win) || { echo "  FAIL  B: no menu window"; ERR=1; WB=""; }
if [ -n "$WB" ]; then
  xdotool windowfocus "$WB" 2>/dev/null; xdotool mousemove --window "$WB" 5 5; sleep 2
  if shot "$WB" title; then python3 -I "$CHECK" "$W/title.ppm" "$MC_VANILLA_JAR" || ERR=1; else echo "  FAIL  B: screenshot"; ERR=1; fi
  click "$WB" 426 284
  click "$WB" 634 396
  xdotool type --delay 80 "127.0.0.1:$PORT"; sleep 0.5
  click "$WB" 426 356
  sleep 4
  if shot "$WB" multi; then python3 -I -c "from PIL import Image; Image.open('$W/multi.ppm').save('$W/multi.png')"; python3 -I tools/mc/menu/ping_check.py "$W/multi.ppm" || ERR=1; else echo "  FAIL  B: screenshot multi"; ERR=1; fi
  click "$WB" 426 92
  click "$WB" 218 396
  wait_log "MenuJ joined the game" 120 || { echo "  FAIL  B: no join via Multiplayer/Add/Join"; ERR=1; }
  sleep 12; shot "$WB" world && python3 -I -c "from PIL import Image; Image.open('$W/world.ppm').save('$W/world.png')"
fi
wait $PB; [ $? -eq 0 ] || { echo "  FAIL  B: exit code"; ERR=1; }
grep -q "127.0.0.1:$PORT" "$W/m.cfg" || { echo "  FAIL  B: server entry not in cfg"; ERR=1; }
grep -q "^name=MenuJ" "$W/m.cfg" || { echo "  FAIL  B: name changed"; ERR=1; }

printf 'name=MenuQ\nprotocol=763\nview=3\n' > "$W/q.cfg"
MCPLAY_CONFIG="$W/q.cfg" MCPLAY_RUN_S=30 "$W/mcplay" > "$W/C.out" 2>&1 & PC=$!
WC=$(win) || { echo "  FAIL  C: no menu window"; ERR=1; WC=""; }
if [ -n "$WC" ]; then xdotool windowfocus "$WC" 2>/dev/null; sleep 1.5; click "$WC" 528 404; fi
wait $PC; [ $? -eq 0 ] || { echo "  FAIL  C: Quit must exit 0"; ERR=1; }
wait_log "MenuQ joined the game" 2 && { echo "  FAIL  C: connected on Quit"; ERR=1; }
[ $ERR -eq 0 ] && echo "RESULT mc-menu: ok" || echo "RESULT mc-menu: FAIL"
exit $ERR
