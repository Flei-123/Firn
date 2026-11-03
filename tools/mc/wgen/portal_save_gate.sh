#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
JARS=${MC_JARS:-/root/.w5/jars}
W=$(mktemp -d)
VPID=""; FPID=""
cleanup() { exec 3>&- 2>/dev/null; for p in $VPID $FPID; do kill "$p" 2>/dev/null; done; sleep 1; for p in $VPID $FPID; do kill -9 "$p" 2>/dev/null; done; rm -rf "$W"; }
trap cleanup EXIT
free_port() { python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
VPORT=$(free_port); FPORT=$(free_port)
OPS=$(python3 -I - <<'PY'
import hashlib, json, uuid
b = bytearray(hashlib.md5(b"OfflinePlayer:PortalBot").digest())
b[6] = (b[6] & 0x0f) | 0x30
b[8] = (b[8] & 0x3f) | 0x80
print(json.dumps([{"uuid": str(uuid.UUID(bytes=bytes(b))), "name": "PortalBot", "level": 4, "bypassesPlayerLimit": False}]))
PY
)
mkdir -p "$W/vanilla" "$W/firn"
printf 'online-mode=false\nserver-port=%s\nlevel-seed=11\nview-distance=3\nsimulation-distance=3\nspawn-protection=0\nspawn-monsters=false\nspawn-animals=false\nspawn-npcs=false\ngenerate-structures=false\ngamemode=creative\nforce-gamemode=true\nenforce-secure-profile=false\nwhite-list=false\n' "$VPORT" > "$W/vanilla/server.properties"
echo "eula=true" > "$W/vanilla/eula.txt"
echo "$OPS" > "$W/vanilla/ops.json"
echo "$OPS" > "$W/firn/ops.json"
"$FIRNC" ${WG_OPT:+--opt-level=$WG_OPT} -o "$W/firnmc-server" demos/firnmc-server/main.fi 2>"$W/err" || { echo "  FAIL  compile firnmc-server"; head -8 "$W/err"; exit 1; }
mkfifo "$W/vanilla/in"; exec 3<>"$W/vanilla/in"
( cd "$W/vanilla" && exec java -Xmx1G -jar "$JARS/server-1.20.1.jar" --nogui < "$W/vanilla/in" > server.out 2>&1 ) &
VPID=$!
for _ in $(seq 1 240); do grep -q 'Done (' "$W/vanilla/server.out" 2>/dev/null && break; sleep 1; done
grep -q 'Done (' "$W/vanilla/server.out" || { echo "  FAIL  vanilla server start"; tail -n 5 "$W/vanilla/server.out"; exit 1; }
FRAME_X=10 node tools/mc/wgen/portal_bot.cjs 127.0.0.1 "$VPORT" > "$W/bot1.json" 2>"$W/bot1.err"
echo "save-all flush" >&3
sleep 3
echo stop >&3
for _ in $(seq 1 60); do kill -0 "$VPID" 2>/dev/null || break; sleep 1; done
VPID=""
ls "$W/vanilla/world/DIM-1/region" > "$W/regions.txt" 2>&1
cp -r "$W/vanilla/world" "$W/firn/world"
rm -f "$W/firn/world/session.lock"
printf 'level-name=world\nview-distance=3\ngamemode=creative\nwhite-list=false\nserver-port=%s\n' "$FPORT" > "$W/firn/server.properties"
mkfifo "$W/firn/in.fifo"
(sleep 120 > "$W/firn/in.fifo" &)
(cd "$W/firn" && setsid nohup "$W/firnmc-server" nogui --protocol 763 < in.fifo > out.log 2>&1 & echo $! > "$W/fpid")
FPID=$(cat "$W/fpid")
for _ in $(seq 1 100); do grep -q 'Done (' "$W/firn/out.log" 2>/dev/null && break; sleep 0.2; done
grep -q 'Done (' "$W/firn/out.log" || { echo "  FAIL  firn server start"; tail -n 5 "$W/firn/out.log"; exit 1; }
grep -q 'non-persistent' "$W/firn/out.log" && { echo "  FAIL  saved world not loaded"; cat "$W/firn/out.log" | tail -n 5; exit 1; }
FRAME_X=80 node tools/mc/wgen/portal_bot.cjs 127.0.0.1 "$FPORT" > "$W/bot2.json" 2>"$W/bot2.err"
node -e '
const fs = require("fs")
const a = JSON.parse(fs.readFileSync(process.argv[1], "utf8"))
const b = JSON.parse(fs.readFileSync(process.argv[2], "utf8"))
const regions = fs.readFileSync(process.argv[3], "utf8").trim()
const errs = []
const want = ["login minecraft:overworld", "respawn minecraft:the_nether", "respawn minecraft:overworld"]
for (const [n, o] of [["vanilla", a], ["firn", b]]) {
  const got = o.events.filter((e) => /^(login|respawn)/.test(e))
  if (JSON.stringify(got) !== JSON.stringify(want)) errs.push(n + " events " + JSON.stringify(got))
  if (o.errors.length) errs.push(n + " errors " + JSON.stringify(o.errors))
  if (!o.nether) errs.push(n + " no nether arrival")
}
if (!/\.mca/.test(regions)) errs.push("vanilla saved no nether region: " + regions)
if (a.nether && b.nether) {
  const d = Math.hypot(a.nether.x - b.nether.x, a.nether.z - b.nether.z, a.nether.y - b.nether.y)
  if (d > 2) errs.push("firn did not reuse the saved vanilla nether portal, distance " + d.toFixed(2) + " vanilla=" + JSON.stringify(a.nether) + " firn=" + JSON.stringify(b.nether))
}
if (errs.length) { console.log(errs.join("; ")); process.exit(1) }
' "$W/bot1.json" "$W/bot2.json" "$W/regions.txt" && echo "  ok    firn finds the nether portal saved by vanilla after loading the saved world" || { echo "  FAIL  portal save"; tail -n 5 "$W/firn/out.log"; exit 1; }
echo "mc/wgen portal save: all ok"
