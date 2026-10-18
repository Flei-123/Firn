#!/usr/bin/env bash
# tools/mc/play/run_models.sh -- mcplay with MCPLAY_MODELS=1 (pack blockstates + models: stairs from baked quads instead of cubes) against the official 1.20.1
# server (flat world): Xvfb + server + mcplay + xdotool. Own fixture pack (built here with python zipfile + PIL, NO Mojang file): oak_planks.png with four
# pure quadrant colours (red / green top, blue / yellow bottom), a stairs model (parent block/stairs) and an oak_stairs blockstate with all four facings.
# Run A: MCPLAY_MODELS=1. Run B: same world, MCPLAY_MODELS unset (default: the stairs are full cubes). Run C: like A, but the T key (pack reload) is
# pressed after the blocks stand -> ModelSet reset + wr_clear, the picture must have the models again.
# Pick probes (MCPLAY_PICK_PROBE, r236b): A / C pick the quads of the stairs (mc_model_pick: distances 3.0 / 4.5 / 4.0), B the whole voxel (2.5 / 4.0 / 4.0).
# Checks: exit 0, frames; A and C print `models on` + `models states=N unmapped=0` (N > 0); B prints no `models` line; A has >= 120 pixels of EACH of
# the four fixture colours in the 3D view; B differs from A in >= 1 % of the 3D view pixels (stairs vs cubes); C has the fixture colours again.
# Prints `RESULT mc-play-models: ok|FAIL`. Env: FIRNC, MODELS_KEEP_DIR (copy of outs/pngs/pack/server.log, always), MODELS_RUN_S (40 = run time per mcplay),
# MCPLAY_PNG_OUT (copy of the A screenshot). Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${MODELS_KEEP_DIR:-}" ] && { mkdir -p "$MODELS_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/pack.zip "$W/server.log" "$MODELS_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { [ "${ERR:-0}" -ne 0 ] && keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-models: FAIL"; exit 1; }
for t in Xvfb xdotool java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
python3 -I -c "import PIL" 2>/dev/null || fail "missing python module: PIL"
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
# ---- fixture pack
python3 -I - "$W/pack.zip" <<'PY' || fail "cannot build the fixture pack"
import sys, zipfile, io
from PIL import Image
im = Image.new("RGBA", (16, 16))
for y in range(16):
    for x in range(16):
        c = (255, 0, 0) if (y < 8 and x < 8) else (0, 255, 0) if (y < 8) else (0, 0, 255) if (x < 8) else (255, 255, 0)
        im.putpixel((x, y), c + (255,))
png = io.BytesIO(); im.save(png, "PNG")
tex = {"bottom": "block/oak_planks", "top": "block/oak_planks", "side": "block/oak_planks"}
import json
model = json.dumps({"parent": "block/stairs", "textures": tex})
def var(y):
    v = {"model": "block/mdl_stairs"}
    if y: v.update({"y": y, "uvlock": True})
    return v
bs = json.dumps({"variants": {"facing=east,half=bottom,shape=straight": var(0), "facing=south,half=bottom,shape=straight": var(90),
    "facing=west,half=bottom,shape=straight": var(180), "facing=north,half=bottom,shape=straight": var(270)}})
with zipfile.ZipFile(sys.argv[1], "w") as z:
    z.writestr("pack.mcmeta", json.dumps({"pack": {"pack_format": 15, "description": "mcplay models gate fixture"}}))
    z.writestr("assets/minecraft/textures/block/oak_planks.png", png.getvalue())
    z.writestr("assets/minecraft/models/block/mdl_stairs.json", model)
    z.writestr("assets/minecraft/blockstates/oak_stairs.json", bs)
PY
MCV=1.20.1
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
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
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
            local k=0
            for spec in "-2 east" "0 south" "2 west" "4 north"; do
                set -- $spec
                echo "setblock $1 -60 5 oak_stairs[facing=$2]" >&3
                k=$((k + 1))
            done
            sleep 4
            if [ "$reload" = 1 ]; then xdotool key t; sleep 6; fi
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
# Pick probes (r236b, mc_model_pick): rays through the stairs at (-2, -60, 5) (facing east: lower half y < 0.5, upper step x > -1.5).
# R1 sideways at y = 0.75 through the empty part into the step (models: 3.0 to the step face, voxel pick: 2.5 to the cell border), R2 down onto the
# lower half (models 4.5 / voxel 4.0), R3 down onto the step (4.0 both).
PROBE="-4500,-59250,5500,1000,0,0;-1750,-55000,5500,0,-1000,0;-1250,-55000,5500,0,-1000,0"
run_one A 0 MCPLAY_MODELS=1 MCPLAY_PICK_PROBE="$PROBE"
run_one B 0 MCPLAY_PICK_PROBE="$PROBE"
run_one C 1 MCPLAY_MODELS=1 MCPLAY_PICK_PROBE="$PROBE"
num() { grep -E "mcplay: (end|stream)" "$W/$1.out" | grep -o "$2=[0-9]*" | tail -1 | cut -d= -f2; }
for n in A B C; do
    F=$(num $n frames)
    [ "${F:-0}" -gt 20 ] || { echo "  FAIL  $n: too few frames (${F:-?})"; ERR=1; }
done
for n in A C; do
    grep -q "^mcplay: models on" "$W/$n.out" || { echo "  FAIL  $n: no 'models on' line"; ERR=1; }
    MS=$(grep -E "^mcplay: models states=" "$W/$n.out" | tail -1)
    S=$(echo "$MS" | grep -o "states=[0-9]*" | cut -d= -f2); U=$(echo "$MS" | grep -o "unmapped=[0-9]*" | cut -d= -f2)
    echo "  $n $MS"
    [ "${S:-0}" -gt 0 ] && [ "${U:-1}" -eq 0 ] || { echo "  FAIL  $n: no model states resolved or unmapped quads (states=${S:-?} unmapped=${U:-?})"; ERR=1; }
done
grep -q "^mcplay: models" "$W/B.out" && { echo "  FAIL  B (default) printed a models line"; ERR=1; }
grep -q "stream workers" "$W/A.out" && { echo "  FAIL  A has the stream on (MCPLAY_MODELS must force MCPLAY_STREAM=0)"; ERR=1; }
for n in A B C; do [ -s "$W/$n.png" ] || { echo "  FAIL  $n: no screenshot"; ERR=1; }; done
if [ $ERR -eq 0 ]; then
    RES=$(python3 -I - "$W/A.png" "$W/B.png" "$W/C.png" <<'PY'
import sys
from PIL import Image, ImageChops
def crop(p): return Image.open(p).convert("RGB").crop((0, 60, 640, 330))
def fixture(im):
    # pure fixture colours: a dominant channel set, the others (almost) zero
    cnt = [0, 0, 0, 0]
    raw = im.tobytes()
    for i in range(0, len(raw), 3):
        r, g, b = raw[i], raw[i + 1], raw[i + 2]
        m = max(r, g, b)
        if m < 100: continue
        lo = lambda v: v * 100 < m * 12
        if r * 100 >= m * 88 and lo(g) and lo(b): cnt[0] += 1       # red
        elif g * 100 >= m * 88 and lo(r) and lo(b): cnt[1] += 1     # green
        elif b * 100 >= m * 88 and lo(r) and lo(g): cnt[2] += 1     # blue
        elif r * 100 >= m * 80 and g * 100 >= m * 80 and lo(b): cnt[3] += 1  # yellow
    return cnt
a, b, c = crop(sys.argv[1]), crop(sys.argv[2]), crop(sys.argv[3])
d = ImageChops.difference(a, b).convert("L").point(lambda v: 255 if v > 0 else 0)
print("A", *fixture(a), "B", *fixture(b), "C", *fixture(c), "diffAB", sum(d.histogram()[255:]), "total", a.width * a.height)
PY
) || RES=""
    echo "  fixture colours (red green blue yellow): $RES"
    set -- $RES
    if [ -z "$RES" ]; then echo "  FAIL  picture analysis failed"; ERR=1; else
        for i in 2 3 4 5; do [ "${!i}" -ge 120 ] || { echo "  FAIL  A: fixture colour #$((i - 1)) only ${!i} pixels"; ERR=1; }; done
        for i in 12 13 14 15; do [ "${!i}" -ge 60 ] || { echo "  FAIL  C (after reload): fixture colour #$((i - 11)) only ${!i} pixels"; ERR=1; }; done
        DP=${17}; DT=${19}
        [ "$DP" -ge $((DT / 100)) ] || { echo "  FAIL  A equals B (stairs drawn as cubes?): $DP of $DT pixels differ"; ERR=1; }
    fi
fi
# pick probes: A and C (models) hit the quads, B (default) the whole voxel
probe_line() { grep -E "^mcplay: probe " "$W/$1.out" | sed -n "${2}p"; } # run, ray number
probe_chk() { # run, ray, expected "x= y= z= n= dist_mm=" tail, what
    local L; L=$(probe_line "$1" "$2")
    case "$L" in
        *" hit=1 x=-2 y=-60 z=5 n=$3 dist_mm=$4 "*) ;;
        *) echo "  FAIL  $1 probe $2 ($5): got '$L', want n=$3 dist_mm=$4"; ERR=1 ;;
    esac
}
for n in A C; do
    grep -q "^mcplay: probe models=1" "$W/$n.out" || { echo "  FAIL  $n: no 'probe models=1' line"; ERR=1; }
    probe_chk $n 1 "-1,0,0" 3000 "step face through the empty half"
    probe_chk $n 2 "0,1,0" 4500 "top of the lower half"
    probe_chk $n 3 "0,1,0" 4000 "top of the step"
done
grep -q "^mcplay: probe models=0" "$W/B.out" || { echo "  FAIL  B: no 'probe models=0' line"; ERR=1; }
probe_chk B 1 "-1,0,0" 2500 "voxel border"
probe_chk B 2 "0,1,0" 4000 "voxel top"
probe_chk B 3 "0,1,0" 4000 "voxel top"
[ -n "${MCPLAY_PNG_OUT:-}" ] && [ -s "$W/A.png" ] && cp "$W/A.png" "$MCPLAY_PNG_OUT"
keep_logs
[ $ERR -eq 0 ] && echo "RESULT mc-play-models: ok" || echo "RESULT mc-play-models: FAIL"
exit $ERR
