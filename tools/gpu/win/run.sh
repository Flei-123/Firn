#!/usr/bin/env bash
# tools/gpu/win/run.sh -- GL 3.3 core WINDOW tests (lib/gpu/glwindow.fi, docs/GPU.md "GL window"):
#   winprobe.fi under Xvfb (-dpi 120) + Mesa llvmpipe, once through EGL (window surface on the Firn window's XID) and
#   once through GLX: open, clear + read back the window framebuffer, swap, swap interval, resize (surface size),
#   full screen request, DPI, ms/frame. SKIP (exit 0) without libEGL / Xvfb. Usage: bash tools/gpu/win/run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libEGL.so.1 ] && command -v Xvfb >/dev/null 2>&1 || { echo "   SKIP: no libEGL / Xvfb"; exit 0; }
D=$(mktemp -d); XP=""
trap '[ -n "$XP" ] && kill "$XP" 2>/dev/null; WINEPREFIX="$D/wineprefix" wineserver -k >/dev/null 2>&1; sleep 1; rm -rf "$D"' EXIT
"$FIRNC" -o "$D/wp" tools/gpu/win/winprobe.fi 2>"$D/err" || { echo "   FAIL: compile winprobe.fi"; head -5 "$D/err"; exit 1; }
"$FIRNC" -o "$D/shot" tools/gpu/win/shot.fi 2>"$D/err2" || { echo "   FAIL: compile shot.fi"; head -5 "$D/err2"; exit 1; }
mkfifo "$D/disp"; Xvfb -displayfd 3 -screen 0 800x600x24 -dpi 120 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 & XP=$!
read -r -t 20 n <"$D/disp" || { echo "   SKIP: Xvfb did not start"; exit 0; }
export DISPLAY=":$n"
fail=0
for m in egl glx; do
    timeout 120 "$D/wp" $m >"$D/$m.txt" 2>&1; rc=$?
    sed "s/^/   $m: /" "$D/$m.txt"
    if [ "$rc" -eq 0 ] && grep -q "^api=$m " "$D/$m.txt" && grep -q "size window=320x180 surface=\(320x180\|0x0\)" "$D/$m.txt" \
        && grep -q "resize ok=true window=400x240" "$D/$m.txt" && grep -q "dpi=120" "$D/$m.txt"; then
        echo "   ok:   GL window through $m"
    else
        echo "   FAIL: GL window through $m (exit $rc)"; fail=1
    fi
done
# window framebuffer against the CPU rasterizer (tools/gpu/win/shot.fi): show_pixels byte equal, GL back end PSNR >= 34
for m in egl glx; do
    timeout 120 "$D/shot" $m >"$D/s$m.txt" 2>&1; rc=$?
    sed "s/^/   shot $m: /" "$D/s$m.txt" | grep -v "renderer="
    if [ "$rc" -eq 0 ] && [ "$(grep -c '^shot .* ok$' "$D/s$m.txt")" -eq 3 ] && grep -q "^RESULT fails=0" "$D/s$m.txt"; then
        echo "   ok:   window framebuffer vs mc_raster through $m (3 scenes)"
    else
        echo "   FAIL: window framebuffer vs mc_raster through $m (exit $rc)"; fail=1
    fi
done
# ---- Windows twin (lib/gpu/glwindow.windows.fi: window.gpu_gl -> WGL) under Wine + Xvfb: winprobe.exe wgl
WINE=""
for c in wine wine64 /usr/lib/wine/wine64; do
    if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
done
if [ -z "$WINE" ] || ! command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
    echo "   SKIP: Windows twin (wine or the mingw linker is missing)"
elif ! "$FIRNC" --target=x86_64-windows -o "$D/wpw.exe" tools/gpu/win/winprobe.fi 2>"$D/errw"; then
    echo "   FAIL: compile winprobe.fi for Windows"; head -5 "$D/errw"; fail=1
else
    timeout 240 env WINEPREFIX="$D/wineprefix" WINEDEBUG=-all "$WINE" "$D/wpw.exe" wgl >"$D/wgl.txt" 2>&1; rc=$?
    sed "s/^/   wgl: /" "$D/wgl.txt"
    if [ "$rc" -eq 0 ] && grep -q "^api=wgl " "$D/wgl.txt" && grep -q "resize ok=true window=400x240" "$D/wgl.txt" && grep -q "^RESULT fails=0" "$D/wgl.txt"; then
        echo "   ok:   GL window through wgl (Wine)"
    else
        echo "   FAIL: GL window through wgl under Wine (exit $rc)"; fail=1
    fi
    if "$FIRNC" --target=x86_64-windows -o "$D/shotw.exe" tools/gpu/win/shot.fi 2>"$D/errs"; then
        timeout 240 env WINEPREFIX="$D/wineprefix" WINEDEBUG=-all "$WINE" "$D/shotw.exe" wgl >"$D/swgl.txt" 2>&1; rc=$?
        sed "s/^/   shot wgl: /" "$D/swgl.txt" | grep -v "renderer="
        if [ "$rc" -eq 0 ] && [ "$(grep -c '^shot .* ok$' "$D/swgl.txt")" -eq 3 ] && grep -q "^RESULT fails=0" "$D/swgl.txt"; then
            echo "   ok:   window framebuffer vs mc_raster through wgl (Wine, 3 scenes)"
        else
            echo "   FAIL: window framebuffer vs mc_raster through wgl under Wine (exit $rc)"; fail=1
        fi
    else
        echo "   FAIL: compile shot.fi for Windows"; head -5 "$D/errs"; fail=1
    fi
fi
[ "$fail" -eq 0 ] && echo "RESULT glwin: ok" || echo "RESULT glwin: FAIL"
exit "$fail"
