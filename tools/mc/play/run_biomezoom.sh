#!/usr/bin/env bash
# tools/mc/play/run_biomezoom.sh -- picture proof of the vanilla Voronoi biome zoom (r258) on a real 1.20.1 server.
# Official server (seed 4242), a floating platform at y=150 (z -31..-1 grass_block, z 0..31 oak_leaves, x -32..31) and `fillbiome` with a straight biome
# border at x=0 (jungle x<0, desert x>=0). The fixture pack maps grass_block / oak_leaves to cube models with a white tinted face, so the picture
# shows the biome tint colour directly. The player hovers at y=170 and looks straight down (tp ... 0 90).
#   Z0B0 = MCPLAY_BIOME_ZOOM=0 BLEND=0 : plain 4x4x4 cells, the border must be a straight line (crossing x identical in all rows)
#   Z1B0 = MCPLAY_BIOME_ZOOM=1 BLEND=0 : Voronoi zoom, the border must be jagged (crossing x varies by whole blocks)
#   Z0B2 / Z1B2 / Z1B2b = default blend 2 with zoom off / on / on again (determinism: Z1B2 == Z1B2b, Z0B2 != Z1B2)
# WATER PHASE (r263): after the five runs the platform becomes a one-block water basin (stone rim, water y=151) and the biomes become swamp (x<0, water 617B64)
# | warm_ocean (x>=0, water 43D5EE). The fixture pack has a white block/water_still. Runs (MCPLAY_WATER_TINT=0/1):
#   WOFF = tint off (water stays on the cube path: one flat colour, no border)  WZ0B0 / WZ1B0 = tint on, zoom off / on, blend 0 (straight / jagged border)
#   WZ1B2 / WZ1B2b = tint on, zoom on, default blend (determinism)  WT = same with a translucent water texture (alpha 150: translucent pass)
# Prints `BW ...` for the water pictures. Env BZ_NO_WATER=1 skips the water phase.
# Prints `BZ ...` (numbers) and `RESULT mc-play-biomezoom: ok|FAIL`. Env: FIRNC, BZ_KEEP_DIR (copy of pngs + diff images + server.log), BZ_RUN_S (35).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="${BZ_FIRNLIB:-$(pwd)/lib}"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${BZ_KEEP_DIR:-}" ] && { mkdir -p "$BZ_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W/server.log" "$BZ_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-biomezoom: FAIL"; exit 1; }
for t in Xvfb xdotool java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
python3 -I -c "import PIL" 2>/dev/null || fail "missing python module: PIL"
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
python3 -I - "$W/pack.zip" "$W/pack_t.zip" <<'PY' || fail "cannot build the fixture pack"
import sys, zipfile, io, json
from PIL import Image
def png(c, a=255):
    im = Image.new("RGBA", (16, 16), c + (a,)); b = io.BytesIO(); im.save(b, "PNG"); return b.getvalue()
def cube(top, side, tinted_all=False):
    f = {}
    for d in ("up", "north", "south", "east", "west"):  # no "down" face: a full 6-face cube stays on the plain cube path (is_unit_cube)
        t = top if (d == "up" or tinted_all) else side
        f[d] = {"texture": "#" + t, "cullface": d}
        if t == top: f[d]["tintindex"] = 0
    return json.dumps({"textures": {"top": "block/stone", "side": "block/dirt"},
                       "elements": [{"from": [0, 0, 0], "to": [16, 16, 16], "faces": f}]})
for path, wa in ((sys.argv[1], 255), (sys.argv[2], 150)):  # second pack: translucent water texture (translucent pass)
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("pack.mcmeta", json.dumps({"pack": {"pack_format": 15, "description": "mcplay biomezoom gate fixture"}}))
        z.writestr("assets/minecraft/textures/block/stone.png", png((255, 255, 255)))
        z.writestr("assets/minecraft/textures/block/dirt.png", png((120, 80, 40)))
        z.writestr("assets/minecraft/textures/block/water_still.png", png((255, 255, 255), wa))
        z.writestr("assets/minecraft/models/block/mdl_grass.json", cube("top", "side"))
        z.writestr("assets/minecraft/models/block/mdl_leaves.json", cube("top", "side", True))
        z.writestr("assets/minecraft/blockstates/grass_block.json", json.dumps({"variants": {"": {"model": "block/mdl_grass"}}}))
        z.writestr("assets/minecraft/blockstates/oak_leaves.json", json.dumps({"variants": {"": {"model": "block/mdl_leaves"}}}))
PY
# r268: optional REAL vanilla water_still (animated 16 x 512 strip, alpha 180): BZ_REAL_WATER=<path of water_still.png> (+ <path>.mcmeta) adds run WR
if [ -n "${BZ_REAL_WATER:-}" ] && [ -s "$BZ_REAL_WATER" ]; then
    python3 -I - "$W/pack.zip" "$W/pack_r.zip" "$BZ_REAL_WATER" <<'PY' || fail "cannot build the real water pack"
import sys, zipfile, os
src, dst, real = sys.argv[1:4]
with zipfile.ZipFile(src) as zi, zipfile.ZipFile(dst, "w") as zo:
    for n in zi.namelist():
        if n != "assets/minecraft/textures/block/water_still.png": zo.writestr(n, zi.read(n))
    zo.writestr("assets/minecraft/textures/block/water_still.png", open(real, "rb").read())
    meta = real + ".mcmeta"
    zo.writestr("assets/minecraft/textures/block/water_still.png.mcmeta", open(meta, "rb").read() if os.path.exists(meta) else '{"animation":{"frametime":2}}')
PY
fi
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
level-seed=4242
view-distance=6
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
for i in $(seq 1 240); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "spawnRadius 0" "randomTickSpeed 0"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
NSB=$(wc -l < "$W/server.log")
echo "fill -32 150 -31 31 150 -1 grass_block" >&3
echo "fill -32 150 0 31 150 31 oak_leaves[persistent=true]" >&3
echo "fillbiome -32 136 -32 -1 167 -1 minecraft:jungle" >&3
echo "fillbiome -32 136 0 -1 167 31 minecraft:jungle" >&3
echo "fillbiome 0 136 -32 31 167 -1 minecraft:desert" >&3
echo "fillbiome 0 136 0 31 167 31 minecraft:desert" >&3
for i in $(seq 1 60); do
    L=$(tail -n +"$((NSB + 1))" "$W/server.log")
    [ "$(echo "$L" | grep -c 'Successfully filled')" -ge 2 ] && [ "$(echo "$L" | grep -c 'biome entry/entries set between')" -ge 4 ] && break; sleep 1
done
L=$(tail -n +"$((NSB + 1))" "$W/server.log")
[ "$(echo "$L" | grep -c 'Successfully filled')" -ge 2 ] || { echo "$L" | tail -8; fail "server fill of the platform (need 2 'Successfully filled')"; }
[ "$(echo "$L" | grep -c 'biome entry/entries set between')" -ge 4 ] || { echo "$L" | tail -8; fail "server fillbiome (need 4 'biome entry/entries set between')"; }
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; sleep 1; done; return 1; }
run_one() { # name, env...
    local name=$1; shift
    local att pn pp rc ok py nl tries
    for att in 1 2 3; do
        pn="B${name}${att}"; ok=1; rm -f "$W/$name.png"
        env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_CREATIVE=1 MCPLAY_RUN_S=${RUN_S:-35} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 \
            MC_PACKS="$W/pack.zip" MCPLAY_SHOT="$W/$name.png" "$@" "$W/mcplay" 127.0.0.1 "$PORT" "$pn" > "$W/$name.out" 2>&1 &
        pp=$!
        if wait_log "$pn joined the game" 120; then
            sleep 3
            xdotool mousemove 320 180 2>/dev/null
            xdotool key f
            tries=0
            while [ $tries -lt 4 ]; do
                nl=$(wc -l < "$W/server.log")
                echo "tp $pn 0.5 170 0.5 0 90" >&3
                wait_log "Teleported $pn to" 10 "$nl" && break
                tries=$((tries + 1))
            done
            [ $tries -lt 4 ] || ok=0
        else
            ok=0
        fi
        wait $pp; rc=$?
        py=$(grep -E "^mcplay: pos " "$W/$name.out" | tail -1 | grep -o " y=-\?[0-9]*" | cut -d= -f2)
        [ -n "$py" ] && [ "$py" -ge 120 ] 2>/dev/null || ok=0
        [ $rc -eq 0 ] || { echo "  FAIL  $name exit code $rc"; ERR=1; }
        [ $ok -eq 1 ] && break
        echo "  retry $name (attempt $att): join / tp lost or player not up (end y=${py:-?})"
    done
    [ $ok -eq 1 ] || { echo "  FAIL  $name: player never got up after 3 attempts"; ERR=1; }
    echo "  $name player=$pn end y=${py:-?} load=$(cut -d' ' -f1 /proc/loadavg)"
}
RUN_S=${BZ_RUN_S:-35}
run_one Z0B0 MCPLAY_BIOME_ZOOM=0 MCPLAY_BIOME_BLEND=0
run_one Z1B0 MCPLAY_BIOME_ZOOM=1 MCPLAY_BIOME_BLEND=0
run_one Z0B2 MCPLAY_BIOME_ZOOM=0
run_one Z1B2 MCPLAY_BIOME_ZOOM=1
run_one Z1B2b MCPLAY_BIOME_ZOOM=1
WATER_NAMES=""
if [ "${BZ_NO_WATER:-0}" != "1" ]; then
    WATER_NAMES="WOFF WZ0B0 WZ1B0 WZ1B2 WZ1B2b WT WG WV"
    NSW=$(wc -l < "$W/server.log")
    echo "fill -32 151 -31 31 151 31 minecraft:stone" >&3
    echo "fill -31 151 -30 30 151 30 minecraft:water" >&3
    echo "fillbiome -32 136 -32 -1 167 -1 minecraft:swamp" >&3
    echo "fillbiome -32 136 0 -1 167 31 minecraft:swamp" >&3
    echo "fillbiome 0 136 -32 31 167 -1 minecraft:warm_ocean" >&3
    echo "fillbiome 0 136 0 31 167 31 minecraft:warm_ocean" >&3
    for i in $(seq 1 90); do
        L=$(tail -n +"$((NSW + 1))" "$W/server.log")
        [ "$(echo "$L" | grep -c 'Successfully filled')" -ge 2 ] && [ "$(echo "$L" | grep -c 'biome entry/entries set between')" -ge 4 ] && break; sleep 1
    done
    L=$(tail -n +"$((NSW + 1))" "$W/server.log")
    [ "$(echo "$L" | grep -c 'Successfully filled')" -ge 2 ] && [ "$(echo "$L" | grep -c 'biome entry/entries set between')" -ge 4 ] || { echo "$L" | tail -8; echo "  FAIL  water phase server setup"; ERR=1; WATER_NAMES=""; }
    if [ -n "$WATER_NAMES" ]; then
        run_one WOFF MCPLAY_WATER_TINT=0 MCPLAY_BIOME_ZOOM=1
        run_one WZ0B0 MCPLAY_WATER_TINT=1 MCPLAY_BIOME_ZOOM=0 MCPLAY_BIOME_BLEND=0
        run_one WZ1B0 MCPLAY_WATER_TINT=1 MCPLAY_BIOME_ZOOM=1 MCPLAY_BIOME_BLEND=0
        run_one WZ1B2 MCPLAY_WATER_TINT=1 MCPLAY_BIOME_ZOOM=1
        run_one WZ1B2b MCPLAY_WATER_TINT=1 MCPLAY_BIOME_ZOOM=1
        run_one WT MCPLAY_WATER_TINT=1 MCPLAY_BIOME_ZOOM=1 MC_PACKS="$W/pack_t.zip"
        run_one WG MCPLAY_WATER_TINT=1 MCPLAY_BIOME_ZOOM=1 MCPLAY_GL=1
        run_one WV MCPLAY_WATER_TINT=1 MCPLAY_BIOME_ZOOM=1 MCPLAY_GFX=vk
        if [ -s "$W/pack_r.zip" ]; then
            WATER_NAMES="$WATER_NAMES WR"
            run_one WR MCPLAY_WATER_TINT=1 MCPLAY_BIOME_ZOOM=1 MC_PACKS="$W/pack_r.zip"
        fi
    fi
fi
for n in Z0B0 Z1B0 Z0B2 Z1B2 Z1B2b $WATER_NAMES; do
    F=$(grep -E "^mcplay: end" "$W/$n.out" | grep -o "frames=[0-9]*" | tail -1 | cut -d= -f2)
    [ "${F:-0}" -gt 20 ] || { echo "  FAIL  $n: too few frames (${F:-?})"; ERR=1; }
    [ -s "$W/$n.png" ] || { echo "  FAIL  $n: no screenshot"; ERR=1; }
    grep -q "^mcplay: models on" "$W/$n.out" || { echo "  FAIL  $n: 'models on' line missing"; ERR=1; }
done
if [ $ERR -eq 0 ]; then
    RES=$(python3 -I - "$W" <<'PY'
import sys
from PIL import Image, ImageChops
W = sys.argv[1]
names = ("Z0B0", "Z1B0", "Z0B2", "Z1B2", "Z1B2b")
im = {n: Image.open(f"{W}/{n}.png").convert("RGB") for n in names}
for n in names: im[n].paste((0, 0, 0), (0, 0, 170, 60))  # the HUD text (FPS, position) changes from run to run: not part of the picture proof
def nz(a, b): return sum(ImageChops.difference(a, b).convert("L").point(lambda v: 255 if v > 0 else 0).histogram()[255:])
# x position (sub pixel) where the colour crosses the middle between the far left (jungle) and far right (desert) colour, per image row
def med(v):
    v = sorted(v); return v[len(v) // 2]
def crossings(i, y0, y1):
    px = i.load(); out = []
    for y in range(y0, y1):
        L = [med([px[x, y][c] for x in range(180, 260)]) for c in range(3)]
        R = [med([px[x, y][c] for x in range(500, 600)]) for c in range(3)]
        d = [R[c] - L[c] for c in range(3)]
        dd = sum(v * v for v in d)
        if dd < 900: continue
        t = [sum((px[x, y][c] - L[c]) * d[c] for c in range(3)) / dd for x in range(0, 640)]
        pos = None
        for x in range(270, 380):  # the border is near the image centre (x = 0.5 blocks left of the player); the screenshot has dark single-pixel seams elsewhere
            # stable crossing: 4 pixels before below the middle, 4 pixels from here on above it (single dark pixels of the screenshot do not count)
            if all(t[x - k] < 0.5 for k in range(1, 5)) and all(t[x + k] >= 0.5 for k in range(0, 4)):
                pos = x - 1 + (0.5 - t[x - 1]) / (t[x] - t[x - 1]) if t[x] != t[x - 1] else x; break
        if pos is not None: out.append(pos)
    return out
def stat(c):
    if len(c) < 10: return (len(c), 0.0, 0)
    c = sorted(c); n = len(c)
    return (n, c[(n * 95) // 100 - 1] - c[(n * 5) // 100], len(set(round(v, 1) for v in c)))
# looking south the screen top is +z: rows 10..170 = leaves half (z >= 0), rows 190..350 = grass half (z < 0); the z = 0 line is the image middle
res = {}
for n in ("Z0B0", "Z1B0", "Z0B2", "Z1B2"):
    l = stat(crossings(im[n], 10, 170)); g = stat(crossings(im[n], 190, 350))
    res[n] = (g, l)
print("rows", *[f"{n}:g={res[n][0][0]}/{res[n][0][1]:.1f}/{res[n][0][2]},l={res[n][1][0]}/{res[n][1][1]:.1f}/{res[n][1][2]}" for n in res],
      "diffB0", nz(im["Z0B0"], im["Z1B0"]), "diffB2", nz(im["Z0B2"], im["Z1B2"]), "diffBlend", nz(im["Z0B0"], im["Z0B2"]),
      "det", nz(im["Z1B2"], im["Z1B2b"]), "total", 640 * 360)
import json
json.dump({n: res[n] for n in res}, open(f"{W}/stats.json", "w"))
# artifacts: side by side (off | on) and amplified diff
for a, b, nm in (("Z0B0", "Z1B0", "cmp_blend0"), ("Z0B2", "Z1B2", "cmp_blend2")):
    c = Image.new("RGB", (1280, 360)); c.paste(im[a], (0, 0)); c.paste(im[b], (640, 0)); c.save(f"{W}/{nm}.png")
    d = ImageChops.difference(im[a], im[b]).point(lambda v: min(255, v * 4)); d.save(f"{W}/diff_{nm}.png")
PY
) || RES=""
    echo "  BZ $RES"
    if [ -z "$RES" ]; then echo "  FAIL  picture analysis failed"; ERR=1; else
        python3 -I - "$W/stats.json" "$RES" <<'PY' || ERR=1
import sys, json, re
st = json.load(open(sys.argv[1])); res = sys.argv[2]
ok = True
def bad(m):
    global ok; ok = False; print("  FAIL ", m)
for half, k in (("grass", 0), ("leaves", 1)):
    a = st["Z0B0"][k]; b = st["Z1B0"][k]
    if a[0] < 60 or b[0] < 60: bad(f"{half}: too few border rows found (off {a[0]}, on {b[0]})"); continue
    if a[1] > 1.5: bad(f"{half}: zoom OFF border is not straight (p5..p95 range {a[1]:.1f} px)")
    if b[1] < 6.0: bad(f"{half}: zoom ON border is not jagged (p5..p95 range {b[1]:.1f} px, need >= 6)")
    if b[2] < 2: bad(f"{half}: zoom ON has only {b[2]} distinct border position")
m = dict(zip(res.split()[0::1][1::2], res.split()[1::2])) if False else None
t = res.split()
v = {t[i]: int(t[i + 1]) for i in range(len(t) - 1) if t[i] in ("diffB0", "diffB2", "diffBlend", "det", "total") and t[i + 1].lstrip("-").isdigit()}
if v.get("diffB0", 0) < 500: bad(f"zoom ON vs OFF (blend 0) differ by only {v.get('diffB0')} px")
if v.get("diffB2", 0) < 200: bad(f"zoom ON vs OFF (blend 2) differ by only {v.get('diffB2')} px")
if v.get("diffBlend", 0) < 200: bad("blend 2 vs blend 0 pictures (zoom off) do not differ: blend has no effect")
if v.get("det", 1) != 0: bad(f"zoom ON is not deterministic: two runs differ by {v.get('det')} px")
sys.exit(0 if ok else 1)
PY
    fi
fi
if [ $ERR -eq 0 ] && [ -n "$WATER_NAMES" ]; then
    WRES=$(python3 -I - "$W" <<'PY'
import sys, json
from PIL import Image, ImageChops
W = sys.argv[1]
import os
names = ("WOFF", "WZ0B0", "WZ1B0", "WZ1B2", "WZ1B2b", "WT", "WG", "WV") + (("WR",) if os.path.exists(f"{W}/WR.png") else ())
im = {n: Image.open(f"{W}/{n}.png").convert("RGB") for n in names}
for n in names: im[n].paste((0, 0, 0), (0, 0, 170, 60))
def nz(a, b): return sum(ImageChops.difference(a, b).convert("L").point(lambda v: 255 if v > 0 else 0).histogram()[255:])
def med(v):
    v = sorted(v); return v[len(v) // 2]
def side(i, y, x0, x1):
    px = i.load(); return [med([px[x, y][c] for x in range(x0, x1)]) for c in range(3)]
def crossings(i, y0, y1):
    px = i.load(); out = []
    for y in range(y0, y1):
        L = side(i, y, 180, 260); R = side(i, y, 500, 600)
        d = [R[c] - L[c] for c in range(3)]
        dd = sum(v * v for v in d)
        if dd < 900: continue
        t = [sum((px[x, y][c] - L[c]) * d[c] for c in range(3)) / dd for x in range(0, 640)]
        pos = None
        for x in range(270, 380):
            if all(t[x - k] < 0.5 for k in range(1, 5)) and all(t[x + k] >= 0.5 for k in range(0, 4)):
                pos = x - 1 + (0.5 - t[x - 1]) / (t[x] - t[x - 1]) if t[x] != t[x - 1] else x; break
        if pos is not None: out.append(pos)
    return out
def stat(c):
    if len(c) < 10: return (len(c), 0.0, 0)
    c = sorted(c); n = len(c)
    return (n, c[(n * 95) // 100 - 1] - c[(n * 5) // 100], len(set(round(v, 1) for v in c)))
import math
def psnr(a, b):
    h = ImageChops.difference(a, b).histogram(); n = 0.0
    for c in range(3):
        for v in range(256): n += h[c * 256 + v] * v * v
    mse = n / (a.size[0] * a.size[1] * 3)
    return 99.0 if mse == 0 else 10 * math.log10(255 * 255 / mse)
res = {}
for n in names:
    s = stat(crossings(im[n], 10, 350))
    L = side(im[n], 100, 180, 260); R = side(im[n], 100, 500, 600)
    L2 = side(im[n], 260, 180, 260); R2 = side(im[n], 260, 500, 600)
    res[n] = {"rows": s[0], "range": round(s[1], 1), "distinct": s[2], "L": L, "R": R, "L2": L2, "R2": R2}
print("rows", *[f"{n}:{res[n]['rows']}/{res[n]['range']}/{res[n]['distinct']}" for n in names],
      "LR", *[f"{n}:L={res[n]['L']},R={res[n]['R']}".replace(" ", "") for n in names],
      "diffWB0", nz(im["WZ0B0"], im["WZ1B0"]), "diffWB2", nz(im["WOFF"], im["WZ1B2"]), "diffWtint", nz(im["WOFF"], im["WZ1B0"]),
      "wdet", nz(im["WZ1B2"], im["WZ1B2b"]), "wtr", nz(im["WZ1B2"], im["WT"]), "wreal", (nz(im["WZ1B2"], im["WR"]) if "WR" in im else 0), "psnrGL", round(psnr(im["WG"], im["WZ1B2"]), 2), "psnrVK", round(psnr(im["WV"], im["WZ1B2"]), 2), "total", 640 * 360)
json.dump(res, open(f"{W}/wstats.json", "w"))
for a, b, nm in (("WOFF", "WZ1B2", "cmp_water_tint"), ("WZ0B0", "WZ1B0", "cmp_water_zoom")):
    c = Image.new("RGB", (1280, 360)); c.paste(im[a], (0, 0)); c.paste(im[b], (640, 0)); c.save(f"{W}/{nm}.png")
    d = ImageChops.difference(im[a], im[b]).point(lambda v: min(255, v * 4)); d.save(f"{W}/diff_{nm}.png")
PY
) || WRES=""
    echo "  BW $WRES"
    if [ -z "$WRES" ]; then echo "  FAIL  water picture analysis failed"; ERR=1; else
        python3 -I - "$W/wstats.json" "$WRES" <<'PY' || ERR=1
import sys, json
st = json.load(open(sys.argv[1])); t = sys.argv[2].split()
ok = True
def bad(m):
    global ok; ok = False; print("  FAIL ", m)
v = {t[i]: int(t[i + 1]) for i in range(len(t) - 1) if t[i] in ("diffWB0", "diffWB2", "diffWtint", "wdet", "wtr", "wreal", "total") and t[i + 1].lstrip("-").isdigit()}
pg = [float(t[i + 1]) for i in range(len(t) - 1) if t[i] in ("psnrGL", "psnrVK")]
if len(pg) != 2 or min(pg) < 30.0: bad(f"GL / Vulkan water picture vs CPU: PSNR {pg} dB (need >= 30; the zoom border differs by sub pixel edge rules, see r261)")
def dist(a, b): return max(abs(a[c] - b[c]) for c in range(3))
# (b) tint off: one flat colour left and right of the biome border, no border rows
o = st["WOFF"]
if dist(o["L"], o["R"]) > 6 or dist(o["L2"], o["R2"]) > 6: bad(f"tint OFF: water colour differs left/right ({o['L']} vs {o['R']})")
if o["rows"] >= 10: bad(f"tint OFF: found a biome border in {o['rows']} rows")
# (a) tint on: water colour follows the biome (swamp 617B64 left, warm_ocean 43D5EE right), border straight without zoom, jagged with zoom
for n in ("WZ0B0", "WZ1B0", "WZ1B2", "WZ1B2b", "WG", "WV"):
    s = st[n]
    if dist(s["L"], s["R"]) < 40: bad(f"{n}: water colour does not follow the biome ({s['L']} vs {s['R']})")
    # looking south the screen left is +x: warm_ocean 43D5EE (67,213,238) left, swamp 617B64 (97,123,100) right; the white texture takes the exact tint colour (+-3)
    if dist(s["L"], (67, 213, 238)) > 3 or dist(s["R"], (97, 123, 100)) > 3: bad(f"{n}: water colour is not the biome tint (left {s['L']} want 43D5EE, right {s['R']} want 617B64)")
a = st["WZ0B0"]; b = st["WZ1B0"]
if a["rows"] < 60 or b["rows"] < 60: bad(f"water: too few border rows (zoom off {a['rows']}, on {b['rows']})")
if a["range"] > 1.5: bad(f"water zoom OFF border is not straight (p5..p95 range {a['range']} px)")
if b["range"] < 6.0: bad(f"water zoom ON border is not jagged (range {b['range']} px, need >= 6)")
if b["distinct"] < 2: bad("water zoom ON has only one border position")
if v.get("diffWB0", 0) < 500: bad(f"water zoom ON vs OFF differ by only {v.get('diffWB0')} px")
if v.get("diffWtint", 0) < 20000: bad(f"tint ON vs OFF differ by only {v.get('diffWtint')} px")
if v.get("wdet", 1) != 0: bad(f"water zoom ON is not deterministic: {v.get('wdet')} px")
w = st["WT"]
if w["rows"] < 60: bad(f"translucent water: border found in only {w['rows']} rows")
if v.get("wtr", 0) < 20000: bad(f"translucent water picture equals the opaque one ({v.get('wtr')} px differ)")
if "WR" in st:
    r = st["WR"]  # real vanilla texture: grey texels x tint, alpha 180 over the stone floor; the biome border must stay visible and differ from the white fixture
    if dist(r["L"], r["R"]) < 20: bad(f"real water_still: colour does not follow the biome ({r['L']} vs {r['R']})")
    if r["rows"] < 60: bad(f"real water_still: border found in only {r['rows']} rows")
    if v.get("wreal", 0) < 20000: bad(f"real water_still picture equals the fixture one ({v.get('wreal')} px differ)")
    if min(r["L"]) < 20 or max(r["L"]) > 250: bad(f"real water_still: implausible water colour {r['L']}")
sys.exit(0 if ok else 1)
PY
    fi
fi
if [ -n "${BZ_KEEP_DIR:-}" ]; then
    mkdir -p "$BZ_KEEP_DIR"; cp "$W"/cmp_*.png "$W"/diff_*.png "$W/stats.json" "$W/wstats.json" "$BZ_KEEP_DIR"/ 2>/dev/null
fi
[ $ERR -eq 0 ] && echo "RESULT mc-play-biomezoom: ok" || echo "RESULT mc-play-biomezoom: FAIL"
exit $ERR
