#!/usr/bin/env bash
# tools/mc/play/run_chat.sh [mc_version=1.20.1] -- chat against an official server: console `say` shows as a client line, MCPLAY_CHAT_TEST sends a message and a command that the server logs

# Prints `RESULT mc-play-chat: ok|FAIL`. Env as run_inv.sh (FIRNC, JAVA, MC_VIEW, MCPLAY_KEEP_DIR).
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
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-chat: FAIL"; exit 1; }
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
gamemode=creative
force-gamemode=true
level-seed=4242
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
sleep "${RUN_INV_WARMUP:-0}" # a cold 1.21.11 / 26.1 server answers the first login late
for gr in "doDaylightCycle false" "doMobSpawning false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
echo "op ChatA" >&3
sleep 1
DN=$((120 + RANDOM % 70))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
MCPLAY_AUDIO=0 MCPLAY_CHAT_LOG=1 MCPLAY_CHAT_TEST='hi there|/time set day' MCPLAY_RUN_S=${CHAT_RUN_S:-30} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 170 "$W/mcplay" 127.0.0.1 "$PORT" ChatA > "$W/a.out" 2>&1 &
PP=$!
for i in $(seq 1 90); do grep -q "ChatA joined the game" "$W/server.log" 2>/dev/null && break; sleep 1; done
sleep 8
echo "say hello_chat" >&3
echo "tellraw ChatA {\"text\":\"raw_line\",\"color\":\"gold\"}" >&3
wait $PP
grep -q 'chat line=.*ChatA joined the game' "$W/a.out" || echo "  note  join message line not shown (sent during the login drain)"
grep -q 'chat line=\[Server\] hello_chat' "$W/a.out" || { echo "  FAIL  say line missing"; ERR=1; }
grep -q 'chat line=raw_line' "$W/a.out" || { echo "  FAIL  tellraw line missing"; ERR=1; }
grep -q 'chattest sent=2' "$W/a.out" || { echo "  FAIL  chattest sent != 2"; ERR=1; }
grep -q '<ChatA> hi there' "$W/server.log" || { echo "  FAIL  server did not log the chat message"; ERR=1; }
grep -q 'ChatA issued server command: /time set day' "$W/server.log" || { echo "  FAIL  server did not log the command"; ERR=1; }
grep -qiE "invalid|disconnect|kicked|chat message validation" "$W/server.log" && grep -iE "invalid|disconnect|kicked|validation" "$W/server.log" | head -3
grep 'chat line=' "$W/a.out" | head -5
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-chat: ok" || echo "RESULT mc-play-chat: FAIL"
exit $ERR
