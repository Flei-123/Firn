#!/usr/bin/env bash
# tools/mc/server/ent_run.sh -- entities vs vanilla 1.20.1: two bots, survival flat world; one digs grass, the observer sees the item entity
#   spawn/metadata/motion/collect/destroy and the other player; digger gets the slot. Same checker runs against vanilla and Firn; wrong expectation is detected.
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
view-distance=8
simulation-distance=4
spawn-protection=0
enforce-secure-profile=false
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
generate-structures=false
gamemode=survival
force-gamemode=true
PROPS
echo "eula=true" > "$W/vanilla/eula.txt"
mkfifo "$W/vanilla/in"; exec 3<>"$W/vanilla/in"
( cd "$W/vanilla" && exec java -Xmx1G -jar "$JARS/server-1.20.1.jar" --nogui < "$W/vanilla/in" > server.out 2>&1 ) &
VPID=$!
$FIRNC -o "$W/mcsrv" demos/mcsrv/main.fi 2>"$W/err" || { echo "  FAIL  compile mcsrv"; head -5 "$W/err"; exit 1; }
for i in $(seq 1 120); do grep -q 'Done (' "$W/vanilla/server.out" 2>/dev/null && break; sleep 1; done
grep -q 'Done (' "$W/vanilla/server.out" || { echo "  FAIL  vanilla server did not start"; exit 1; }
"$W/mcsrv" --port "$FPORT" --mode 0 --ticks 1200 > "$W/f.out" 2>&1 &
FPID=$!
sleep 2
node "$TOOLS/bot_ent.cjs" 127.0.0.1 "$VPORT" > "$W/vanilla.json" 2>"$W/vbot.err"
node "$TOOLS/bot_ent.cjs" 127.0.0.1 "$FPORT" > "$W/firn.json" 2>"$W/fbot.err"
echo "vanilla:"; node "$TOOLS/check_ent.cjs" "$W/vanilla.json" dirt || ERR=1
echo "firn:"; node "$TOOLS/check_ent.cjs" "$W/firn.json" dirt || ERR=1
echo "negative control (wrong item must fail):"
if node "$TOOLS/check_ent.cjs" "$W/firn.json" cobblestone >/dev/null; then echo "  FAIL  wrong expectation not detected"; ERR=1; else echo "  ok    wrong expectation detected"; fi
echo stop >&3
wait $VPID 2>/dev/null
[ -n "${ENT_KEEP:-}" ] && mkdir -p "$ENT_KEEP" && cp "$W"/*.json "$W"/*.err "$W"/f.out "$ENT_KEEP"/ 2>/dev/null
exit $ERR
