#!/usr/bin/env bash
# tools/mc/server/rs_run.sh [stage1|stage2] -- redstone vs vanilla 1.20.1: creative bot builds circuits and operates them; block_change sequences and tick timing compared. stage1: lever/wire/lamp/repeater/torch; stage2: buttons/observer/comparator/redstone block.
set -uo pipefail
cd "$(dirname "$0")/../../.."
REPO=$(pwd)
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$REPO/lib"
JARS=${MC_JARS:-/root/.w5/jars}
TOOLS="$REPO/tools/mc/server"
STAGE=${1:-stage1}
case "$STAGE" in stage1) BOT=bot_rs.cjs; CHECK=check_rs.cjs ;; stage2) BOT=bot_rs2.cjs; CHECK=check_rs2.cjs ;; *) echo "usage: rs_run.sh [stage1|stage2]"; exit 2 ;; esac
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
gamemode=creative
force-gamemode=true
PROPS
echo "eula=true" > "$W/vanilla/eula.txt"
mkfifo "$W/vanilla/in"; exec 3<>"$W/vanilla/in"
( cd "$W/vanilla" && exec java -Xmx1G -jar "$JARS/server-1.20.1.jar" --nogui < "$W/vanilla/in" > server.out 2>&1 ) &
VPID=$!
$FIRNC -o "$W/mcsrv" demos/mcsrv/main.fi 2>"$W/err" || { echo "  FAIL  compile mcsrv"; head -5 "$W/err"; exit 1; }
for i in $(seq 1 120); do grep -q 'Done (' "$W/vanilla/server.out" 2>/dev/null && break; sleep 1; done
grep -q 'Done (' "$W/vanilla/server.out" || { echo "  FAIL  vanilla server did not start"; exit 1; }
"$W/mcsrv" --port "$FPORT" --mode 1 --ticks 3000 > "$W/f.out" 2>&1 &
FPID=$!
sleep 2
node "$TOOLS/$BOT" 127.0.0.1 "$VPORT" > "$W/vanilla.json" 2>"$W/vbot.err"
node "$TOOLS/$BOT" 127.0.0.1 "$FPORT" > "$W/firn.json" 2>"$W/fbot.err"
echo "vanilla:"; node "$TOOLS/$CHECK" "$W/vanilla.json" || ERR=1
echo "firn vs vanilla:"; node "$TOOLS/$CHECK" "$W/firn.json" "$W/vanilla.json" || ERR=1
echo "negative control (wrong expectation must fail):"
if node "$TOOLS/$CHECK" "$W/firn.json" - wrong >/dev/null; then echo "  FAIL  wrong expectation not detected"; ERR=1; else echo "  ok    wrong expectation detected"; fi
echo stop >&3
wait $VPID 2>/dev/null
[ -n "${RS_KEEP:-}" ] && mkdir -p "$RS_KEEP" && cp "$W"/*.json "$W"/*.err "$W"/f.out "$RS_KEEP"/ 2>/dev/null
exit $ERR
