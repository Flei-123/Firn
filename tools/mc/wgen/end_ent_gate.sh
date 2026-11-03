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
printf 'level-type=minecraft:end\nlevel-seed=5\nlevel-name=gw\nworldgen-data=%s/tools/mcgen/worldgen/1.20.1\nview-distance=6\nwhite-list=false\nserver-port=%s\n' "$ROOT" "$PORT" > "$W/run/server.properties"
start() {
    rm -f "$W/run/in.fifo"; mkfifo "$W/run/in.fifo"
    (sleep 60 > "$W/run/in.fifo" &)
    (cd "$W/run" && setsid nohup "$W/firnmc-server" nogui --protocol 763 < in.fifo > out.log 2>&1 & echo $! > "$W/pid")
    for _ in $(seq 1 100); do grep -q 'Done (' "$W/run/out.log" 2>/dev/null && break; sleep 0.1; done
    grep -q 'Done (' "$W/run/out.log" || { echo "  FAIL  server start"; tail -n 5 "$W/run/out.log"; exit 1; }
}
start
node tools/mc/wgen/end_ent_bot.cjs 127.0.0.1 "$PORT" > "$W/bot1.json" 2>"$W/bot.err"
echo stop > "$W/run/in.fifo"
for _ in $(seq 1 50); do kill -0 "$(cat "$W/pid")" 2>/dev/null || break; sleep 0.2; done
: > "$W/run/out.log"
start
node tools/mc/wgen/end_ent_bot.cjs 127.0.0.1 "$PORT" > "$W/bot2.json" 2>>"$W/bot.err"
node -e '
const a = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))
const b = JSON.parse(require("fs").readFileSync(process.argv[2], "utf8"))
const want = [[-41.5,-0.5],[-34,0],[-12.5,-39.5],[-12.5,39.5],[12.5,-39.5],[12.5,39.5],[33.5,-24.5],[33.5,24.5],[42.5,0.5]]
const errs = []
if (a.crystals.length !== 10) errs.push("crystals " + a.crystals.length)
if (b.crystals.length !== 10) errs.push("crystals after restart " + b.crystals.length)
if (JSON.stringify(a.crystals) !== JSON.stringify(b.crystals)) errs.push("crystals differ after restart")
for (const c of a.crystals) if (Math.abs(c.y - Math.round(c.y)) > 1e-6 || c.y < 77 || c.y > 106) errs.push("crystal y " + c.y)
if (a.errors.length) errs.push("errors " + JSON.stringify(a.errors))
if (errs.length) { console.log(errs.join("; ")); console.log(JSON.stringify(a)); process.exit(1) }
console.log("  gateways seen: " + a.gateways.length)
' "$W/bot1.json" "$W/bot2.json" && echo "  ok    end crystals: 10 spawned on spikes, persisted over restart" || { echo "  FAIL  end crystals"; tail -n 5 "$W/run/out.log"; exit 1; }
echo "mc/wgen end_ent: all ok"
