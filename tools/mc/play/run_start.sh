#!/usr/bin/env bash
# tools/mc/play/run_start.sh -- r159: mcplay without arguments shows the start screen (lib/mc/mc_startscreen_run.fi) against the official 1.20.1 server (offline mode).
# A: MCPLAY_CONFIG prefilled + MCPLAY_START_AUTO=1 -> no window input needed, the player joins.
# B: MCPLAY_CONFIG prefilled, form shown, Return (xdotool) -> the player joins and the config keeps host / port / name.
# C: empty config, form typed by xdotool (host, Tab, port, Tab, name, Return) -> the player joins and the config file is written.
# F: ctrl+a replaces the port, Save writes the config without joining, a second instance exits 0 (MCPLAY_SINGLE=1).
# D: Escape in the form -> exit 0, no connection.
# Env: FIRNC, START_KEEP_DIR (copy of outs + server.log). Start it via heavy (java server). Prints `RESULT mc-play-start: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { [ -n "${SESSION_KEEP_DIR:-}" ] && { mkdir -p "$SESSION_KEEP_DIR"; cp "$W"/*.out "$W/server.log" "$SESSION_KEEP_DIR"/ 2>/dev/null; }; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-session: FAIL"; exit 1; }
for t in Xvfb java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
mkdir -p "$W/srv"
bash tools/mcclient/fetch_server.sh "$W/srv" 1.20.1 || fail "server download"
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
export FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_LOG=-
wait_log() { local i; for i in $(seq 1 "$2"); do grep -qE "$1" "$W/server.log" && return 0; sleep 1; done; return 1; }
ERR=0
cfg() { printf 'host=127.0.0.1\nport=%s\nname=%s\nprotocol=763\nview=3\n' "$PORT" "$1"; }
focus() { local i w; for i in $(seq 1 60); do w=$(xdotool search --onlyvisible --name mcplay 2>/dev/null | head -1); [ -n "$w" ] && xdotool windowfocus "$w" 2>/dev/null && [ "$(xdotool getwindowfocus 2>/dev/null)" = "$w" ] && { sleep 1; return 0; }; sleep 0.5; done; return 1; }

cfg StartA > "$W/a.cfg"
MCPLAY_CONFIG="$W/a.cfg" MCPLAY_START_AUTO=1 MCPLAY_RUN_S=20 "$W/mcplay" > "$W/A.out" 2>&1 & PA=$!
wait_log "StartA joined the game" 120 || { echo "  FAIL  A: no join"; ERR=1; }
wait $PA; [ $? -eq 0 ] || { echo "  FAIL  A: exit code"; ERR=1; }

cfg StartB > "$W/b.cfg"
MCPLAY_CONFIG="$W/b.cfg" MCPLAY_RUN_S=20 "$W/mcplay" > "$W/B.out" 2>&1 & PB=$!
focus || { echo "  FAIL  B: no start window"; ERR=1; }
xdotool key Return
wait_log "StartB joined the game" 120 || { echo "  FAIL  B: no join"; ERR=1; }
wait $PB; [ $? -eq 0 ] || { echo "  FAIL  B: exit code"; ERR=1; }
grep -q "^name=StartB" "$W/b.cfg" && grep -q "^port=$PORT" "$W/b.cfg" || { echo "  FAIL  B: config changed"; ERR=1; }

rm -f "$W/c.cfg"
MCPLAY_CONFIG="$W/c.cfg" MCPLAY_RUN_S=20 "$W/mcplay" > "$W/C.out" 2>&1 & PC=$!
focus || { echo "  FAIL  C: no start window"; ERR=1; }
xdotool type --delay 80 "127.0.0.1"; sleep 0.3
xdotool key Tab; sleep 0.3
xdotool key ctrl+a; sleep 0.3
xdotool type --delay 80 "$PORT"; sleep 0.3
xdotool key Tab; sleep 0.3
xdotool type --delay 80 "StartC"; sleep 0.5
xdotool key Return
wait_log "StartC joined the game" 120 || { echo "  FAIL  C: no join"; ERR=1; }
wait $PC; [ $? -eq 0 ] || { echo "  FAIL  C: exit code"; ERR=1; }
grep -q "^host=127.0.0.1" "$W/c.cfg" && grep -q "^name=StartC" "$W/c.cfg" && grep -q "^port=$PORT" "$W/c.cfg" || { echo "  FAIL  C: config not written"; ERR=1; }

rm -f "$W/e.cfg"
MCPLAY_CONFIG="$W/e.cfg" MCPLAY_RUN_S=20 "$W/mcplay" > "$W/E.out" 2>&1 & PE=$!
focus || { echo "  FAIL  E: no start window"; ERR=1; }
xdotool type --delay 80 "localhost"; sleep 0.3
xdotool key Tab; sleep 0.3
xdotool key BackSpace BackSpace BackSpace BackSpace BackSpace BackSpace; sleep 0.3
xdotool type --delay 80 "$PORT"; sleep 0.3
xdotool key Tab; sleep 0.3
xdotool type --delay 80 "StartE"; sleep 0.5
xdotool key Return
wait_log "StartE joined the game" 120 || { echo "  FAIL  E: no join via hostname localhost"; ERR=1; }
wait $PE; [ $? -eq 0 ] || { echo "  FAIL  E: exit code"; ERR=1; }
grep -q "^host=localhost" "$W/e.cfg" && grep -q "^name=StartE" "$W/e.cfg" && grep -q "^port=$PORT" "$W/e.cfg" || { echo "  FAIL  E: hostname not written to config"; ERR=1; }

rm -f "$W/f.cfg"
MCPLAY_SINGLE=1 MCPLAY_CONFIG="$W/f.cfg" "$W/mcplay" > "$W/F.out" 2>&1 & PF=$!
focus || { echo "  FAIL  F: no start window"; ERR=1; }
xdotool type --delay 80 "127.0.0.1"; sleep 0.3
xdotool key Tab; sleep 0.3
xdotool key ctrl+a; sleep 0.3
xdotool type --delay 80 "$PORT"; sleep 0.3
xdotool key Tab; sleep 0.3
xdotool type --delay 80 "StartF"; sleep 0.5
WF=$(xdotool search --name mcplay 2>/dev/null | head -1)
xdotool mousemove --window "$WF" 402 368 click 1; sleep 1
grep -q "^host=127.0.0.1" "$W/f.cfg" && grep -q "^name=StartF" "$W/f.cfg" && grep -q "^port=$PORT" "$W/f.cfg" || { echo "  FAIL  F: Save did not write the config"; ERR=1; }
MCPLAY_SINGLE=1 MCPLAY_CONFIG="$W/f.cfg" timeout 20 "$W/mcplay" > "$W/F2.out" 2>&1; RF2=$?
[ $RF2 -eq 0 ] && grep -q "already running" "$W/F2.out" || { echo "  FAIL  F: second instance rc=$RF2 must exit 0 with 'already running'"; ERR=1; }
kill -0 $PF 2>/dev/null || { echo "  FAIL  F: first instance gone"; ERR=1; }
xdotool key Escape
wait $PF; [ $? -eq 0 ] || { echo "  FAIL  F: Escape must exit 0"; ERR=1; }

MCPLAY_CONFIG="$W/d.cfg" "$W/mcplay" > "$W/D.out" 2>&1 & PD=$!
focus || { echo "  FAIL  D: no start window"; ERR=1; }
xdotool key Escape
wait $PD; [ $? -eq 0 ] || { echo "  FAIL  D: Escape must exit 0"; ERR=1; }
grep -q "connect" "$W/D.out" && { echo "  FAIL  D: tried to connect"; ERR=1; }
[ $ERR -eq 0 ] && echo "RESULT mc-play-start: ok" || echo "RESULT mc-play-start: FAIL"
exit $ERR
