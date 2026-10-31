#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
W=$(mktemp -d)
SRV=""
trap '[ -n "$SRV" ] && kill -9 $SRV 2>/dev/null; rm -rf "$W"' EXIT
SEED=${GEN_SEED:-11}
VIEW=${GEN_VIEW:-3}
EXPECT=$(( (2 * VIEW + 1) * (2 * VIEW + 1) ))
PORT=$(python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
"$FIRNC" -o "$W/firnmc-server" demos/firnmc-server/main.fi 2>"$W/err" || { echo "  FAIL  compile firnmc-server"; head -8 "$W/err"; exit 1; }
mkdir -p "$W/run"
printf 'level-type=minecraft:normal\nlevel-seed=%s\nworldgen-data=%s/tools/mcgen/worldgen/1.20.1\nview-distance=%s\nwhite-list=false\nserver-port=%s\n' "$SEED" "$ROOT" "$VIEW" "$PORT" > "$W/run/server.properties"
mkfifo "$W/run/in.fifo"
(sleep 600 > "$W/run/in.fifo" &)
(cd "$W/run" && setsid nohup "$W/firnmc-server" nogui < in.fifo > out.log 2>&1 & echo $! > "$W/pid")
SRV=$(cat "$W/pid")
for _ in $(seq 1 100); do grep -q 'Done (' "$W/run/out.log" 2>/dev/null && break; sleep 0.1; done
grep -q 'Done (' "$W/run/out.log" || { echo "  FAIL  server start"; tail -5 "$W/run/out.log"; exit 1; }
node tools/mc/wgen/gen_bot.cjs 127.0.0.1 "$PORT" "$SEED" "$EXPECT" > "$W/got.txt" 2> "$W/bot.err"
RC=$?
cat "$W/bot.err"
[ $RC = 0 ] || { echo "  FAIL  bot join/chunks (rc=$RC)"; tail -5 "$W/run/out.log"; exit 1; }
bash tools/mc/wgen/fill_oracle.sh "$W/ref.txt" overworld "$SEED:0:0" 2>&1 | grep -v WARNING
if cmp -s "$W/ref.txt" "$W/got.txt"; then echo "  ok    join: $EXPECT chunks, chunk 0,0 identical to oracle ($(wc -l < "$W/ref.txt") rows)"; echo "mc/wgen join: all ok"; exit 0; fi
echo "  FAIL  chunk 0,0 differs: $(diff "$W/ref.txt" "$W/got.txt" | grep -c '^<') rows"; exit 1
