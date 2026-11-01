#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# join_only.sh <version> <protocol> <mcclient-binary> [server-jar]
# Joins the Firn client to a REAL vanilla server (offline mode, local throw-away instance) and prints the
# client metrics (walk_ok / walk_fail = schema walker verdict on every received packet). No reference client.
# EULA: eula=true is written for this local test instance (bound to 127.0.0.1), as in run.sh.
set -uo pipefail
VER=${1:?version}
PROTO=${2:?protocol}
BIN=${3:?mcclient binary}
JAR=${4:-/root/.w5/jars/server-$VER.jar}
JAVA=${JAVA:-/root/tools/jdk25/bin/java}
VIEW=${MC_VIEW:-4}
IDLE=${MC_IDLE_MS:-4000}
W=$(mktemp -d)
SPID=""
cleanup() {
  [ -n "$SPID" ] && kill "$SPID" 2>/dev/null
  sleep 1
  [ -n "$SPID" ] && kill -9 "$SPID" 2>/dev/null
  rm -rf "$W"
}
trap cleanup EXIT
mkdir -p "$W/srv"
cp "$JAR" "$W/srv/server.jar"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<EOP
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
level-seed=4242
view-distance=$VIEW
simulation-distance=$VIEW
spawn-protection=0
max-players=4
network-compression-threshold=256
enable-rcon=false
enable-query=false
motd=join test
white-list=false
enforce-whitelist=false
EOP
mkfifo "$W/in"
exec 3<>"$W/in"
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SPID=$!
for i in $(seq 1 120); do
  grep -q 'Done (' "$W/server.log" 2>/dev/null && break
  kill -0 "$SPID" 2>/dev/null || { tail -n 20 "$W/server.log"; echo "server died"; exit 2; }
  sleep 1
done
grep -q 'Done (' "$W/server.log" || { tail -n 20 "$W/server.log"; echo "server timeout"; exit 2; }
echo "== server ready (port $PORT) =="
timeout 120 "$BIN" 127.0.0.1 "$PORT" Dumper "$W/out.dump" "$VIEW" "$IDLE" "$PROTO" > "$W/metrics" 2> "$W/err"
echo "client exit $?"
cat "$W/metrics" | head -30
head -c 3000 "$W/err"
sleep 2
grep -E "lost connection|Disconnect|rror|xception|Failed|Invalid" "$W/server.log" | tail -n 6 | cut -c1-300
[ -f "${MC_DUMP_OUT:-}" ] || { [ -n "${MC_DUMP_OUT:-}" ] && cp "$W/out.dump" "$MC_DUMP_OUT" 2>/dev/null; }
echo "stop" >&3
