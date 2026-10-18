#!/usr/bin/env bash
# tools/mc/play/run_lodseam.sh -- models + LOD together (r254): seam full ring / LOD ring, ring change LOD -> full, frame times.
# Official 1.20.1 server (seed 4242, view-distance 10 so the LOD ring is wide), Xvfb + mcplay + xdotool. Fixture pack as in run_models.sh (pure quadrant
# colours, stairs model). Stairs float at y=100: NEAR set at z=5 (full ring), FAR set at z=64 (chunk 4 = LOD ring while the player stands at spawn, view 3).
# The player flies at y=100, looks south, pitch 20 (tp ... 0 20).
# Seam runs at spawn (VIEW=3 LOD=9): A = models + LOD, B = cubes + LOD (MCPLAY_MODELS=0), B2 = like B (noise baseline).
#   Checks: A and B differ only around the NEAR stairs (pixels outside the dilated bbox of the fixture colours: 0, i.e. no hole / double drawing / colour
#   jump at the seam), B == B2 (determinism of the LOD picture), A has LOD columns and models, the stairs show the 4 fixture colours.
# Ring change: R = like A, but first at spawn (FAR stairs are LOD cubes), then tp to z=59.5 (FAR stairs become full ring -> remeshed with models);
#   S = tp straight to z=59.5; T = like S with cubes (MCPLAY_MODELS=0). Checks: R and S show the 4 fixture colours, R differs from T (>= 0.2 % of the
#   pixels: the models appeared), R == S (the late remesh gives the same picture as a direct load).
# Perf (LODSEAM_PERF=1, default 0): frame work time p50 (MCPLAY_FRAME_STATS, ms, CPU raster path) over LODSEAM_REPS (3) runs per
#   config: ML = models + LOD (view 3), MN = models without LOD (view 9, same visible range), CL = cubes + LOD (view 3). Median over the runs is printed
#   (`PERF ML p50_ms=.. MN .. CL ..`) with the load average; NO GPU. Start it via heavy (java server).
# Prints `RESULT mc-play-lodseam: ok|FAIL`. Env: FIRNC, SEAM_KEEP_DIR (copy of outs/pngs/server.log), SEAM_RUN_S (40), SEAM_PERF_RUN_S (35).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""
keep_logs() { [ -n "${SEAM_KEEP_DIR:-}" ] && { mkdir -p "$SEAM_KEEP_DIR"; cp "$W"/*.out "$W"/*.png "$W"/pack.zip "$W/server.log" "$SEAM_KEEP_DIR"/ 2>/dev/null; }; }
cleanup() { keep_logs; exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-lodseam: FAIL"; exit 1; }
for t in Xvfb xdotool java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
python3 -I -c "import PIL" 2>/dev/null || fail "missing python module: PIL"
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
python3 -I - "$W/pack.zip" <<'PY' || fail "cannot build the fixture pack"
import sys, zipfile, io, json
from PIL import Image
im = Image.new("RGBA", (16, 16))
for y in range(16):
    for x in range(16):
        c = (255, 0, 0) if (y < 8 and x < 8) else (0, 255, 0) if (y < 8) else (0, 0, 255) if (x < 8) else (255, 255, 0)
        im.putpixel((x, y), c + (255,))
png = io.BytesIO(); im.save(png, "PNG")
tex = {"bottom": "block/oak_planks", "top": "block/oak_planks", "side": "block/oak_planks"}
model = json.dumps({"parent": "block/stairs", "textures": tex})
def var(y):
    v = {"model": "block/mdl_stairs"}
    if y: v.update({"y": y, "uvlock": True})
    return v
bs = json.dumps({"variants": {"facing=east,half=bottom,shape=straight": var(0), "facing=south,half=bottom,shape=straight": var(90),
    "facing=west,half=bottom,shape=straight": var(180), "facing=north,half=bottom,shape=straight": var(270)}})
with zipfile.ZipFile(sys.argv[1], "w") as z:
    z.writestr("pack.mcmeta", json.dumps({"pack": {"pack_format": 15, "description": "mcplay lodseam gate fixture"}}))
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
level-seed=4242
view-distance=10
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
# stairs on the server before any client joins (r244: they arrive with the chunk data)
NSB=$(wc -l < "$W/server.log")
for z in 5 64; do
    for spec in "-2 east" "0 south" "2 west" "4 north"; do
        set -- $spec
        echo "setblock $1 100 $z oak_stairs[facing=$2]" >&3
    done
done
for i in $(seq 1 40); do [ "$(tail -n +"$((NSB + 1))" "$W/server.log" | grep -c 'Changed the block')" -ge 8 ] && break; sleep 1; done
[ "$(tail -n +"$((NSB + 1))" "$W/server.log" | grep -c 'Changed the block')" -ge 8 ] || fail "server setblock of the stairs (need 8, FAR chunk loaded?)"
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
wait_log() { local i; for i in $(seq 1 "$2"); do tail -n +"$(( ${3:-0} + 1 ))" "$W/server.log" | grep -qE "$1" && return 0; sleep 1; done; return 1; }
# One run: join, fly (F), tp to spawn; $2 = hop z (empty = stay at spawn, else tp to (0.5 100 z) after $3 s); the shot is taken at the end.
# A lost join / tp / the player not up at the end = environment flake: repeated (own player name), up to 3 times.
run_one() { # name, hop_z, env...
    local name=$1 hz=$2; shift 2
    local att pn pp rc ok py nl tries
    for att in 1 2 3; do
        pn="L${name}${att}"; ok=1; rm -f "$W/$name.png"
        env FIRN_AUDIO=null MCPLAY_AUDIO=0 MCPLAY_RUN_S=${RUN_S:-40} MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_VIEW=3 \
            MC_PACKS="$W/pack.zip" MCPLAY_SHOT="$W/$name.png" "$@" "$W/mcplay" 127.0.0.1 "$PORT" "$pn" > "$W/$name.out" 2>&1 &
        pp=$!
        if wait_log "$pn joined the game" 120; then
            sleep 3
            xdotool mousemove 320 180 2>/dev/null
            xdotool key f
            tries=0
            while [ $tries -lt 4 ]; do
                nl=$(wc -l < "$W/server.log")
                echo "tp $pn 0.5 100 ${FIRST_Z:-0.5} 0 20" >&3
                wait_log "Teleported $pn to" 10 "$nl" && break
                tries=$((tries + 1))
            done
            [ $tries -lt 4 ] || ok=0
            if [ -n "$hz" ] && [ $ok -eq 1 ]; then
                sleep "${HOP_WAIT:-10}"
                nl=$(wc -l < "$W/server.log")
                echo "tp $pn 0.5 100 $hz 0 20" >&3
                wait_log "Teleported $pn to" 10 "$nl" || ok=0
            fi
        else
            ok=0
        fi
        wait $pp; rc=$?
        py=$(grep -E "^mcplay: pos " "$W/$name.out" | tail -1 | grep -o " y=-\?[0-9]*" | cut -d= -f2)
        [ -n "$py" ] && [ "$py" -ge 80 ] 2>/dev/null || ok=0
        [ $rc -eq 0 ] || { echo "  FAIL  $name exit code $rc"; ERR=1; }
        [ $ok -eq 1 ] && break
        echo "  retry $name (attempt $att): join / tp lost or player not up (end y=${py:-?})"
    done
    [ $ok -eq 1 ] || { echo "  FAIL  $name: player never got up after 3 attempts"; ERR=1; }
    echo "  $name player=$pn end y=${py:-?} load=$(cut -d' ' -f1 /proc/loadavg)"
}
RUN_S=${SEAM_RUN_S:-40}
run_one A "" MCPLAY_MODELS=1 MCPLAY_LOD=9
run_one B "" MCPLAY_MODELS=0 MCPLAY_LOD=9
run_one B2 "" MCPLAY_MODELS=0 MCPLAY_LOD=9
run_one R 59.5 MCPLAY_MODELS=1 MCPLAY_LOD=9
FIRST_Z=59.5 run_one S "" MCPLAY_MODELS=1 MCPLAY_LOD=9
run_one T 59.5 MCPLAY_MODELS=0 MCPLAY_LOD=9
num() { grep -E "mcplay: (end|stream)" "$W/$1.out" | grep -o "$2=[0-9]*" | tail -1 | cut -d= -f2; }
for n in A B B2 R S T; do
    F=$(num $n frames)
    [ "${F:-0}" -gt 20 ] || { echo "  FAIL  $n: too few frames (${F:-?})"; ERR=1; }
    [ -s "$W/$n.png" ] || { echo "  FAIL  $n: no screenshot"; ERR=1; }
done
for n in A R S; do
    grep -q "^mcplay: models on.*lod=9" "$W/$n.out" || { echo "  FAIL  $n: 'models on ... lod=9' line missing"; ERR=1; }
done
for n in B B2 T; do grep -q "^mcplay: models" "$W/$n.out" && { echo "  FAIL  $n (MCPLAY_MODELS=0) printed a models line"; ERR=1; }; done
LA=$(num A lod_cols); echo "  A lod_cols=${LA:-?} lod_tris=$(num A lod_tris)"
[ "${LA:-0}" -gt 0 ] || { echo "  FAIL  A: no LOD columns drawn"; ERR=1; }
if [ $ERR -eq 0 ]; then
    RES=$(python3 -I - "$W" <<'PY'
import sys
from PIL import Image, ImageChops
W = sys.argv[1]
def crop(n): return Image.open(f"{W}/{n}.png").convert("RGB").crop((0, 60, 640, 330))
im = {n: crop(n) for n in ("A", "B", "B2", "R", "S", "T")}
def fixture_mask(i):
    raw = i.tobytes(); w, h = i.size
    m = Image.new("L", (w, h), 0); px = m.load()
    cnt = [0, 0, 0, 0]
    for k in range(0, len(raw), 3):
        r, g, b = raw[k], raw[k + 1], raw[k + 2]
        mx = max(r, g, b)
        if mx < 100: continue
        lo = lambda v: v * 100 < mx * 12
        c = -1
        if r * 100 >= mx * 88 and lo(g) and lo(b): c = 0
        elif g * 100 >= mx * 88 and lo(r) and lo(b): c = 1
        elif b * 100 >= mx * 88 and lo(r) and lo(g): c = 2
        elif r * 100 >= mx * 80 and g * 100 >= mx * 80 and lo(b): c = 3
        if c >= 0:
            cnt[c] += 1; px[(k // 3) % w, (k // 3) // w] = 255
    return cnt, m
def dmask(x, y): return ImageChops.difference(x, y).convert("L").point(lambda v: 255 if v > 0 else 0)
def nz(m): return sum(m.histogram()[255:])
ca, ma = fixture_mask(im["A"]); cb, mb = fixture_mask(im["B"])
cr, _ = fixture_mask(im["R"]); cs, _ = fixture_mask(im["S"])
u = ImageChops.lighter(ma, mb)
bb = u.getbbox()
dab = dmask(im["A"], im["B"])
out_ab = 0
if bb:
    x0, y0, x1, y1 = bb; pad = 8
    r = (max(0, x0 - pad), max(0, y0 - pad), min(640, x1 + pad), min(270, y1 + pad))
    inside = dab.crop(r); out_ab = nz(dab) - nz(inside)
print("fixA", *ca, "fixR", *cr, "fixS", *cs, "diffAB", nz(dab), "outAB", out_ab, "diffBB2", nz(dmask(im["B"], im["B2"])),
      "diffRT", nz(dmask(im["R"], im["T"])), "diffRS", nz(dmask(im["R"], im["S"])), "total", 640 * 270, "bbox", bb)
PY
) || RES=""
    echo "  $RES"
    set -- $RES
    if [ -z "$RES" ]; then echo "  FAIL  picture analysis failed"; ERR=1; else
        for i in 2 3 4 5; do [ "${!i}" -ge 100 ] || { echo "  FAIL  A: fixture colour #$((i - 1)) only ${!i} pixels"; ERR=1; }; done
        for i in 7 8 9 10; do [ "${!i}" -ge 60 ] || { echo "  FAIL  R (ring change LOD -> full): fixture colour #$((i - 6)) only ${!i} pixels"; ERR=1; }; done
        for i in 12 13 14 15; do [ "${!i}" -ge 60 ] || { echo "  FAIL  S (direct): fixture colour #$((i - 11)) only ${!i} pixels"; ERR=1; }; done
        OUTAB=${19}; BB2=${21}; RT=${23}; RS=${25}; TOT=${27}
        [ "$BB2" -le 0 ] || { echo "  FAIL  B vs B2 (cubes + LOD, same config) differ by $BB2 px: the LOD picture is not deterministic"; ERR=1; }
        [ "$OUTAB" -le 0 ] || { echo "  FAIL  A vs B differ by $OUTAB px OUTSIDE the stairs (seam hole / double drawing / colour jump)"; ERR=1; }
        [ "$RT" -ge $((TOT / 500)) ] || { echo "  FAIL  R equals T: models did not appear after the ring change ($RT px)"; ERR=1; }
        [ "$RS" -le 0 ] || { echo "  FAIL  R (late remesh) differs from S (direct load) by $RS px"; ERR=1; }
    fi
fi
# ---- perf: frame work time p50, median over reps
if [ "${LODSEAM_PERF:-0}" = 1 ]; then
    RUN_S=${SEAM_PERF_RUN_S:-35}; REPS=${LODSEAM_REPS:-3}
    : > "$W/perf.txt"
    for rep in $(seq 1 "$REPS"); do
        run_one "PML$rep" "" MCPLAY_MODELS=1 MCPLAY_LOD=9 MCPLAY_FRAME_STATS=1 MCPLAY_FPS=0
        run_one "PMN$rep" "" MCPLAY_MODELS=1 MCPLAY_VIEW=9 MCPLAY_FRAME_STATS=1 MCPLAY_FPS=0
        run_one "PCL$rep" "" MCPLAY_MODELS=0 MCPLAY_LOD=9 MCPLAY_FRAME_STATS=1 MCPLAY_FPS=0
        for k in ML MN CL; do
            P=$(grep -E "^mcplay: frame_stats" "$W/P$k$rep.out" | grep -o 'p50_us=[0-9]*' | cut -d= -f2)
            P95=$(grep -E "^mcplay: frame_stats" "$W/P$k$rep.out" | grep -o 'p95_us=[0-9]*' | cut -d= -f2)
            echo "$k ${P:-0} ${P95:-0} $(cut -d' ' -f1 /proc/loadavg)" >> "$W/perf.txt"
        done
    done
    python3 -I - "$W/perf.txt" <<'PY'
import sys, statistics
d = {}
for l in open(sys.argv[1]):
    k, p, p95, ld = l.split(); d.setdefault(k, []).append((int(p), int(p95), float(ld)))
parts = []
for k in ("ML", "MN", "CL"):
    v = d.get(k, [])
    if not v: parts.append(f"{k} n/a"); continue
    parts.append(f"{k} p50_ms={statistics.median(x[0] for x in v) / 1000:.1f} p95_ms={statistics.median(x[1] for x in v) / 1000:.1f} runs={[x[0] // 1000 for x in v]} load={statistics.median(x[2] for x in v):.0f}")
print("PERF " + " | ".join(parts))
PY
fi
keep_logs
[ $ERR -eq 0 ] && echo "RESULT mc-play-lodseam: ok" || echo "RESULT mc-play-lodseam: FAIL"
exit $ERR
