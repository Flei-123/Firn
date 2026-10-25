#!/usr/bin/env bash
# tools/mc/play/run_session.sh -- r39: FleiLauncher -> mcplay hand-off (GDD 5a.2, lib/mc/mc_session.fi) against the official 1.20.1 server (offline mode).
# A: MCPLAY_SESSION_FILE=<file> (host, port, name, protocol, view; NO command line arguments) -> the player joins, the file is DELETED.
# B: MCPLAY_SESSION_FILE=- (same text on stdin) -> the player joins.
# C: MCPLAY_SESSION_KEEP=1 keeps the file; command line arguments override the file (wrong name in the file, right one on the command line).
# D: missing file / no host -> exit 2, no connection.
# Env: FIRNC, SESSION_KEEP_DIR (copy of outs + server.log). Start it via heavy (java server). Prints `RESULT mc-play-session: ok`.
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
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
export FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360
wait_log() { local i; for i in $(seq 1 "$2"); do grep -qE "$1" "$W/server.log" && return 0; sleep 1; done; return 1; }
ERR=0
txt() { printf '# launcher session\nhost=127.0.0.1\nport=%s\nname=%s\nprotocol=763\nview=3\n' "$PORT" "$1"; }

# A: file, no arguments
txt SessA > "$W/a.session"; chmod 600 "$W/a.session"
MCPLAY_SESSION_FILE="$W/a.session" MCPLAY_RUN_S=25 "$W/mcplay" > "$W/A.out" 2>&1 & PA=$!
wait_log "SessA joined the game" 120 || { echo "  FAIL  A: no join"; ERR=1; }
[ ! -e "$W/a.session" ] || { echo "  FAIL  A: session file not deleted"; ERR=1; }
wait $PA; [ $? -eq 0 ] || { echo "  FAIL  A: exit code"; ERR=1; }

# B: stdin
txt SessB | MCPLAY_SESSION_FILE=- MCPLAY_RUN_S=25 "$W/mcplay" > "$W/B.out" 2>&1 & PB=$!
wait_log "SessB joined the game" 120 || { echo "  FAIL  B: no join"; ERR=1; }
wait $PB; [ $? -eq 0 ] || { echo "  FAIL  B: exit code"; ERR=1; }

# C: KEEP + command line override
txt WrongName > "$W/c.session"
MCPLAY_SESSION_FILE="$W/c.session" MCPLAY_SESSION_KEEP=1 MCPLAY_RUN_S=25 "$W/mcplay" 127.0.0.1 "$PORT" SessC > "$W/C.out" 2>&1 & PC=$!
wait_log "SessC joined the game" 120 || { echo "  FAIL  C: no join"; ERR=1; }
wait $PC; [ $? -eq 0 ] || { echo "  FAIL  C: exit code"; ERR=1; }
[ -e "$W/c.session" ] || { echo "  FAIL  C: MCPLAY_SESSION_KEEP=1 but the file is gone"; ERR=1; }
grep -q "WrongName joined" "$W/server.log" && { echo "  FAIL  C: file name used instead of the command line"; ERR=1; }

# D: missing file, no host
MCPLAY_SESSION_FILE="$W/none.session" "$W/mcplay" > "$W/D1.out" 2>&1; [ $? -eq 2 ] || { echo "  FAIL  D1: missing file must exit 2"; ERR=1; }
printf 'port=%s\nname=NoHost\n' "$PORT" > "$W/d.session"
MCPLAY_SESSION_FILE="$W/d.session" "$W/mcplay" > "$W/D2.out" 2>&1; [ $? -eq 2 ] || { echo "  FAIL  D2: no host must exit 2"; ERR=1; }
grep -q "NoHost joined" "$W/server.log" && { echo "  FAIL  D2: connected without a host"; ERR=1; }
focus() { local i w; for i in $(seq 1 20); do w=$(xdotool search --name mcplay 2>/dev/null | head -1); [ -n "$w" ] && { xdotool windowfocus "$w" 2>/dev/null; sleep 0.5; return 0; }; sleep 0.5; done; return 1; }
printf 'host=127.0.0.1\nport=%s\nname=ConfigName\nprotocol=763\nview=3\n' "$PORT" > "$W/e.cfg"
printf 'form=1\nname=FormE\nprotocol=763\nview=3\n' > "$W/e.session"
MCPLAY_CONFIG="$W/e.cfg" MCPLAY_SESSION_FILE="$W/e.session" MCPLAY_RUN_S=25 "$W/mcplay" > "$W/E.out" 2>&1 & PE=$!
focus || { echo "  FAIL  E: no start window"; ERR=1; }
xdotool key Return
wait_log "FormE joined the game" 120 || { echo "  FAIL  E: form with session name did not join as FormE"; ERR=1; }
wait $PE; [ $? -eq 0 ] || { echo "  FAIL  E: exit code"; ERR=1; }
[ ! -e "$W/e.session" ] || { echo "  FAIL  E: session file not deleted"; ERR=1; }
grep -q "ConfigName joined" "$W/server.log" && { echo "  FAIL  E: config name used instead of the session name"; ERR=1; }
[ $ERR -eq 0 ] && echo "RESULT mc-play-session: ok" || echo "RESULT mc-play-session: FAIL"
exit $ERR
