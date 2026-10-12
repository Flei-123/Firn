#!/usr/bin/env bash
# tools/mc/audio/sounds_live.sh [out.jsonl] -- joins a REAL vanilla server (MC_VERSION, default 1.20.1, offline mode, 127.0.0.1) with
# a listener (sound_cap.cjs) and lets the server console send sounds (playsound, stopsound, block destroy). Writes one JSON line per
# sound packet (wire id, body hex, nmp's parse). Everything lives in one mktemp dir; the server is stopped on exit.
# Heavy (JVM): /root/jarvis/bin/heavy bash tools/mc/audio/sounds_live.sh <out>
# Clean room: the jar is a black box test subject (tools/mcclient/fetch_server.sh), nothing of it is read.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT=${1:-/dev/stdout}
MCV=${MC_VERSION:-1.20.1}
JAVA=${JAVA:-/usr/bin/java}
NMP=$(python3 -I -c 'import json,sys;print(json.load(open(sys.argv[1]))["versions"][sys.argv[2]]["nmp"])' "$HERE/../../mcgen/versions.json" "$MCV") || { echo "unknown MC_VERSION $MCV" >&2; exit 2; }
export NODE_PATH="${NODE_PATH:-/root/firn/tools/mcserver/node_modules}"
export MCCLIENT_JAR_CACHE=${MCCLIENT_JAR_CACHE:-/root/.w5/jars}
W=$(mktemp -d); exec 3<>/dev/null
cleanup() { [ -n "${CAP:-}" ] && kill "$CAP" 2>/dev/null; echo stop >&3 2>/dev/null; [ -n "${SRV:-}" ] && { sleep 2; kill "$SRV" 2>/dev/null; }; rm -rf "$W"; }
trap cleanup EXIT
die() { echo "sounds_live: $*" >&2; exit 2; }
mkdir -p "$W/srv"
bash "$HERE/../../mcclient/fetch_server.sh" "$W/srv" "$MCV" >&2 || die "server download failed"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(node -e 'const s=require("net").createServer();s.listen(0,"127.0.0.1",()=>{console.log(s.address().port);s.close()})')
cat > "$W/srv/server.properties" <<EOP
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
level-seed=4242
view-distance=3
simulation-distance=3
spawn-protection=0
enable-rcon=false
network-compression-threshold=256
player-idle-timeout=0
allow-flight=true
EOP
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 $SRV 2>/dev/null || die "server died"; sleep 1; done
grep -q 'Done (' "$W/server.log" || die "server start timeout"
for g in "doMobSpawning false" "doDaylightCycle false" "doWeatherCycle false"; do echo "gamerule $g" >&3; done
mkfifo "$W/capin"; exec 4<>"$W/capin"
( exec node "$HERE/sound_cap.cjs" 127.0.0.1 "$PORT" Listener "$NMP" < "$W/capin" > "$W/cap.log" 2> "$W/cap.err" ) &
CAP=$!
for i in $(seq 1 60); do grep -q '^ready' "$W/cap.log" 2>/dev/null && break; sleep 0.5; done
grep -q '^ready' "$W/cap.log" || { cat "$W/cap.err" >&2; die "listener did not join"; }
sleep 3
c() { echo "$1" >&3; sleep 0.6; }
c "execute at Listener run playsound minecraft:ambient.cave ambient Listener ~ ~ ~ 1 1 0"
c "execute at Listener run playsound minecraft:block.stone.break block Listener ~4 ~ ~-2 0.75 0.5 0"
c "execute at Listener run playsound minecraft:entity.pig.ambient neutral Listener ~ ~ ~ 1 2 0"
c "execute at Listener run playsound minecraft:block.note_block.harp record Listener ~1 ~2 ~3 3 1.5"
c "execute at Listener run playsound minecraft:entity.zombie.hurt hostile Listener ~-8 ~ ~ 0.25 0.8"
c "execute at Listener run playsound minecraft:ui.button.click master Listener ~ ~ ~ 1 1"
c "execute at Listener run playsound custom:not.in.registry master Listener ~ ~ ~ 1 1"
c "execute at Listener run setblock ~3 ~-1 ~ minecraft:stone"
c "execute at Listener run setblock ~3 ~-1 ~ minecraft:air destroy"
c "execute at Listener run summon minecraft:pig ~ ~ ~5 {NoAI:1b}"
c "execute at Listener run summon minecraft:chicken ~2 ~ ~5"
sleep 6
c "kill @e[type=minecraft:pig]"
c "stopsound Listener * minecraft:ambient.cave"
c "stopsound Listener block"
c "stopsound Listener master minecraft:ui.button.click"
c "stopsound Listener *"
sleep 2
echo stop >&4
grep -v '^ready' "$W/cap.log" > "$OUT"
