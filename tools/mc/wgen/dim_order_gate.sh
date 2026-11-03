#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
PROTO=${DIM_PROTO:-763}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
JARS=${MC_JARS:-/root/.w5/jars}
JAVA=java
JAR="$JARS/server-1.20.1.jar"
BOT=tools/mc/wgen/pkt_bot.cjs
if [ "$PROTO" = 777 ]; then JAVA=/root/tools/jdk25/bin/java; JAR="$JARS/server-26.3.jar"; BOT=tools/mc/wgen/pkt_bot777.cjs; fi
W=$(mktemp -d)
VPID=""; FPID=""
cleanup() { exec 3>&- 2>/dev/null; for p in $VPID $FPID; do kill "$p" 2>/dev/null; done; sleep 1; for p in $VPID $FPID; do kill -9 "$p" 2>/dev/null; done; [ -n "${ORDER_KEEP:-}" ] && mkdir -p "$ORDER_KEEP" && cp "$W"/*.json "$W"/*.log "$W"/*.err "$ORDER_KEEP"/ 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
free_port() { python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
VPORT=$(free_port); FPORT=$(free_port)
OPS=$(python3 -I - <<'PY'
import hashlib, json, uuid
b = bytearray(hashlib.md5(b"OfflinePlayer:DimBot").digest())
b[6] = (b[6] & 0x0f) | 0x30
b[8] = (b[8] & 0x3f) | 0x80
print(json.dumps([{"uuid": str(uuid.UUID(bytes=bytes(b))), "name": "DimBot", "level": 4, "bypassesPlayerLimit": False}]))
PY
)
mkdir -p "$W/vanilla" "$W/firn"
printf 'online-mode=false\nserver-port=%s\nlevel-seed=11\nview-distance=3\nsimulation-distance=3\nspawn-protection=0\nspawn-monsters=false\nspawn-animals=false\nspawn-npcs=false\ngenerate-structures=false\ngamemode=creative\nforce-gamemode=true\nenforce-secure-profile=false\nwhite-list=false\n' "$VPORT" > "$W/vanilla/server.properties"
echo "eula=true" > "$W/vanilla/eula.txt"
echo "$OPS" > "$W/vanilla/ops.json"
printf 'level-type=minecraft:normal\nlevel-seed=11\nlevel-name=gw\nworldgen-data=%s/tools/mcgen/worldgen/1.20.1\nview-distance=3\nwhite-list=false\ngamemode=creative\nserver-port=%s\n' "$ROOT" "$FPORT" > "$W/firn/server.properties"
echo "$OPS" > "$W/firn/ops.json"
mkfifo "$W/vanilla/in"; exec 3<>"$W/vanilla/in"
( cd "$W/vanilla" && exec "$JAVA" -Xmx1G -jar "$JAR" --nogui < "$W/vanilla/in" > server.out 2>&1 ) &
VPID=$!
"$FIRNC" ${WG_OPT:+--opt-level=$WG_OPT} -o "$W/firnmc-server" demos/firnmc-server/main.fi 2>"$W/err" || { echo "  FAIL  compile firnmc-server"; head -8 "$W/err"; exit 1; }
mkfifo "$W/firn/in.fifo"
(sleep 400 > "$W/firn/in.fifo" &)
(cd "$W/firn" && setsid nohup "$W/firnmc-server" nogui --protocol "$PROTO" < in.fifo > out.log 2>&1 & echo $! > "$W/fpid")
FPID=$(cat "$W/fpid")
for _ in $(seq 1 180); do grep -q 'Done (' "$W/vanilla/server.out" 2>/dev/null && break; sleep 1; done
grep -q 'Done (' "$W/vanilla/server.out" || { echo "  FAIL  vanilla server start"; tail -n 5 "$W/vanilla/server.out"; exit 1; }
grep -q 'Done (' "$W/firn/out.log" 2>/dev/null || sleep 3
node "$BOT" 127.0.0.1 "$VPORT" > "$W/vanilla.json" 2>"$W/vbot.err"
node "$BOT" 127.0.0.1 "$FPORT" > "$W/firn.json" 2>"$W/fbot.err"
PORTAL_RC=0
if [ "$PROTO" = 763 ]; then
    PCMDS='["tp @s 10.5 150 3.5","raw:frame","raw:ignite","raw:enter","raw:leave"]'
    DIM_CMDS=$PCMDS node "$BOT" 127.0.0.1 "$VPORT" > "$W/vanilla_p.json" 2>"$W/vbot_p.err"
    DIM_CMDS=$PCMDS node "$BOT" 127.0.0.1 "$FPORT" > "$W/firn_p.json" 2>"$W/fbot_p.err"
fi
echo stop >&3
node tools/mc/wgen/pkt_cmp.cjs "$W/vanilla.json" "$W/firn.json" || PORTAL_RC=1
if [ "$PROTO" = 763 ]; then node tools/mc/wgen/pkt_cmp.cjs "$W/vanilla_p.json" "$W/firn_p.json" || PORTAL_RC=1; fi
[ "$PORTAL_RC" = 0 ] && echo "mc/wgen dim order $PROTO: ok" || { echo "  FAIL  dim order"; exit 1; }
