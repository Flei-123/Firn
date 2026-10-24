#!/usr/bin/env bash
# tools/mc/play/run_lod.sh -- mcplay with MCPLAY_LOD (mc_stream + mc_lod rings) against the official 1.20.1 server: Xvfb + server + mcplay + xdotool.
# Run A: MCPLAY_VIEW=3 MCPLAY_LOD=9 (3 chunks full detail, the rest LOD; the vanilla server sends its own view distance either way), walks 3 s, screenshot.
# Run B: MCPLAY_VIEW=3 (stream is the default, no LOD): control + worker pool + window sync (server radius > view + 2). Prints `RESULT mc-play-lod: ok`.
# The player flies (F) at y=100 over the terrain: only from up there the LOD ring is visible (r194: a player that fell into a hillside saw stone in both
# runs, the old `cmp` passed only through the FPS number in the HUD). Join is awaited in the server log, tp is confirmed ("Teleported"), the end position
# line of mcplay must be y >= 80, a lost tp repeats the attempt (3x).
# Checks: exit 0, frames, A has LOD columns drawn (lod_cols > 0) and LOD triangles, B none, the 3D view (without HUD) differs in >= 1 % of the pixels.
# Env: MC_VIEW_SRV (10 = server view distance), FIRNC, MCPLAY_PNG_OUT (copy of the A screenshot), LOD_KEEP_DIR (copy of outs/pngs/server.log, always), LOD_RUN_S (45 = run time per mcplay). Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${LOD_KEEP_DIR:-}" ] && { mkdir -p "$LOD_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W/server.log" "$LOD_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { [ "${ERR:-0}" -ne 0 ] && keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-lod: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
MCV=1.20.1; SV=${MC_VIEW_SRV:-10}
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
level-seed=4242
view-distance=$SV
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
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
# wait (up to $2 s) until server.log has a line matching the regex $1 (after line $3, default 0); 0 = found
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; sleep 1; done; return 1; }
# One run. The picture is only meaningful when the player is really UP in the air looking over the terrain (r194): a player that fell into a
# hillside sees a flat stone wall in both runs and the A/B pictures are equal. So: wait for the join (log, not a fixed sleep: under load the
# client needs > 6 s), toggle fly once (F), tp, wait for the server's "Teleported", and at the end check the position line of mcplay. A lost tp
# (no teleport confirmed, or the player is not high at the end) = environment flake: that attempt is repeated (own player name), up to 3 times.
run_one() { # name, env...
    local name=$1; shift
    local att pn pp rc ok py
    for att in 1 2 3; do
        pn="P${name}${att}"; ok=1; rm -f "$W/$name.png"
        env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_RUN_S=${LOD_RUN_S:-45} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/$name.png" "$@" \
            "$W/mcplay" 127.0.0.1 "$PORT" "$pn" > "$W/$name.out" 2>&1 &
        pp=$!
        if wait_log "$pn joined the game" 90; then
            sleep 3
            xdotool mousemove 320 180 2>/dev/null
            xdotool key f
            local tries=0 nl
            while [ $tries -lt 4 ]; do
                nl=$(wc -l < "$W/server.log")
                echo "tp $pn 0.5 100 0.5" >&3
                wait_log "Teleported $pn to" 10 "$nl" && break
                tries=$((tries + 1))
            done
            [ $tries -lt 4 ] || ok=0
            sleep 2
            xdotool keydown w; sleep 2; xdotool keyup w
        else
            ok=0
        fi
        wait $pp; rc=$?
        py=$(grep -E "^mcplay: pos " "$W/$name.out" | tail -1 | grep -o " y=-\?[0-9]*" | cut -d= -f2)
        [ -n "$py" ] && [ "$py" -ge 80 ] 2>/dev/null || ok=0
        [ $rc -eq 0 ] || { echo "  FAIL  $name exit code $rc"; ERR=1; }
        [ $ok -eq 1 ] && break
        echo "  retry $name (attempt $att): player not up (tp lost or no join; end y=${py:-?})"
    done
    [ $ok -eq 1 ] || { echo "  FAIL  $name: player never got up after 3 attempts (end y=${py:-?})"; ERR=1; }
    echo "  $name player=$pn end y=${py:-?}"
    sed 's/^/  /' "$W/$name.out" | grep -E "stream|end frames" | tail -3
}
run_one A MCPLAY_VIEW=3 MCPLAY_LOD=9
run_one B MCPLAY_VIEW=3
run_one C MCPLAY_VIEW=12
run_one D MCPLAY_VIEW=12 MCPLAY_LOD=0
# only the `mcplay: end` / `mcplay: stream` lines: `mcplay: entities_draw ... frames=0` (entity draw frames) comes after them and used to win the tail -1
num() { grep -E "mcplay: (end|stream)" "$W/$1.out" | grep -o "$2=[0-9]*" | tail -1 | cut -d= -f2; }
FA=$(num A frames); FB=$(num B frames); CA=$(num A chunks); CB=$(num B chunks)
LA=$(num A lod_cols); LB=$(num B lod_cols); TA=$(num A lod_tris); TB=$(num B lod_tris)
echo "  A frames=${FA:-?} chunks=${CA:-?} lod_cols=${LA:-?} lod_tris=${TA:-?}   B frames=${FB:-?} chunks=${CB:-?} lod_cols=${LB:-?}"
[ "${FA:-0}" -gt 20 ] && [ "${FB:-0}" -gt 20 ] || { echo "  FAIL  too few frames"; ERR=1; }
[ "${CA:-0}" -gt 100 ] || { echo "  FAIL  LOD run loaded only ${CA:-0} chunks"; ERR=1; }
[ "${LA:-0}" -gt 0 ] && [ "${TA:-0}" -gt 0 ] || { echo "  FAIL  no LOD columns drawn in run A"; ERR=1; }
[ "${LB:-0}" -eq 0 ] || { echo "  FAIL  LOD columns drawn without MCPLAY_LOD (${LB:-?})"; ERR=1; }
# worker pool + window logic (a2): B has view 3 but the server sends MC_VIEW_SRV chunks -> columns beyond view + 2 leave the stream (window_ops > 0), the pool is up
WB=$(num B window_ops); KB=$(grep -o " cols=[0-9]*" "$W/B.out" | tail -1 | cut -d= -f2)
grep -q 'stream workers=3 pool=true' "$W/B.out" || { echo "  FAIL  stream has no mesher pool (workers=3 expected)"; ERR=1; }
[ "${WB:-0}" -gt 0 ] || { echo "  FAIL  window sync did nothing (window_ops=${WB:-?}) although the server sends more than view+2"; ERR=1; }
[ "${KB:-9999}" -lt "${CB:-0}" ] || { echo "  FAIL  stream keeps all ${KB:-?} of ${CB:-?} columns"; ERR=1; }
echo "  B window_ops=${WB:-?} stream_cols=${KB:-?}"
FC=$(num C frames); FD=$(num D frames); LC=$(num C lod_cols); TC=$(num C lod_tris); LD=$(num D lod_cols)
echo "  C (view 12, default LOD) frames=${FC:-?} lod_cols=${LC:-?} lod_tris=${TC:-?}   D (view 12, MCPLAY_LOD=0) frames=${FD:-?} lod_cols=${LD:-?}"
[ "${FC:-0}" -gt 20 ] && [ "${FD:-0}" -gt 20 ] || { echo "  FAIL  too few frames in the default-LOD runs"; ERR=1; }
[ "${LC:-0}" -gt 0 ] && [ "${TC:-0}" -gt 0 ] || { echo "  FAIL  view 12 without MCPLAY_LOD drew no LOD columns (LOD must be the default beyond view 8)"; ERR=1; }
[ "${LD:-0}" -eq 0 ] || { echo "  FAIL  MCPLAY_LOD=0 still drew LOD columns (${LD:-?})"; ERR=1; }
grep -q 'stream workers=3 pool=true' "$W/C.out" || { echo "  FAIL  default-LOD run has no mesher pool"; ERR=1; }
[ -s "$W/A.png" ] && [ -s "$W/B.png" ] && [ -s "$W/C.png" ] && [ -s "$W/D.png" ] || { echo "  FAIL  no screenshot"; ERR=1; }
# compare only the 3D view (rows 60..330: no debug text top-left, no hotbar): LOD must add visible terrain (>= 1 % of the pixels differ)
if [ -s "$W/A.png" ] && [ -s "$W/B.png" ]; then
    DIFF=$(python3 -I - "$W/A.png" "$W/B.png" <<'PY'
import sys
from PIL import Image, ImageChops
a = Image.open(sys.argv[1]).convert("RGB").crop((0, 60, 640, 330))
b = Image.open(sys.argv[2]).convert("RGB").crop((0, 60, 640, 330))
d = ImageChops.difference(a, b).convert("L").point(lambda v: 255 if v > 0 else 0)
print(sum(d.histogram()[255:]), a.width * a.height)
PY
) || DIFF=""
    echo "  picture diff (3D view): ${DIFF:-?} (differing / total pixels)"
    DP=${DIFF%% *}; DT=${DIFF##* }
    { [ -n "$DP" ] && [ "$DP" -ge $((DT / 100)) ] 2>/dev/null; } || { echo "  FAIL  LOD picture equals the no-LOD picture (3D view)"; ERR=1; }
fi
[ -n "${MCPLAY_PNG_OUT:-}" ] && [ -s "$W/A.png" ] && cp "$W/A.png" "$MCPLAY_PNG_OUT"
keep_logs
[ $ERR -eq 0 ] && echo "RESULT mc-play-lod: ok" || echo "RESULT mc-play-lod: FAIL"
exit $ERR
