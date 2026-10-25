#!/usr/bin/env bash
# tools/mc/vk/run.sh -- Vulkan lifecycle gate (lib/gpu/vk_life.fi, docs/VK-LIFE.md; roadmap r179). Lavapipe (CPU renderer) + Xvfb,
# NO real GPU: nothing here is a GPU result.
#   1. tests 3100 (pure), 3101 (pipeline cache round trip, damaged / foreign files), 3102 (device lost + recovery)
#   2. lifeprobe.fi on an Xvfb window: resize, OUT_OF_DATE, SUBOPTIMAL, minimised, SURFACE_LOST, DEVICE_LOST, each vs an offscreen render
#   3. tools/gpu/vk/run.sh (offscreen triangles vs mc_raster, PSNR >= 34 dB, ms/frame, swapchain) = the regression of vk.fi
#   4. Windows builds (vkplat.windows.fi + vk_life) compile only; pure test also runs under Wine when available
#   5. voxel / scene / model layers vs mc_raster (PSNR >= 34 dB) through vk_mc: vkmc_run.sh, vkscene_run.sh, vkmodel_run.sh
#   6. Windows cross build of demos/mcplay (vk_mc + vkplat.windows) compiles (compile only, no Windows Vulkan device here)
# SKIP (exit 0) without libvulkan / ICD. Usage: bash tools/mc/vk/run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc   # worktrees have no built compiler (r194)
export FIRNLIB="$(pwd)/lib" LP_NUM_THREADS=${LP_NUM_THREADS:-4}
[ -e /usr/lib/x86_64-linux-gnu/libvulkan.so.1 ] || [ -e /usr/lib64/libvulkan.so.1 ] || { echo "   SKIP: no libvulkan.so.1"; exit 0; }
D=$(mktemp -d); XP=""
trap '[ -n "$XP" ] && kill "$XP" 2>/dev/null; WINEPREFIX="$D/wp" wineserver -k 2>/dev/null; rm -rf "$D"' EXIT
fail=0
for t in 3100_mc_vk_life_pure 3101_mc_vk_life_cache 3102_mc_vk_life_lost; do
    if ! "$FIRNC" -o "$D/$t" "tests/$t.fi" 2>"$D/err"; then echo "   FAIL: compile $t"; head -5 "$D/err"; fail=1; continue; fi
    out=$(timeout 120 "$D/$t" 2>&1); rc=$?
    echo "$out" | sed 's/^/   /'
    if [ "$rc" -eq 0 ]; then echo "   ok:   $t"; else echo "   FAIL: $t exit $rc"; fail=1; fi
done
# X11 lifecycle probe
export XDG_RUNTIME_DIR="$D"
if command -v Xvfb >/dev/null 2>&1; then
    mkfifo "$D/disp"; Xvfb -displayfd 3 -screen 0 800x600x24 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 & XP=$!
    if read -r -t 20 n <"$D/disp"; then export DISPLAY=":$n"; else unset DISPLAY; fi
else
    unset DISPLAY
fi
if "$FIRNC" -o "$D/lifeprobe" tools/mc/vk/lifeprobe.fi 2>"$D/err"; then
    timeout 240 "$D/lifeprobe" >"$D/life.txt" 2>&1; rc=$?
    grep -E "FAIL|differing|rebuilds|SUBOPTIMAL frames|RESULT|SKIP|failed" "$D/life.txt" | sed 's/^/   /'
    if [ "$rc" -eq 0 ]; then echo "   ok:   lifeprobe ($(grep -c '^ok ' "$D/life.txt") checks: resize, OUT_OF_DATE, SUBOPTIMAL, minimised, SURFACE_LOST, DEVICE_LOST + recovery, xlib + xcb)"
    elif [ "$rc" -eq 4 ]; then echo "   SKIP: lifeprobe (no DISPLAY / no device / no swapchain)"
    else echo "   FAIL: lifeprobe exit $rc"; fail=1; fi
else
    echo "   FAIL: compile lifeprobe.fi"; head -5 "$D/err"; fail=1
fi
# regression of the base layer + ms/frame of the probe triangle
bash tools/gpu/vk/run.sh "$FIRNC" 2>&1 | grep -E "ok:|FAIL|SKIP|us/frame|psnr=" | sed 's/^/   /'
[ "${PIPESTATUS[0]}" -eq 0 ] || fail=1
# Windows: compile only
for f in tools/mc/vk/lifeprobe.fi tests/3101_mc_vk_life_cache.fi demos/mcplay/main.fi; do
    if "$FIRNC" --target=x86_64-windows --opt-level=dev-fast -o "$D/w.exe" "$f" 2>"$D/err"; then echo "   ok:   Windows build of $(basename "$f") (compile only)"; else echo "   FAIL: Windows build of $f"; head -3 "$D/err"; fail=1; fi
done
WINE=$(command -v wine64 || command -v wine || true)
if [ -n "$WINE" ] && "$FIRNC" --target=x86_64-windows -o "$D/p.exe" tests/3100_mc_vk_life_pure.fi 2>/dev/null; then
    if WINEPREFIX="$D/wp" WINEDEBUG=-all timeout -s KILL 90 "$WINE" "$D/p.exe" 2>/dev/null | tr -d '\r' | grep -q "vk_life pure ok"; then echo "   ok:   pure test 3100 under Wine"; else echo "   FAIL: pure test under Wine"; fail=1; fi
fi
if [ -e lib/mc/vk_mc.fi ]; then
    bash tools/gpu/vk/vkmc_run.sh "$FIRNC" || fail=1
    bash tools/gpu/vk/vkscene_run.sh "$FIRNC" || fail=1   # sky / tint / HUD / entities vs mc_sky + mc_hud + mc_raster (r180)
    bash tools/gpu/vk/vkmodel_run.sh "$FIRNC" || fail=1   # model quads MESH_MODEL / MESH_MODEL_T vs mc_raster (r241)
else
    echo "   NOTE: lib/mc/vk_mc.fi not on this branch: voxel scene PSNR vs mc_raster not run (vkprobe covers flat + gradient triangles only)"
fi
[ "$fail" -eq 0 ] && echo "RESULT vk-life: ok" || echo "RESULT vk-life: FAIL"
exit "$fail"
