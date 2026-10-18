#!/usr/bin/env bash
# tools/mc/play/run_models_gl.sh -- r245: pack models (stairs from baked quads) on the GL and Vulkan back ends of mcplay against the official 1.20.1 server
# (flat world, Xvfb, Mesa llvmpipe / lavapipe = software, NO real GPU). Same fixture pack and server setup as run_models.sh (four stairs, spawnRadius 0).
# Runs: C = CPU raster (MCPLAY_MODELS=1, stream), L = GL (MCPLAY_GL=1, models = auto default), V = Vulkan (MCPLAY_GFX=vk, auto), L0 / V0 = GL / Vulkan with
# MCPLAY_MODELS=0 (stairs = cubes), LS / VS = GL / Vulkan with MCPLAY_STREAM=0 (mc_render_world path).
# Checks: exit 0, frames, `models on` + `models states=N unmapped=0`; every model run has >= 60 pixels of EACH of the four fixture colours;
# PSNR(L, C), PSNR(LS, C), PSNR(V, C), PSNR(VS, C) (3D view crop) >= MODELS_GL_MIN_DB (34); L / V differ from the cube runs L0 / V0 in >= 0.5 % of the pixels.
# Prints `RESULT mc-play-models-gl: ok|FAIL`. Env: FIRNC, MODELS_KEEP_DIR, MODELS_RUN_S (30), MODELS_GL_MIN_DB (34), MODELS_GL_ONLY=gl|vk. Start it via heavy (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${MODELS_KEEP_DIR:-}" ] && { mkdir -p "$MODELS_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/pack.zip "$W/server.log" "$MODELS_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { [ "${ERR:-0}" -ne 0 ] && keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-models-gl: FAIL"; exit 1; }
for t in Xvfb xdotool java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
python3 -I -c "import PIL" 2>/dev/null || fail "missing python module: PIL"
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
# ---- fixture pack
python3 -I - "$W/pack.zip" "$W/pack2.zip" <<'PY' || fail "cannot build the fixture pack"
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
with zipfile.ZipFile(sys.argv[2], "w") as z:  # pack 2: only the texture, no blockstates / models (stairs stay cubes)
    z.writestr("pack.mcmeta", json.dumps({"pack": {"pack_format": 15, "description": "mcplay models gate fixture, no models"}}))
    z.writestr("assets/minecraft/textures/block/oak_planks.png", png.getvalue())
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
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "spawnRadius 0"; do echo "gamerule $gr" >&3; done
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
        env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_VSYNC=0 MCPLAY_RUN_S=${MODELS_RUN_S:-30} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 \
            MC_PACKS="$W/pack.zip" MCPLAY_SHOT="$W/$name.png" "$@" "$W/mcplay" 127.0.0.1 "$PORT" "$pn" > "$W/$name.out" 2>&1 &
        pp=$!
        if wait_log "$pn joined the game" 90; then
            sleep 2
            xdotool mousemove 320 180 2>/dev/null
            nl=$(wc -l < "$W/server.log")
            echo "tp $pn 0.5 -60 0.5 0 20" >&3
            wait_log "Teleported $pn to" 10 "$nl" || ok=0
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
ONLY=${MODELS_GL_ONLY:-}
run_one C 0 MCPLAY_MODELS=1
if [ "$ONLY" != vk ]; then
    run_one L 0 MCPLAY_GL=1
    run_one L0 0 MCPLAY_GL=1 MCPLAY_MODELS=0
    run_one LS 0 MCPLAY_GL=1 MCPLAY_STREAM=0
fi
if [ "$ONLY" != gl ]; then
    run_one V 0 MCPLAY_GFX=vk
    run_one V0 0 MCPLAY_GFX=vk MCPLAY_MODELS=0
    run_one VS 0 MCPLAY_GFX=vk MCPLAY_STREAM=0
fi
RUNS="C"; [ "$ONLY" != vk ] && RUNS="$RUNS L L0 LS"; [ "$ONLY" != gl ] && RUNS="$RUNS V V0 VS"
num() { grep -E "mcplay: (end|stream)" "$W/$1.out" | grep -o "$2=[0-9]*" | tail -1 | cut -d= -f2; }
for n in $RUNS; do
    F=$(num $n frames)
    [ "${F:-0}" -gt 5 ] || { echo "  FAIL  $n: too few frames (${F:-?})"; ERR=1; }
    [ -s "$W/$n.png" ] || { echo "  FAIL  $n: no screenshot"; ERR=1; }
done
for n in L LS; do case " $RUNS " in *" $n "*) grep -q "^mcplay: gl renderer=" "$W/$n.out" || { echo "  FAIL  $n: GL path did not start"; ERR=1; } ;; esac; done
for n in C L LS V VS; do
    case " $RUNS " in *" $n "*) ;; *) continue ;; esac
    grep -q "^mcplay: models on" "$W/$n.out" || { echo "  FAIL  $n: no 'models on' line (auto default with a pack)"; ERR=1; }
    MS=$(grep -E "^mcplay: models states=" "$W/$n.out" | tail -1)
    S=$(echo "$MS" | grep -o "states=[0-9]*" | cut -d= -f2); U=$(echo "$MS" | grep -o "unmapped=[0-9]*" | cut -d= -f2)
    echo "  $n $MS"
    [ "${S:-0}" -gt 0 ] && [ "${U:-1}" -eq 0 ] || { echo "  FAIL  $n: no model states resolved or unmapped quads"; ERR=1; }
done
for n in L0 V0; do case " $RUNS " in *" $n "*) grep -q "^mcplay: models" "$W/$n.out" && { echo "  FAIL  $n (MCPLAY_MODELS=0) printed a models line"; ERR=1; } ;; esac; done
for n in L0 V0 L V; do case " $RUNS " in *" $n "*) grep -qi "warn\|ignored" "$W/$n.out" && { echo "  FAIL  $n printed a warning:"; grep -i "warn\|ignored" "$W/$n.out" | head -2; ERR=1; } ;; esac; done
if [ $ERR -eq 0 ]; then
    python3 -I - "$W" "$RUNS" "${MODELS_GL_MIN_DB:-34}" <<'PY' || ERR=1
import sys, math
from PIL import Image, ImageChops
W, runs, mindb = sys.argv[1], sys.argv[2].split(), float(sys.argv[3])
box = (0, 60, 640, 330)
def img(n): return Image.open(f"{W}/{n}.png").convert("RGB").crop(box)
def fixture(im):
    cnt = [0, 0, 0, 0]
    raw = im.tobytes()
    for i in range(0, len(raw), 3):
        r, g, b = raw[i], raw[i + 1], raw[i + 2]
        m = max(r, g, b)
        if m < 100: continue
        lo = lambda v: v * 100 < m * 12
        if r * 100 >= m * 88 and lo(g) and lo(b): cnt[0] += 1
        elif g * 100 >= m * 88 and lo(r) and lo(b): cnt[1] += 1
        elif b * 100 >= m * 88 and lo(r) and lo(g): cnt[2] += 1
        elif r * 100 >= m * 80 and g * 100 >= m * 80 and lo(b): cnt[3] += 1
    return cnt
def psnr(a, b):
    ha = ImageChops.difference(a, b).histogram()
    se = 0
    for ch in range(3):
        for v in range(256): se += ha[ch * 256 + v] * v * v
    mse = se / (a.width * a.height * 3)
    return 99.0 if mse == 0 else 10 * math.log10(255 * 255 / mse)
def ndiff(a, b, t=0):
    return sum(ImageChops.difference(a, b).convert("L").point(lambda v: 255 if v > t else 0).histogram()[255:])
ims = {n: img(n) for n in runs}
tot = ims["C"].width * ims["C"].height
bad = 0
for n in runs:
    if n in ("L0", "V0"): continue
    fx = fixture(ims[n])
    print(f"  fixture colours (red green blue yellow) {n}: {fx}")
    if min(fx) < 60: print(f"  FAIL  {n}: a fixture colour has < 60 pixels"); bad = 1
for n in runs:
    if n == "C": continue
    p = psnr(ims[n], ims["C"])
    tag = "cubes vs models-CPU (info)" if n in ("L0", "V0") else "vs CPU models"
    print(f"  PSNR {n} {tag}: {p:.2f} dB  (px differing > 2 levels: {ndiff(ims[n], ims['C'], 2)} of {tot})")
    if n not in ("L0", "V0") and p < mindb: print(f"  FAIL  PSNR {n} {p:.2f} dB < {mindb}"); bad = 1
for a, b in (("L", "L0"), ("V", "V0")):
    if a in ims and b in ims:
        d = ndiff(ims[a], ims[b], 2)
        print(f"  {a} vs {b} (models vs cubes): {d} px differ")
        if d < tot // 200: print(f"  FAIL  {a} equals {b}: stairs drawn as cubes?"); bad = 1
sys.exit(bad)
PY
fi
keep_logs
[ $ERR -eq 0 ] && echo "RESULT mc-play-models-gl: ok" || echo "RESULT mc-play-models-gl: FAIL"
exit $ERR
