#!/usr/bin/env bash
# tools/mc/server/inv_run.sh -- inventory clicks of the Firn server (protocol 763) against the REAL vanilla 1.20.1 server with the same node bot:
#   every scenario (pickup, split, merge, swap, shift-click, hotbar swap, offhand key, clone, throw, drag, double click, crafting 2x2 incl. result slot,
#   armor slots, offhand) is run in creative and in survival; the full inventory + cursor after the clicks must be identical.
# Env: INV_ONLY=name,name  INV_MODES=creative,survival  INV_KEEP=<dir>
set -uo pipefail
cd "$(dirname "$0")/../../.."
REPO=$(pwd)
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$REPO/lib"
JARS=${MC_JARS:-/root/.w5/jars}
TOOLS="$REPO/tools/mc/server"
W=$(mktemp -d)
VPID=""; FPID=""
cleanup() { exec 3>&- 2>/dev/null; for p in $VPID $FPID; do kill "$p" 2>/dev/null; done; sleep 1; for p in $VPID $FPID; do kill -9 "$p" 2>/dev/null; done; rm -rf "$W"; }
trap cleanup EXIT
ERR=0
free_port() { python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
VPORT=$(free_port); FPORT=$(free_port)
mkdir -p "$W/vanilla"
cat > "$W/vanilla/server.properties" <<PROPS
online-mode=false
server-port=$VPORT
level-type=flat
view-distance=4
simulation-distance=2
spawn-protection=0
enforce-secure-profile=false
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
generate-structures=false
gamemode=creative
max-tick-time=-1
PROPS
echo "eula=true" > "$W/vanilla/eula.txt"
python3 -I - "$W/vanilla/ops.json" InvVan:4 InvFirn:4 <<'PY'
import hashlib, json, sys, uuid
def offline(name):
    b = bytearray(hashlib.md5(("OfflinePlayer:" + name).encode()).digest())
    b[6] = (b[6] & 0x0f) | 0x30
    b[8] = (b[8] & 0x3f) | 0x80
    return str(uuid.UUID(bytes=bytes(b)))
json.dump([{"uuid": offline(n), "name": n, "level": int(l), "bypassesPlayerLimit": False} for n, l in (a.split(":") for a in sys.argv[2:])], open(sys.argv[1], "w"))
PY
mkfifo "$W/vanilla/in"; exec 3<>"$W/vanilla/in"
( cd "$W/vanilla" && exec java -Xmx1G -jar "$JARS/server-1.20.1.jar" --nogui < "$W/vanilla/in" > server.out 2>&1 ) &
VPID=$!
$FIRNC -o "$W/mcsrv" demos/mcsrv/main.fi 2>"$W/err" || { echo "  FAIL  compile mcsrv"; head -5 "$W/err"; exit 1; }
for i in $(seq 1 120); do grep -q 'Done (' "$W/vanilla/server.out" 2>/dev/null && break; sleep 1; done
grep -q 'Done (' "$W/vanilla/server.out" || { echo "  FAIL  vanilla server did not start"; exit 1; }
"$W/mcsrv" --port "$FPORT" --mode 1 --ticks 1000000 > "$W/f.out" 2>&1 &
FPID=$!
sleep 2
node "$TOOLS/bot_inv.cjs" 127.0.0.1 "$VPORT" InvVan "$W/vanilla.json" 2>"$W/vbot.err" &
B1=$!
node "$TOOLS/bot_inv.cjs" 127.0.0.1 "$FPORT" InvFirn "$W/firn.json" 2>"$W/fbot.err" &
B2=$!
wait $B1 $B2
[ -f "$W/vanilla.json" ] || { echo "  FAIL  vanilla bot produced no result"; head -5 "$W/vbot.err"; ERR=1; }
[ -f "$W/firn.json" ] || { echo "  FAIL  firn bot produced no result"; head -5 "$W/fbot.err"; ERR=1; }
if [ $ERR -eq 0 ]; then
    node "$TOOLS/check_inv.cjs" "$W/vanilla.json" "$W/vanilla.json" vanilla || ERR=1
    node "$TOOLS/check_inv.cjs" "$W/vanilla.json" "$W/firn.json" firn-763 || ERR=1
    python3 -I - "$W/firn.json" "$W/mut.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
for mode in d["modes"]:
    for r in d["modes"][mode][:1]:
        r["cursor"] = "stone*99"
json.dump(d, open(sys.argv[2], "w"))
PY
    if node "$TOOLS/check_inv.cjs" "$W/vanilla.json" "$W/mut.json" mutated expect-diff >/dev/null; then echo "  ok    counter-check: a mutated result is detected"; else echo "  FAIL  counter-check did not detect the mutation"; ERR=1; fi
fi
[ -n "${INV_KEEP:-}" ] && mkdir -p "$INV_KEEP" && cp "$W"/*.json "$W"/*.err "$W"/f.out "$W/vanilla/server.out" "$INV_KEEP"/ 2>/dev/null
echo stop >&3
wait $VPID 2>/dev/null
if [ $ERR -eq 0 ]; then echo "RESULT mc-server-inv: ok"; else echo "RESULT mc-server-inv: FAIL"; fi
exit $ERR
