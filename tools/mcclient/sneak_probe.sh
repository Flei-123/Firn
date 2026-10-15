#!/usr/bin/env bash
# tools/mcclient/sneak_probe.sh [mc_version=1.21.4] [variants="none shift shift_loaded shift_move"] -- real server: does a right click on a chest open it after
# Player Input (shift) / Entity Action? Uses node-minecraft-protocol as an independent packet writer (see sneak_probe.cjs). Run via heavy.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export NODE_PATH="${NODE_PATH:-/root/firn/tools/mcserver/node_modules}"
MCV=${1:-1.21.4}; VARIANTS=${2:-"none shift shift_loaded shift_move"}
read -r PROTO NMP < <(node -e '
  const v = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).versions[process.argv[2]];
  if (!v) process.exit(1);
  console.log(v.protocol + " " + (v.nmp || process.argv[2]));
' "$HERE/../mcgen/versions.json" "$MCV") || { echo "unknown version $MCV"; exit 2; }
export MC_NMP=$NMP
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
W=$(mktemp -d); SRV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; sleep 1; rm -rf "$W"; }
trap cleanup EXIT
mkdir -p "$W/srv"
MCCLIENT_JAR_CACHE=${MCCLIENT_JAR_CACHE:-/root/.w5/jars} bash "$HERE/fetch_server.sh" "$W/srv" "$MCV" || exit 2
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=creative
force-gamemode=true
level-type=minecraft\:flat
view-distance=3
simulation-distance=3
spawn-protection=0
difficulty=peaceful
spawn-monsters=false
spawn-animals=false
enable-rcon=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; echo "server did not start"; exit 1; }
echo "gamerule doDaylightCycle false" >&3
for v in $VARIANTS; do
    P="Sn$v"; P=${P:0:16}
    mkfifo "$W/p_$v"; exec 4<>"$W/p_$v"
    node "$HERE/sneak_probe.cjs" 127.0.0.1 "$PORT" "$P" "$v" <"$W/p_$v" >"$W/o_$v" 2>&1 &
    NP=$!
    for i in $(seq 1 40); do grep -q ready "$W/o_$v" 2>/dev/null && break; sleep 0.5; done
    sleep 2
    echo "gamemode survival $P" >&3; echo "give $P stone 32" >&3
    echo "execute at $P run setblock ~1 ~ ~ minecraft:chest" >&3
    sleep 2
    echo go >&4; sleep 4
    [ -n "${PROBE_DEBUG:-}" ] && head -c 1500 "$W/o_$v"; grep result "$W/o_$v" || { echo "no result for $v"; head -5 "$W/o_$v"; }
    echo quit >&4; exec 4>&-; kill $NP 2>/dev/null
    echo "kill $P" >&3; sleep 1
done
echo stop >&3; sleep 1
