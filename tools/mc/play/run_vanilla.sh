#!/usr/bin/env bash
# tools/mc/play/run_vanilla.sh -- mcplay with the vanilla resource pack default: the client jar (MC_VANILLA_JAR, fetched by tools/mcclient/fetch_client.sh into a
# throw-away dir, never committed) is turned into a cache ZIP at run time (MC_VANILLA_CACHE), mcplay loads it as the lowest pack and switches models on.
# Runs: V1 vanilla default, V2 MC_VANILLA=0 (built-in look), V3 no jar (fallback log line), V4 second start (cache reused). Prints `RESULT mc-play-vanilla: ok|FAIL`.
# Env: FIRNC, VANILLA_JAR_CACHE (dir that keeps client-<ver>.jar, default /root/.w5/jars), MODELS_RUN_S. Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
MCV=1.20.1
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${MODELS_KEEP_DIR:-}" ] && { mkdir -p "$MODELS_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W/server.log" "$MODELS_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { [ "${ERR:-0}" -ne 0 ] && keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-vanilla: FAIL"; exit 1; }
for t in Xvfb xdotool java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
python3 -I -c "import PIL" 2>/dev/null || fail "missing python module: PIL"
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
mkdir -p "$W/srv"
bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=creative
force-gamemode=true
level-type=flat
view-distance=4
simulation-distance=4
spawn-protection=0
difficulty=peaceful
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
enable-rcon=false
enable-query=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "${JAVA:-/usr/bin/java}" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "spawnRadius 0" "randomTickSpeed 0"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
# the four stairs (one per facing, row at z = 5) are set on the SERVER before any client joins: every run then gets them with the chunk data (r244:
# stairs that arrive as block updates after the join give a different picture on ~10 % of the grass (+-1 level) than stairs loaded with the chunk,
# in the inline path too -- the first run on a fresh world used to differ from all the others) and the spawn is fixed (spawnRadius 0)
NSB=$(wc -l < "$W/server.log")
for spec in "-2 east" "0 south" "2 west" "4 north"; do
    set -- $spec
    echo "setblock $1 -60 5 oak_stairs[facing=$2]" >&3
done
for i in $(seq 1 30); do [ "$(tail -n +"$((NSB + 1))" "$W/server.log" | grep -c 'Changed the block')" -ge 4 ] && break; sleep 1; done
[ "$(tail -n +"$((NSB + 1))" "$W/server.log" | grep -c 'Changed the block')" -ge 4 ] || fail "server setblock of the stairs"
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; sleep 1; done; return 1; }
# One run: join (awaited in the log), four stairs (one per facing) in a row 5 blocks in front of the player, who looks south and slightly down.
# $2 = 1: press T (pack reload) afterwards. A lost join / tp is an environment flake: the attempt is repeated (own player name), up to 3 times.
run_one() { # name, reload, env...
    local name=$1 reload=$2; shift 2
    local att pn pp rc ok nl
    for att in 1 2 3; do
        pn="M${name}${att}"; ok=1; rm -f "$W/$name.png"
        env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_RUN_S=${MODELS_RUN_S:-40} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 \
            MC_PACKS="$W/pack.zip" MCPLAY_SHOT="$W/$name.png" "$@" "$W/mcplay" 127.0.0.1 "$PORT" "$pn" > "$W/$name.out" 2>&1 &
        pp=$!
        if wait_log "$pn joined the game" 90; then
            sleep 2
            xdotool mousemove 320 180 2>/dev/null
            nl=$(wc -l < "$W/server.log")
            echo "tp $pn 0.5 -60 0.5 0 20" >&3
            wait_log "Teleported $pn to" 10 "$nl" || ok=0
            sleep 4
            if [ "$reload" = 1 ]; then xdotool key F5; sleep 6; fi
        else
            ok=0
        fi
        wait $pp; rc=$?
        [ $rc -eq 0 ] || { echo "  FAIL  $name exit code $rc"; ERR=1; }
        [ $ok -eq 1 ] && break
        echo "  retry $name (attempt $att): join / tp lost"
    done
    [ $ok -eq 1 ] || { echo "  FAIL  $name: join / tp failed 3 times"; ERR=1; }
    echo "  $name player=$pn"
}

MCCLIENT_JAR_CACHE=${VANILLA_JAR_CACHE:-/root/.w5/jars} bash tools/mcclient/fetch_client.sh "$W/jar" "$MCV" >/dev/null 2>&1 || fail "client jar download"
export MC_VANILLA_CACHE="$W/vcache"
run_one V1 0 MC_PACKS= MC_VANILLA=1 MC_VANILLA_JAR="$W/jar/client.jar"
run_one V2 0 MC_PACKS= MC_VANILLA=0 MC_VANILLA_JAR="$W/jar/client.jar"
run_one V3 0 MC_PACKS= MC_VANILLA=1 MC_VANILLA_JAR= MC_VANILLA_DIR="$W/nodir" HOME="$W/nohome" APPDATA=
CF=$(ls "$W"/vcache/vanilla-*.zip 2>/dev/null | head -1); T1=$(stat -c %Y "$CF" 2>/dev/null)
run_one V4 0 MC_PACKS= MC_VANILLA=1 MC_VANILLA_JAR="$W/jar/client.jar"
T2=$(stat -c %Y "$CF" 2>/dev/null)
[ -n "$CF" ] || fail "no cache zip written"
grep -q "^mcplay: vanilla resources from" "$W/V1.out" || { echo "  FAIL  V1: no 'vanilla resources from' line"; ERR=1; }
grep -q "^mcplay: models on" "$W/V1.out" || { echo "  FAIL  V1: models not on by default"; ERR=1; }
grep -q "^mcplay: vanilla resources" "$W/V2.out" && { echo "  FAIL  V2: vanilla loaded although MC_VANILLA=0"; ERR=1; }
grep -q "^mcplay: vanilla resources unavailable" "$W/V3.out" || { echo "  FAIL  V3: no fallback line"; ERR=1; }
[ "$T1" = "$T2" ] || { echo "  FAIL  cache rewritten on the second start"; ERR=1; }
grep -q "^mcplay: vanilla resources from" "$W/V4.out" || { echo "  FAIL  V4: vanilla not loaded from cache"; ERR=1; }
MS=$(grep -E "^mcplay: models states=" "$W/V1.out" | tail -1); echo "  V1 $MS"
U=$(echo "$MS" | grep -o "unmapped=[0-9]*" | cut -d= -f2); [ "${U:-1}" = 0 ] || { echo "  FAIL  V1: unmapped model textures: ${U:-?}"; ERR=1; }
python3 -I - "$W/V1.png" "$W/V2.png" "$W/V3.png" <<'PY' || ERR=1
import sys
from PIL import Image, ImageChops
a, b, c = (Image.open(p).convert("RGB") for p in sys.argv[1:4])
def diff(x, y):
    d = ImageChops.difference(x, y).convert("L").point(lambda v: 255 if v > 24 else 0)
    return sum(1 for v in d.getdata() if v) / (x.width * x.height)
v12 = diff(a, b)
v23 = diff(b, c)
print("  vanilla vs built-in: %.1f %% of pixels differ; built-in vs fallback: %.2f %%" % (100 * v12, 100 * v23))
if v12 < 0.05 or v23 > 0.005:
    print("  FAIL  picture check"); sys.exit(1)
PY
[ "${ERR:-0}" -eq 0 ] && echo "RESULT mc-play-vanilla: ok" || { echo "RESULT mc-play-vanilla: FAIL"; exit 1; }
