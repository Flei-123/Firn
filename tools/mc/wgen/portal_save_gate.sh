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
printf 'level-type=minecraft:flat\nlevel-name=sw\nview-distance=2\ngamemode=creative\nwhite-list=false\nserver-port=%s\n' "$PORT" > "$W/run/server.properties"
python3 -I - "$W/run/ops.json" PortalBot:4 <<'PY'
import hashlib, json, sys, uuid
def offline(name):
    b = bytearray(hashlib.md5(("OfflinePlayer:" + name).encode()).digest())
    b[6] = (b[6] & 0x0f) | 0x30
    b[8] = (b[8] & 0x3f) | 0x80
    return str(uuid.UUID(bytes=bytes(b)))
json.dump([{"uuid": offline("PortalBot"), "name": "PortalBot", "level": 4, "bypassesPlayerLimit": False}], open(sys.argv[1], "w"))
PY
start_server() {
    rm -f "$W/run/in.fifo" "$W/run/out.log"
    mkfifo "$W/run/in.fifo"
    (sleep 200 > "$W/run/in.fifo" &)
    (cd "$W/run" && setsid nohup "$W/firnmc-server" nogui --protocol "$PROTO" < in.fifo > out.log 2>&1 & echo $! > "$W/pid")
    for _ in $(seq 1 100); do grep -q 'Done (' "$W/run/out.log" 2>/dev/null && break; sleep 0.1; done
    grep -q 'Done (' "$W/run/out.log" || { echo "  FAIL  server start"; tail -n 5 "$W/run/out.log"; exit 1; }
}
stop_server() {
    echo "save-all" > "$W/run/in.fifo"
    sleep 2
    echo "stop" > "$W/run/in.fifo"
    sleep 2
    kill -9 "$(cat "$W/pid")" 2>/dev/null
}
BOT=tools/mc/wgen/portal_bot.cjs; [ "$PROTO" = 777 ] && BOT=tools/mc/wgen/portal_bot777.cjs
start_server
FRAME_X=10 node "$BOT" 127.0.0.1 "$PORT" > "$W/bot1.json" 2>"$W/bot1.err"
stop_server
SAVED=$(ls "$W/run/sw/DIM-1/region" 2>/dev/null | wc -l)
start_server
FRAME_X=80 node "$BOT" 127.0.0.1 "$PORT" > "$W/bot2.json" 2>"$W/bot2.err"
stop_server
node -e '
const fs = require("fs")
const a = JSON.parse(fs.readFileSync(process.argv[1], "utf8"))
const b = JSON.parse(fs.readFileSync(process.argv[2], "utf8"))
const saved = parseInt(process.argv[3], 10)
const errs = []
const want = ["login minecraft:overworld", "respawn minecraft:the_nether", "respawn minecraft:overworld"]
for (const [n, o] of [["first", a], ["second", b]]) {
  const got = o.events.filter((e) => /^(login|respawn)/.test(e))
  if (JSON.stringify(got) !== JSON.stringify(want)) errs.push(n + " events " + JSON.stringify(got))
  if (o.errors.length) errs.push(n + " errors " + JSON.stringify(o.errors))
  if (!o.nether) errs.push(n + " no nether arrival")
}
if (!saved) errs.push("no nether region files saved")
if (a.nether && b.nether) {
  const d = Math.hypot(a.nether.x - b.nether.x, a.nether.z - b.nether.z, a.nether.y - b.nether.y)
  if (d > 2) errs.push("second portal did not reuse the saved nether portal, distance " + d.toFixed(2) + " a=" + JSON.stringify(a.nether) + " b=" + JSON.stringify(b.nether))
}
if (errs.length) { console.log(errs.join("; ")); process.exit(1) }
' "$W/bot1.json" "$W/bot2.json" "$SAVED" && echo "  ok    saved nether portal found after restart" || { echo "  FAIL  portal save"; tail -n 5 "$W/run/out.log"; exit 1; }
echo "mc/wgen portal save: all ok"
