#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
PROTO=${DIM_PROTO:-763}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
W=$(mktemp -d)
trap 'kill -9 "$(cat "$W/pid" 2>/dev/null)" 2>/dev/null; rm -rf "$W"' EXIT
PORT=$(python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
"$FIRNC" ${WG_OPT:+--opt-level=$WG_OPT} -o "$W/firnmc-server" demos/firnmc-server/main.fi 2>"$W/err" || { echo "  FAIL  compile firnmc-server"; head -8 "$W/err"; exit 1; }
mkdir -p "$W/run"
printf 'level-type=minecraft:normal\nlevel-seed=11\nlevel-name=gw\nworldgen-data=%s/tools/mcgen/worldgen/1.20.1\nview-distance=2\nwhite-list=false\nserver-port=%s\n' "$ROOT" "$PORT" > "$W/run/server.properties"
python3 -I - "$W/run/ops.json" DimBot:4 <<'PY'
import hashlib, json, sys, uuid
def offline(name):
    b = bytearray(hashlib.md5(("OfflinePlayer:" + name).encode()).digest())
    b[6] = (b[6] & 0x0f) | 0x30
    b[8] = (b[8] & 0x3f) | 0x80
    return str(uuid.UUID(bytes=bytes(b)))
json.dump([{"uuid": offline("DimBot"), "name": "DimBot", "level": 4, "bypassesPlayerLimit": False}], open(sys.argv[1], "w"))
PY
mkfifo "$W/run/in.fifo"
(sleep 200 > "$W/run/in.fifo" &)
(cd "$W/run" && setsid nohup "$W/firnmc-server" nogui --protocol "$PROTO" < in.fifo > out.log 2>&1 & echo $! > "$W/pid")
for _ in $(seq 1 100); do grep -q 'Done (' "$W/run/out.log" 2>/dev/null && break; sleep 0.1; done
grep -q 'Done (' "$W/run/out.log" || { echo "  FAIL  server start"; tail -n 5 "$W/run/out.log"; exit 1; }
stop_server() {
    echo "save-all" > "$W/run/in.fifo"
    sleep 2
    echo "stop" > "$W/run/in.fifo"
    sleep 2
    kill -9 "$(cat "$W/pid")" 2>/dev/null
}
BOT=tools/mc/wgen/dim_bot.cjs; [ "$PROTO" = 777 ] && BOT=tools/mc/wgen/dim_bot777.cjs
DIM_CMDS='["execute in minecraft:the_nether run tp 8 70 8"]' node "$BOT" 127.0.0.1 "$PORT" > "$W/bot1.json" 2>"$W/bot1.err"
stop_server
rm -f "$W/run/in.fifo" "$W/run/out.log"
mkfifo "$W/run/in.fifo"
(sleep 100 > "$W/run/in.fifo" &)
(cd "$W/run" && setsid nohup "$W/firnmc-server" nogui --protocol "$PROTO" < in.fifo > out.log 2>&1 & echo $! > "$W/pid")
for _ in $(seq 1 100); do grep -q 'Done (' "$W/run/out.log" 2>/dev/null && break; sleep 0.1; done
DIM_CMDS='[]' node "$BOT" 127.0.0.1 "$PORT" > "$W/bot2.json" 2>"$W/bot2.err"
node -e '
const a = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))
const b = JSON.parse(require("fs").readFileSync(process.argv[2], "utf8"))
const errs = []
if (a.final !== "minecraft:the_nether") errs.push("first run final " + a.final)
if (b.final !== "minecraft:the_nether") errs.push("second run final " + b.final)
const entry = b.events.filter((e) => /^(login|respawn)/.test(e))
if (JSON.stringify(entry) !== JSON.stringify(["login minecraft:the_nether"])) errs.push("rejoin events " + JSON.stringify(entry))
if (!(b.chunks["minecraft:the_nether"] >= 9)) errs.push("chunks " + JSON.stringify(b.chunks))
if (a.errors.length || b.errors.length) errs.push("errors " + JSON.stringify([a.errors, b.errors]))
if (errs.length) { console.log(errs.join("; ")); console.log(JSON.stringify(b)); process.exit(1) }
' "$W/bot1.json" "$W/bot2.json" && echo "  ok    player logs in directly into the saved dimension after restart" || { echo "  FAIL  dimension restart"; tail -n 5 "$W/run/out.log"; stop_server; exit 1; }
stop_server
echo "mc/wgen dim restart: all ok"
