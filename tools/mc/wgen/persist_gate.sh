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
BOT=tools/mc/wgen/persist_bot.cjs
PORT=$(python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
"$FIRNC" ${WG_OPT:+--opt-level=$WG_OPT} -o "$W/firnmc-server" demos/firnmc-server/main.fi 2>"$W/err" || { echo "  FAIL  compile firnmc-server"; head -8 "$W/err"; exit 1; }
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

props() {
    printf 'level-type=minecraft:normal\nlevel-seed=%s\nlevel-name=gw\nworldgen-data=%s/tools/mcgen/worldgen/1.20.1\nview-distance=3\nwhite-list=false\nserver-port=%s\ngamemode=%s\n' "$1" "$ROOT" "$PORT" "$2" > "$W/run/server.properties"
}
stop_server() {
    echo "save-all" > "$W/run/in.fifo"
    sleep 2
    echo "stop" > "$W/run/in.fifo"
    sleep 2
    kill -9 "$(cat "$W/pid")" 2>/dev/null
}
FAILS=0
fail() { echo "  FAIL  $1"; FAILS=$((FAILS + 1)); }
ok() { echo "  ok    $1"; }
props "$SEED" creative

start_server
BEFORE=$(chunk_row 20)
echo "time set 6000" > "$W/run/in.fifo"
sleep 1
node "$BOT" 127.0.0.1 "$PORT" edit "$W/state.json" > "$W/edit.json" 2>"$W/edit.err"
sleep 1
node -e '
const s = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))
const bad = s.expect.filter((e, i) => s.after[i] !== e.block)
if (bad.length || s.errors.length) { console.log("bad " + JSON.stringify({ after: s.after, errors: s.errors })); process.exit(1) }
' "$W/state.json" > "$W/edit.chk" 2>&1 && ok "edit: dig + place applied live" || fail "edit: live state wrong: $(cat "$W/edit.chk")"
stop_server
[ -s "$W/run/gw/firn_world.txt" ] || fail "no world meta file"
[ -s "$W/run/gw/firn_edits.bin" ] && ok "firn_edits.bin written ($(stat -c %s "$W/run/gw/firn_edits.bin") bytes)" || fail "firn_edits.bin empty"
ls "$W/run/gw"/firn_player_*.bin >/dev/null 2>&1 && ok "player file written" || fail "player file missing"
[ -s "$W/run/gw/firn_clock.bin" ] && ok "clock file written" || fail "clock file missing"

props 999 survival
start_server
node "$BOT" 127.0.0.1 "$PORT" check "$W/state.json" > "$W/check.json" 2>"$W/check.err"
AFTER=$(chunk_row 20)
node -e '
const fs = require("fs")
const c = JSON.parse(fs.readFileSync(process.argv[1], "utf8"))
const s = c.state
const errs = []
c.after.forEach((a, i) => { if (a !== s.expect[i].block) errs.push("block " + i + " " + a) })
const near = (a, b) => Math.abs(a - b) < 1e-6
if (!near(c.pos.x, s.final_pos.x) || !near(c.pos.y, s.final_pos.y) || !near(c.pos.z, s.final_pos.z)) errs.push("pos " + JSON.stringify(c.pos) + " vs " + JSON.stringify(s.final_pos))
if (c.gameMode !== 1) errs.push("gamemode " + c.gameMode)
if (c.window[36] !== s.stone) errs.push("hotbar " + JSON.stringify(c.window))
if (c.placed !== "minecraft:stone") errs.push("place after restart " + c.placed)
if (!(c.time >= 6000 && c.time < 9000)) errs.push("time " + c.time)
if (errs.length) { console.log(errs.join("; ")); process.exit(1) }
' "$W/check.json" > "$W/check.chk" 2>&1 && ok "restart: blocks, position, gamemode, hotbar, time identical" || fail "restart: $(cat "$W/check.chk")"
if [ -n "$BEFORE" ] && [ "$BEFORE" = "$AFTER" ]; then ok "restart keeps saved seed (chunk row $BEFORE == $AFTER)"; else fail "seed: before=$(echo "$BEFORE" | wc -l) lines after=$(echo "$AFTER" | wc -l) lines; $(echo "$BEFORE" | head -c 40) / $(echo "$AFTER" | head -c 40)"; fi
stop_server
[ "$FAILS" = 0 ] && { echo "mc/wgen persist: all ok"; exit 0; }
exit 1
