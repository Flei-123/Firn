#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# capture_v777.sh <out.json> -- records the configuration packets (and the first play packets) of a real 26.3 server.
set -uo pipefail
OUT=${1:?out json}
HERE=$(cd "$(dirname "$0")" && pwd)
JAR=${JAR:-/root/.w5/jars/server-26.3.jar}
JAVA=${JAVA:-/root/tools/jdk25/bin/java}
W=$(mktemp -d)
SPID=""
cleanup() {
  exec 3>&- 2>/dev/null
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
gamemode=creative
level-seed=4242
level-type=minecraft\:flat
view-distance=4
simulation-distance=4
white-list=false
enforce-whitelist=false
network-compression-threshold=256
enable-rcon=false
enable-query=false
EOP
mkfifo "$W/in"
exec 3<>"$W/in"
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SPID=$!
for i in $(seq 1 150); do
  grep -q 'Done (' "$W/server.log" 2>/dev/null && break
  kill -0 "$SPID" 2>/dev/null || { tail -n 20 "$W/server.log"; echo "server died"; exit 2; }
  sleep 1
done
grep -q 'Done (' "$W/server.log" || { echo "server timeout"; exit 2; }
node "$HERE/capture_cfg.cjs" 127.0.0.1 "$PORT" 777 "$OUT" 60
echo "capture exit $?"
