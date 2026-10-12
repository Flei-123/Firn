#!/usr/bin/env bash
# tools/gpu/win/run.sh -- GL 3.3 core WINDOW tests (lib/gpu/glwin.fi, docs/GPU.md "GL window"):
#   winprobe.fi under Xvfb (-dpi 120) + Mesa llvmpipe, once through EGL (window surface on the Firn window's XID) and
#   once through GLX: open, clear + read back the window framebuffer, swap, swap interval, resize (surface size),
#   full screen request, DPI, ms/frame. SKIP (exit 0) without libEGL / Xvfb. Usage: bash tools/gpu/win/run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libEGL.so.1 ] && command -v Xvfb >/dev/null 2>&1 || { echo "   SKIP: no libEGL / Xvfb"; exit 0; }
D=$(mktemp -d); XP=""
trap '[ -n "$XP" ] && kill "$XP" 2>/dev/null; rm -rf "$D"' EXIT
"$FIRNC" -o "$D/wp" tools/gpu/win/winprobe.fi 2>"$D/err" || { echo "   FAIL: compile winprobe.fi"; head -5 "$D/err"; exit 1; }
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
[ "$fail" -eq 0 ] && echo "RESULT glwin: ok" || echo "RESULT glwin: FAIL"
exit "$fail"
