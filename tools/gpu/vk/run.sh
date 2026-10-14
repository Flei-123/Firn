#!/usr/bin/env bash
# tools/gpu/vk/run.sh -- gpu.vk (lib/gpu/vk.fi, docs/GL-VS-VULKAN.md): Vulkan loader via dynlib, instance, device, offscreen triangles vs
# mc_raster / a float CPU reference (PSNR >= 34 dB), swapchain on an X11 window (xlib + xcb surface, present, XGetImage, resize),
# the "no loader" error path, Windows build (vulkan-1.dll twin, compile only). SKIP (exit 0) without libvulkan / an ICD / Xvfb.
# Usage: bash tools/gpu/vk/run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libvulkan.so.1 ] || [ -e /usr/lib64/libvulkan.so.1 ] || { echo "   SKIP: no libvulkan.so.1"; exit 0; }
D=$(mktemp -d); XP=""
trap '[ -n "$XP" ] && kill "$XP" 2>/dev/null; rm -rf "$D"' EXIT
"$FIRNC" -o "$D/vkp" tools/gpu/vk/vkprobe.fi 2>"$D/err" || { echo "   FAIL: compile vkprobe.fi"; head -5 "$D/err"; exit 1; }
export XDG_RUNTIME_DIR="$D"
if command -v Xvfb >/dev/null 2>&1; then
    mkfifo "$D/disp"; Xvfb -displayfd 3 -screen 0 800x600x24 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 & XP=$!
    if read -r -t 20 n <"$D/disp"; then export DISPLAY=":$n"; else unset DISPLAY; fi
else
    unset DISPLAY
fi
timeout 180 "$D/vkp" >"$D/out.txt" 2>&1; rc=$?
sed 's/^/   /' "$D/out.txt"
fail=0
if [ "$rc" -eq 4 ]; then echo "   SKIP: no Vulkan device (no ICD installed)"; exit 0; fi
if [ "$rc" -eq 0 ] && grep -q "^RESULT fails=0" "$D/out.txt"; then
    echo "   ok:   Vulkan offscreen triangles vs CPU ($(grep -c 'compare: psnr' "$D/out.txt") comparisons within PSNR >= 34 dB)"
else
    echo "   FAIL: vkprobe exit $rc"; fail=1
fi
if grep -q "^swapchain xlib" "$D/out.txt" && grep -q "^swapchain xcb" "$D/out.txt"; then
    echo "   ok:   swapchain on an X11 window (xlib + xcb), window read back, resize"
elif grep -q "^SKIP swapchain" "$D/out.txt"; then
    echo "   SKIP: swapchain (no DISPLAY / no VK_KHR_swapchain)"
fi
# error path: no loader -> honest false
FIRN_VK_LIB=libvulkan-does-not-exist.so.9 timeout 60 "$D/vkp" nolib >"$D/nolib.txt" 2>&1; rc=$?
sed 's/^/   /' "$D/nolib.txt"
if [ "$rc" -eq 0 ]; then echo "   ok:   no Vulkan loader -> open() false, STEP_NO_LOADER"; else echo "   FAIL: nolib path exit $rc"; fail=1; fi
# Windows twin: compile only
if "$FIRNC" --target=x86_64-windows -o "$D/vkp.exe" tools/gpu/vk/vkprobe.fi 2>"$D/errw"; then
    echo "   ok:   Windows build (vulkan-1.dll twin, not run)"
else
    echo "   FAIL: Windows build of vkprobe.fi"; head -5 "$D/errw"; fail=1
fi
# MC renderer shaders (docs/VK-RENDERER.md (Shaders)): committed .spv == fresh glslang build (byte equal) + GL parity diff. SKIP without glslang.
if command -v glslangValidator >/dev/null 2>&1 && command -v spirv-val >/dev/null 2>&1; then
    if bash tools/gpu/mkspv.sh --check >"$D/mkspv.txt" 2>&1; then echo "   ok:   Vulkan shaders: committed SPIR-V == fresh build ($(grep -c '^ok' "$D/mkspv.txt") modules)"
    else echo "   FAIL: tools/gpu/mkspv.sh --check"; sed 's/^/   /' "$D/mkspv.txt" | head; fail=1; fi
    if bash tools/gpu/mkspv.sh --parity >"$D/parity.txt" 2>&1; then echo "   ok:   Vulkan shaders vs GL shaders: $(tail -1 "$D/parity.txt")"
    else echo "   FAIL: shader parity"; sed 's/^/   /' "$D/parity.txt" | head -20; fail=1; fi
else
    echo "   SKIP: Vulkan shader rebuild check (no glslangValidator / spirv-val)"
fi
exit $fail
