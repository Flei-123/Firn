#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
W=$(mktemp -d)
trap 'pkill -9 -f "$W/firnmc-server" 2>/dev/null; rm -rf "$W"' EXIT
SEED=11
PORT=$(python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
"$FIRNC" -o "$W/firnmc-server" demos/firnmc-server/main.fi 2>"$W/err" || { echo "  FAIL  compile firnmc-server"; head -8 "$W/err"; exit 1; }
mkdir -p "$W/run"
printf 'level-type=minecraft:normal\nlevel-seed=%s\nlevel-name=gw\nworldgen-data=%s/tools/mcgen/worldgen/1.20.1\nview-distance=2\nwhite-list=false\nserver-port=%s\n' "$SEED" "$ROOT" "$PORT" > "$W/run/server.properties"

start_server() {
    rm -f "$W/run/in.fifo" "$W/run/out.log"
    mkfifo "$W/run/in.fifo"
    (sleep 120 > "$W/run/in.fifo" &)
    (cd "$W/run" && setsid nohup "$W/firnmc-server" nogui < in.fifo > out.log 2>&1 & echo $! > "$W/pid")
    for _ in $(seq 1 100); do grep -q 'Done (' "$W/run/out.log" 2>/dev/null && break; sleep 0.1; done
    grep -q 'Done (' "$W/run/out.log" || { echo "  FAIL  server start"; tail -5 "$W/run/out.log"; exit 1; }
}

chunk_row() {
    node tools/mc/wgen/gen_bot.cjs 127.0.0.1 "$PORT" "$SEED" 25 2>/dev/null | awk -v y="$1" '$4 == y { print substr($5, 137, 1) }'
}

start_server
BEFORE=$(chunk_row 20)
echo "save-all" > "$W/run/in.fifo"
sleep 2
echo "stop" > "$W/run/in.fifo"
sleep 2
kill -9 "$(cat "$W/pid")" 2>/dev/null
[ -s "$W/run/gw/firn_world.txt" ] || { echo "  FAIL  persist: no world meta file"; exit 1; }
sed -i 's/^11$/11/' "$W/run/gw/firn_world.txt"
printf 'level-type=minecraft:normal\nlevel-seed=999\nlevel-name=gw\nworldgen-data=%s/tools/mcgen/worldgen/1.20.1\nview-distance=2\nwhite-list=false\nserver-port=%s\n' "$ROOT" "$PORT" > "$W/run/server.properties"
start_server
AFTER=$(chunk_row 20)
if [ -n "$BEFORE" ] && [ "$BEFORE" = "$AFTER" ]; then echo "  ok    persist: restart keeps saved seed (chunk row $BEFORE == $AFTER)"; echo "mc/wgen persist: all ok"; exit 0; fi
echo "  FAIL  persist: before=$BEFORE after=$AFTER"; exit 1
