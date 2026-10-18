#!/usr/bin/env bash
# tools/gpu/run.sh -- lib/gpu tests (docs/GPU.md):
#   1. smoke.fi: headless EGL GL 3.3 core context (Mesa llvmpipe), clear, readback
#   2. dump.fi: the OpenGL back end (lib/gpu/gl_mc.fi, shaders mc_vs/mc_fs.glsl) against the CPU rasterizer
#      (lib/mc/mc_raster.fi) on 8 scenes (triangle, textured quad x2, cut-out, voxel chunk, fog, camera inside, far
#      from the origin) + 3 translucent / entity scenes water_glass, water_glass_fog, boxes (PASS_TRANS, instanced entity boxes
#      through gl_scene.scene_attach, r94; PSNR ~42 dB measured on llvmpipe, threshold 38): coverage mismatch + PSNR thresholds; ms/frame GPU (llvmpipe) vs CPU raster
#   2b. model_dump.fi: MESH_MODEL / MESH_MODEL_T quads (own program mc_model_vs/fs.glsl) against mc_raster on 7 scenes (PSNR >= 34 dB)
#   3. the same dump.fi built for Windows (lib/gpu/glctx.windows.fi, wgl) under Wine + Xvfb; its scene lines must be
#      IDENTICAL to the Linux ones
# Prints SKIP (exit 0) when libEGL is missing / no Wine. Usage: bash tools/gpu/run.sh [firnc]
set -u
cd "$(dirname "$0")/../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
D=$(mktemp -d)
XPID=""
cleanup() {
    if [ -d "$D/wineprefix" ] && command -v wineserver >/dev/null 2>&1; then
        WINEPREFIX="$D/wineprefix" wineserver -k 2>/dev/null
        sleep 1
    fi
    [ -n "$XPID" ] && kill "$XPID" 2>/dev/null
    rm -rf "$D" 2>/dev/null || { sleep 2; rm -rf "$D"; }
}
trap cleanup EXIT
fail=0
[ -e /usr/lib/x86_64-linux-gnu/libEGL.so.1 ] || { echo "   SKIP: no libEGL.so.1"; exit 0; }

"$FIRNC" -o "$D/smoke" tools/gpu/smoke.fi || { echo "   FAIL: compile smoke.fi"; exit 1; }
env -u DISPLAY -u EGL_PLATFORM timeout 60 "$D/smoke" | sed 's/^/   /'; rc=${PIPESTATUS[0]}
if [ "$rc" -eq 0 ]; then echo "   ok:   GL 3.3 core context, clear + readback"; else echo "   FAIL: smoke exit $rc"; exit 1; fi

# ---- GL back end vs CPU rasterizer (Linux, headless)
if ! "$FIRNC" -o "$D/dump" tools/gpu/dump.fi 2>"$D/err"; then
    echo "   FAIL: dump.fi does not compile"; head -5 "$D/err"; exit 1
fi
env -u DISPLAY -u EGL_PLATFORM timeout 300 "$D/dump" > "$D/linux.txt" 2>"$D/linux.err"; rc=$?
sed 's/^/   /' "$D/linux.txt"
if [ "$rc" -eq 0 ]; then
    echo "   ok:   GL back end vs mc_raster: $(grep -c '^scene .* ok$' "$D/linux.txt") scenes within the PSNR / coverage thresholds"
else
    echo "   FAIL: dump exit $rc"; head -3 "$D/linux.err"; fail=1
fi

# ---- model quads (MESH_MODEL / MESH_MODEL_T, mc_model_vs/fs.glsl) vs mc_raster
if ! "$FIRNC" -o "$D/model_dump" tools/gpu/model_dump.fi 2>"$D/err"; then
    echo "   FAIL: model_dump.fi does not compile"; head -5 "$D/err"; fail=1
else
    env -u DISPLAY -u EGL_PLATFORM timeout 300 "$D/model_dump" > "$D/model.txt" 2>"$D/model.err"; rc=$?
    sed 's/^/   /' "$D/model.txt"
    if [ "$rc" -eq 0 ]; then
        echo "   ok:   GL model quads vs mc_raster: $(grep -c '^scene .* ok$' "$D/model.txt") scenes (tint, AO, light, cut-out + fog, translucent sorted)"
    else
        echo "   FAIL: model_dump exit $rc"; head -3 "$D/model.err"; fail=1
    fi
fi

# ---- the same program as a Windows exe under Wine (wgl through the Win64 call gate)
WINE=""
for c in wine wine64 /usr/lib/wine/wine64; do
    if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
done
if [ -z "$WINE" ] || ! command -v x86_64-w64-mingw32-ld >/dev/null 2>&1 || ! command -v Xvfb >/dev/null 2>&1; then
    echo "   SKIP: Windows twin (wine, the mingw linker or Xvfb is missing)"
else
    mkfifo "$D/disp" && { Xvfb -displayfd 3 -screen 0 800x600x24 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 & XPID=$!; }
    read -r -t 20 n <"$D/disp" || n=""
    if [ -z "$n" ]; then
        echo "   SKIP: Windows twin (Xvfb did not start)"
    elif ! "$FIRNC" --target=x86_64-windows -o "$D/dump.exe" tools/gpu/dump.fi 2>"$D/err"; then
        echo "   FAIL: dump.fi does not compile for Windows"; head -5 "$D/err"; fail=1
    else
        WINEPREFIX="$D/wineprefix" WINEDEBUG=-all DISPLAY=":$n" timeout -s KILL 300 "$WINE" "$D/dump.exe" > "$D/win.txt" 2>/dev/null; rc=$?
        grep -E '^(metric|renderer)' "$D/win.txt" | sed 's/^/   wine: /'
        if [ "$rc" -ne 0 ]; then
            echo "   FAIL: dump.exe under Wine exit $rc"; fail=1
        elif ! diff <(grep '^scene ' "$D/linux.txt") <(grep '^scene ' "$D/win.txt" | tr -d '\r') >/dev/null; then
            echo "   FAIL: the Windows build renders other pixels than the Linux build"
            diff <(grep '^scene ' "$D/linux.txt") <(grep '^scene ' "$D/win.txt" | tr -d '\r') | head -6
            fail=1
        else
            echo "   ok:   Windows twin under Wine: same scene statistics as Linux (wgl + Win64 call gate)"
        fi
    fi
fi
[ "$fail" -eq 0 ] && echo "RESULT gpu: ok" || echo "RESULT gpu: FAIL"
exit "$fail"
