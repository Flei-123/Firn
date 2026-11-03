#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
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
(cd "$W/run" && setsid nohup "$W/firnmc-server" nogui --protocol 763 < in.fifo > out.log 2>&1 & echo $! > "$W/pid")
for _ in $(seq 1 100); do grep -q 'Done (' "$W/run/out.log" 2>/dev/null && break; sleep 0.1; done
grep -q 'Done (' "$W/run/out.log" || { echo "  FAIL  server start"; tail -n 5 "$W/run/out.log"; exit 1; }
node tools/mc/wgen/dim_bot.cjs 127.0.0.1 "$PORT" > "$W/bot.json" 2>"$W/bot.err"
node -e '
const o = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))
const want = ["login minecraft:overworld", "respawn minecraft:the_nether", "respawn minecraft:overworld", "respawn minecraft:the_end", "respawn minecraft:the_nether"]
const got = o.events.filter((e) => /^(login|respawn)/.test(e))
const errs = []
if (JSON.stringify(got) !== JSON.stringify(want)) errs.push("events " + JSON.stringify(got))
for (const d of ["minecraft:the_nether", "minecraft:the_end"]) if (!(o.chunks[d] >= 9)) errs.push("chunks " + d + " " + o.chunks[d])
if (o.errors.length) errs.push("errors " + JSON.stringify(o.errors))
if (errs.length) { console.log(errs.join("; ")); console.log(JSON.stringify(o)); process.exit(1) }
' "$W/bot.json" && echo "  ok    dimension switch: nether, overworld, end, nether via execute in + tp" || { echo "  FAIL  dimension switch"; tail -n 5 "$W/run/out.log"; exit 1; }
echo "mc/wgen dim: all ok"
