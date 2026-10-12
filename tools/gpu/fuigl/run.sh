#!/usr/bin/env bash
# tools/gpu/fuigl/run.sh -- fUi host over a GL window (lib/plat/fuiwingl.fi, docs/GPU.md "fUi over GL", roadmap r422):
#   fuigl.fi (= hello_lowlevel plus a switch) under Xvfb + Mesa llvmpipe, three runs:
#     cpu      the old path (RGBA -> BGRX -> PutImage)
#     gl       the same window through the present hook (glshow.show_pixels + EGL/GLX swap on the window's XID)
#     glbreak  GL attached, then the hook fails on purpose -> fuiwin must drop the hook and paint on the CPU
#   The X server's own picture (xwd -root) is compared: GL screenshot == its canvas, == CPU screenshot (PSNR >= 34, in
#   practice byte equal). Then the repaint time of both paths (ms/frame, Mesa llvmpipe ONLY: a real GPU is NOT measured).
#   SKIP (exit 0) without libEGL / Xvfb / xwd / python3. Usage: bash tools/gpu/fuigl/run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libEGL.so.1 ] && command -v Xvfb >/dev/null 2>&1 && command -v xwd >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1 \
    && command -v python3 >/dev/null 2>&1 || { echo "   SKIP: no libEGL / Xvfb / xwd / xdotool / python3"; exit 0; }
D=$(mktemp -d); XP=""; PP=""
trap 'WINEPREFIX="$D/wineprefix" wineserver -k >/dev/null 2>&1; [ -n "$PP" ] && kill "$PP" 2>/dev/null; [ -n "$XP" ] && kill "$XP" 2>/dev/null; rm -rf "$D"' EXIT
"$FIRNC" -o "$D/fuigl" tools/gpu/fuigl/fuigl.fi 2>"$D/err" || { echo "   FAIL: compile fuigl.fi"; head -5 "$D/err"; exit 1; }
mkfifo "$D/disp"; Xvfb -displayfd 3 -screen 0 1400x900x24 -dpi 96 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 & XP=$!
read -r -t 20 n <"$D/disp" || { echo "   SKIP: Xvfb did not start"; exit 0; }
export DISPLAY=":$n"
fail=0
# one run: mode, bench frames, width, height; leaves $D/<tag>/{out.txt,shot.xwd,canvas.rgba}
run() {
    local tag=$1 mode=$2 frames=$3 w=$4 h=$5 rs=${6:-0} hold=${7:-40}
    mkdir -p "$D/$tag"
    (cd "$D/$tag" && exec timeout 120 "$D/fuigl" "$mode" "$frames" "$w" "$h" "$hold" >out.txt 2>&1) & PP=$!
    local i
    for i in $(seq 1 150); do grep -q READY "$D/$tag/out.txt" 2>/dev/null && break; sleep 0.2; done
    grep -q READY "$D/$tag/out.txt" 2>/dev/null || { echo "   FAIL: $tag did not reach READY"; head -5 "$D/$tag/out.txt"; fail=1; }
    xwd -root -silent -out "$D/$tag/shot.xwd"
    # resize the window (EV_CONFIGURE -> new canvas, GL: new texture + viewport), second screenshot
    local wid; wid=$(xdotool search --name "fUi over GL" 2>/dev/null | head -1)
    if [ "$rs" = 1 ] && [ -n "$wid" ]; then xdotool windowsize "$wid" 500 300; sleep 1.2; xwd -root -silent -out "$D/$tag/shot2.xwd"; fi
    wait "$PP" 2>/dev/null; PP=""
}
run cpu cpu 50 640 400 1
run gl gl 50 640 400 1
run glb glbreak 50 640 400
sed 's/^/   cpu: /' "$D/cpu/out.txt" | head -3
sed 's/^/   gl:  /' "$D/gl/out.txt" | head -3
grep -q "^path=gl api=" "$D/gl/out.txt" && grep -q "hooked=true" "$D/gl/out.txt" && grep -q "^DONE" "$D/gl/out.txt" \
    && echo "   ok:   fUi window really presents through GL" || { echo "   FAIL: the gl run did not use GL (see above)"; fail=1; }
if python3 -I tools/gpu/fuigl/cmp.py "$D/cpu" "$D/gl" 640 400 34 > "$D/cmp.txt" 2>&1; then
    cat "$D/cmp.txt"; echo "   ok:   GL screenshot equals the CPU path (PSNR >= 34)"
else cat "$D/cmp.txt"; echo "   FAIL: GL picture differs from the CPU path"; fail=1; fi
if python3 -I tools/gpu/fuigl/cmp.py "$D/cpu" "$D/gl" 500 300 34 2 > "$D/cmp2.txt" 2>&1; then
    cat "$D/cmp2.txt"; echo "   ok:   after a window resize (640x400 -> 500x300) GL still equals the CPU path"
else cat "$D/cmp2.txt"; echo "   FAIL: GL picture after resize differs from the CPU path"; fail=1; fi
# fallback: hook failed -> hook dropped, CPU path painted the frame, picture still right
if grep -q "hooked=false" "$D/glb/out.txt" && python3 -I tools/gpu/fuigl/cmp.py "$D/cpu" "$D/glb" 640 400 34 > "$D/cmpb.txt" 2>&1; then
    echo "   ok:   failing GL hook falls back to the CPU path (picture intact)"
else cat "$D/cmpb.txt" 2>/dev/null; head -5 "$D/glb/out.txt"; echo "   FAIL: GL hook failure did not fall back cleanly"; fail=1; fi
# repaint time (forced repaints incl. fUi painting), llvmpipe only
# 3 rounds each, alternating, the minimum counts (the machine is shared)
bc=999999999; bg=999999999
for r in 1 2 3; do
    run bcpu cpu 200 1280 800 0 3; run bgl gl 200 1280 800 0 3
    for t in bcpu bgl; do grep -q "^bench " "$D/$t/out.txt" || { echo "   FAIL: no bench line from $t"; fail=1; }; done
    x=$(sed -n 's/^bench .*us_per_frame=\([0-9]*\).*/\1/p' "$D/bcpu/out.txt"); [ -n "$x" ] && [ "$x" -lt "$bc" ] && bc=$x
    x=$(sed -n 's/^bench .*us_per_frame=\([0-9]*\).*/\1/p' "$D/bgl/out.txt"); [ -n "$x" ] && [ "$x" -lt "$bg" ] && bg=$x
done
echo "   metric fuigl 1280x800 us/frame, min of 3 (Mesa llvmpipe, NOT a real GPU): cpu=$bc gl=$bg"
# ---- Windows twin (lib/plat/fuiwingl.windows.fi: WGL) under Wine + Xvfb + Mesa: path=gl api=wgl, hook in use, same canvas as CPU
WINE=""
for c in wine wine64 /usr/lib/wine/wine64; do
    if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
done
if [ -z "$WINE" ] || ! command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
    echo "   SKIP: Windows twin (wine or the mingw linker is missing)"
elif ! "$FIRNC" --target=x86_64-windows -o "$D/fuigl.exe" tools/gpu/fuigl/fuigl.fi 2>"$D/errw"; then
    echo "   FAIL: compile fuigl.fi for Windows"; head -5 "$D/errw"; fail=1
else
    for m in cpu gl; do
        mkdir -p "$D/w$m"
        (cd "$D/w$m" && timeout 240 env WINEPREFIX="$D/wineprefix" WINEDEBUG=-all "$WINE" "$D/fuigl.exe" $m 30 640 400 2 >out.txt 2>&1)
    done
    sed 's/^/   wine gl: /' "$D/wgl/out.txt" | grep -v "^   wine gl: \(READY\|size2\)"
    if grep -q "^path=gl api=wgl" "$D/wgl/out.txt" && grep -q "bench .*hooked=true" "$D/wgl/out.txt" && grep -q "^DONE" "$D/wgl/out.txt" \
        && grep -q "^DONE" "$D/wcpu/out.txt" && cmp -s "$D/wcpu/canvas.rgba" "$D/wgl/canvas.rgba"; then
        echo "   ok:   fUi window over WGL (Wine): hook in use, canvas identical to the CPU path"
    else
        echo "   FAIL: fUi window over WGL under Wine"; head -6 "$D/wgl/out.txt"; fail=1
    fi
    WINEPREFIX="$D/wineprefix" wineserver -k >/dev/null 2>&1
fi
[ "$fail" -eq 0 ] && echo "RESULT fuigl: ok" || echo "RESULT fuigl: FAIL"
exit "$fail"
