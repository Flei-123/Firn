#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/fui/gpuhost.sh -- THE HOST SWITCH FUI_GPU=1 in a real window (lib/plat/fuiwinvk.fi, W18-F3), on Xvfb.
#
#     bash tools/fui/gpuhost.sh [--bench]
#
#   gpuhost/main (hello window) runs 5 times on one Xvfb:
#     cpu      FUI_GPU unset                -> CPU painter (the reference)
#     vk       FUI_GPU=1                    -> fUi painted on Vulkan, canvas read back, CPU present; X shot + canvas == CPU (PSNR >= 34),
#                                              also after a window resize (640x400 -> 500x300: the canvas target is made again)
#     lost     FUI_GPU=1 FUI_GPU_FAIL=lost  -> the device "disappears" at the 3rd frame: that frame is painted again on the CPU,
#                                              the GPU is dropped (gpu=false), the picture equals the CPU run bit for bit
#     open     FUI_GPU=1 FUI_GPU_FAIL=open  -> no Vulkan: the window paints on the CPU, bit-equal to the CPU run
#     compile  FUI_GPU=1 FUI_GPU_FAIL=compile -> shader does not build: same
#   --bench adds the repaint time (us per forced frame) at 1280x800 and 1920x1080, CPU vs Vulkan, min of 3 alternating rounds.
# SKIP (exit 0) without Xvfb / xwd / xdotool / python3 or without a Vulkan device. Software Vulkan (lavapipe) is a CPU: the timings are
# NOT GPU timings. Heavy (compiles two programs): run via /root/jarvis/bin/heavy. Everything lands in a mktemp dir.
set -u
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
BENCH=0
[ "${1:-}" = "--bench" ] && BENCH=1
command -v Xvfb >/dev/null 2>&1 && command -v xwd >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1 \
    && command -v python3 >/dev/null 2>&1 || { echo "   SKIP: no Xvfb / xwd / xdotool / python3"; exit 0; }
D=$(mktemp -d "${TMPDIR:-/tmp}/fui-gpuhost-run.XXXXXX"); XP=""; PP=""
trap '[ -n "$PP" ] && kill "$PP" 2>/dev/null; [ -n "$XP" ] && kill "$XP" 2>/dev/null; rm -rf "$D"' EXIT
mkdir -p "$D/xdg"; chmod 700 "$D/xdg"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$D/xdg}"
"$FIRNC" --opt-level=dev -o "$D/smoke" tools/fui/gpuvk_main.fi 2>"$D/err" || { echo "   FAIL: compile gpuvk_main.fi"; head -5 "$D/err"; exit 1; }
"$D/smoke" >/dev/null 2>&1; rc=$?
if [ "$rc" = 4 ]; then echo "   SKIP: no Vulkan device"; exit 0; fi
"$FIRNC" --opt-level=dev -o "$D/gpuhost" tools/fui/gpuhost/main.fi 2>"$D/err" || { echo "   FAIL: compile gpuhost/main.fi"; head -5 "$D/err"; exit 1; }
mkfifo "$D/disp"; Xvfb -displayfd 3 -screen 0 1920x1200x24 -dpi 96 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 & XP=$!
read -r -t 20 n <"$D/disp" || { echo "   SKIP: Xvfb did not start"; exit 0; }
export DISPLAY=":$n"
fail=0
# run <tag> <frames> <w> <h> <resize 0|1> <hold x100ms> [VAR=value ...]  -> $D/<tag>/{out.txt,shot.xwd,shot2.xwd,canvas.rgba,canvas2.rgba}
run() {
    tag=$1; frames=$2; w=$3; h=$4; rs=$5; hold=$6; shift 6
    mkdir -p "$D/$tag"
    (cd "$D/$tag" && exec env "$@" timeout 120 "$D/gpuhost" "$frames" "$w" "$h" "$hold" >out.txt 2>&1) & PP=$!
    i=0
    while [ "$i" -lt 150 ]; do grep -q READY "$D/$tag/out.txt" 2>/dev/null && break; sleep 0.2; i=$((i + 1)); done
    grep -q READY "$D/$tag/out.txt" 2>/dev/null || { echo "   FAIL: $tag did not reach READY"; head -5 "$D/$tag/out.txt"; fail=1; }
    xwd -root -silent -out "$D/$tag/shot.xwd"
    wid=$(xdotool search --name "fUi on Vulkan" 2>/dev/null | head -1)
    if [ "$rs" = 1 ] && [ -n "$wid" ]; then xdotool windowsize "$wid" 500 300; sleep 1.2; xwd -root -silent -out "$D/$tag/shot2.xwd"; fi
    wait "$PP" 2>/dev/null; PP=""
}
NOGPU="FUI_GPU=0"
run cpu 5 640 400 1 40 "$NOGPU"
run vk 5 640 400 1 40 FUI_GPU=1 FUI_GPU_PRESENT=0
run vkp 5 640 400 1 40 FUI_GPU=1
run lost 5 640 400 0 20 FUI_GPU=1 FUI_GPU_PRESENT=0 FUI_GPU_FAIL=lost
run lostp 5 640 400 0 20 FUI_GPU=1 FUI_GPU_FAIL=lost
run open 5 640 400 0 20 FUI_GPU=1 FUI_GPU_FAIL=open
run compile 5 640 400 0 20 FUI_GPU=1 FUI_GPU_FAIL=compile
# W42-b: the rest of the chain. present = no swapchain (read-back keeps the GPU), presentstop = the swapchain stops showing at present 3
# (read-back from then on, GPU kept), glfail = the window's GL stage is faked dead (Vulkan unaffected), def/polvk/polgl = the policies
run present 5 640 400 0 20 FUI_GPU=1 FUI_GPU_FAIL=present
run pstop 5 640 400 0 20 FUI_GPU=1 FUI_GPU_FAIL=presentstop
run glfail 5 640 400 0 20 FUI_GPU=1 FUI_GPU_FAIL=glopen
run def 5 640 400 0 20 FUI_GPU=
run polvk 5 640 400 0 20 FUI_GPU=vk
run polgl 5 640 400 0 20 FUI_GPU=gl
for t in cpu vk vkp lost lostp open compile present pstop glfail def polvk polgl; do sed "s/^/   $t: /" "$D/$t/out.txt" | grep "path=\|^   $t: size=\|size2"; done
# 1. the CPU run is the reference; 2. Vulkan really paints and equals it (near)
grep -q "^path=cpu$" "$D/cpu/out.txt" || { echo "   FAIL: the cpu run did not paint on the CPU"; fail=1; }
if grep -q "^path=vulkan-readback" "$D/vk/out.txt" && grep -q "gpu=true" "$D/vk/out.txt" && grep -q "^DONE" "$D/vk/out.txt"; then
    echo "   ok:   FUI_GPU=1 paints on Vulkan (read-back + CPU present)"
else echo "   FAIL: FUI_GPU=1 did not paint on Vulkan"; head -6 "$D/vk/out.txt"; fail=1; fi
# W39 PRESENT: the swapchain run has no read-back canvas of its own; the read-back run's canvas stands in for it, so "screenshot ==
# canvas" below says: what the swapchain shows is the picture the read-back path would have shown (same Vulkan frame, no CPU copy)
cp "$D/vk/canvas.rgba" "$D/vkp/canvas.rgba" 2>/dev/null; cp "$D/vk/canvas2.rgba" "$D/vkp/canvas2.rgba" 2>/dev/null
if grep -q "^path=vulkan-present" "$D/vkp/out.txt" && grep -q "gpu=true" "$D/vkp/out.txt" && grep -q "^DONE" "$D/vkp/out.txt"; then
    echo "   ok:   FUI_GPU=1 shows the frame by swapchain present (no read-back)"
    if python3 -I tools/gpu/fuigl/cmp.py "$D/vk" "$D/vkp" 640 400 50 > "$D/cmpp.txt" 2>&1 \
        && python3 -I tools/gpu/fuigl/cmp.py "$D/vk" "$D/vkp" 500 300 50 2 > "$D/cmpp2.txt" 2>&1; then
        cat "$D/cmpp.txt" "$D/cmpp2.txt"; echo "   ok:   swapchain picture == read-back picture (PSNR >= 50), also after a resize"
    else cat "$D/cmpp.txt" "$D/cmpp2.txt"; echo "   FAIL: swapchain picture differs from the read-back picture"; fail=1; fi
elif grep -q "^path=vulkan-readback" "$D/vkp/out.txt"; then
    echo "   note: the window has no swapchain here (read-back used): present path NOT exercised"
else echo "   FAIL: FUI_GPU=1 neither presented nor read back"; head -6 "$D/vkp/out.txt"; fail=1; fi
if python3 -I tools/gpu/fuigl/cmp.py "$D/cpu" "$D/vk" 640 400 34 > "$D/cmp.txt" 2>&1; then
    cat "$D/cmp.txt"; echo "   ok:   Vulkan screenshot == its canvas, near the CPU picture (PSNR >= 34)"
else cat "$D/cmp.txt"; echo "   FAIL: Vulkan picture differs from the CPU path"; fail=1; fi
if python3 -I tools/gpu/fuigl/cmp.py "$D/cpu" "$D/vk" 500 300 34 2 > "$D/cmp2.txt" 2>&1; then
    cat "$D/cmp2.txt"; echo "   ok:   after a window resize (640x400 -> 500x300) Vulkan still equals the CPU path"
else cat "$D/cmp2.txt"; echo "   FAIL: Vulkan picture after resize differs from the CPU path"; fail=1; fi
# 3. fallbacks: bit-equal to the CPU run
if grep -q "^path=vulkan-readback" "$D/lost/out.txt" && grep -q "^size=.* gpu=false" "$D/lost/out.txt" && grep -q "^DONE" "$D/lost/out.txt" \
    && cmp -s "$D/cpu/canvas.rgba" "$D/lost/canvas.rgba"; then
    echo "   ok:   device lost at frame 3 -> that frame repainted on the CPU, GPU dropped, canvas bit-equal to the CPU run"
else echo "   FAIL: device-lost fallback"; head -6 "$D/lost/out.txt"; fail=1; fi
if grep -q "^path=vulkan-present" "$D/lostp/out.txt" && grep -q "^size=.* gpu=false" "$D/lostp/out.txt" && grep -q "^DONE" "$D/lostp/out.txt" \
    && cmp -s "$D/cpu/canvas.rgba" "$D/lostp/canvas.rgba"; then
    echo "   ok:   device lost at present 3 -> swapchain closed, frame repainted on the CPU, canvas bit-equal to the CPU run"
elif grep -q "^path=vulkan-readback" "$D/lostp/out.txt"; then echo "   note: no swapchain here: present-lost path NOT exercised"
else echo "   FAIL: device-lost fallback with swapchain"; head -6 "$D/lostp/out.txt"; fail=1; fi
# W42-b chain
if grep -q "^path=vulkan-readback" "$D/present/out.txt" && grep -q "^size=.* gpu=true readback=true present=false" "$D/present/out.txt" && grep -q "^DONE" "$D/present/out.txt" \
    && cmp -s "$D/vk/canvas.rgba" "$D/present/canvas.rgba"; then
    echo "   ok:   FUI_GPU_FAIL=present -> no swapchain, read-back keeps the GPU, canvas bit-equal to the read-back run"
elif grep -q "^path=vulkan-readback" "$D/present/out.txt" && grep -q "^size=.* gpu=true readback=true present=false" "$D/present/out.txt" && grep -q "^DONE" "$D/present/out.txt"; then
    echo "   ok:   FUI_GPU_FAIL=present -> no swapchain, read-back keeps the GPU (canvas differs from the read-back run: see below)"; fail=1
else echo "   FAIL: FUI_GPU_FAIL=present"; head -6 "$D/present/out.txt"; fail=1; fi
if grep -q "^path=vulkan-present" "$D/pstop/out.txt" && grep -q "^size=.* gpu=true readback=true present=false" "$D/pstop/out.txt" && grep -q "^DONE" "$D/pstop/out.txt" \
    && cmp -s "$D/vk/canvas.rgba" "$D/pstop/canvas.rgba"; then
    echo "   ok:   FUI_GPU_FAIL=presentstop -> swapchain closed at present 3, GPU kept (read-back), canvas bit-equal to the read-back run"
elif grep -q "^path=vulkan-readback" "$D/pstop/out.txt"; then echo "   note: no swapchain here: present-stop path NOT exercised"
else echo "   FAIL: FUI_GPU_FAIL=presentstop"; head -6 "$D/pstop/out.txt"; fail=1; fi
if grep -q "^path=vulkan-" "$D/glfail/out.txt" && grep -q "gpu=true" "$D/glfail/out.txt" && grep -q "^DONE" "$D/glfail/out.txt"; then
    echo "   ok:   FUI_GPU_FAIL=glopen (window GL faked dead) -> Vulkan still paints (FUI_GPU=1 goes Vulkan first)"
else echo "   FAIL: FUI_GPU_FAIL=glopen"; head -6 "$D/glfail/out.txt"; fail=1; fi
if grep -q "^path=cpu$" "$D/def/out.txt" && cmp -s "$D/cpu/canvas.rgba" "$D/def/canvas.rgba"; then
    echo "   ok:   FUI_GPU unset on this platform -> CPU painter (platform default), bit-equal"
else echo "   FAIL: FUI_GPU unset is not the CPU painter here"; head -4 "$D/def/out.txt"; fail=1; fi
if grep -q "software=true" "$D/vk/out.txt"; then
    for t in polvk polgl; do
        if grep -q "^path=cpu (FUI_GPU=1 asked" "$D/$t/out.txt" && grep -q "^DONE" "$D/$t/out.txt" && cmp -s "$D/cpu/canvas.rgba" "$D/$t/canvas.rgba"; then
            echo "   ok:   policy FUI_GPU=${t#pol} on a software Vulkan device -> refused, CPU painter, bit-equal"
        else echo "   FAIL: policy $t must refuse the software device"; head -4 "$D/$t/out.txt"; fail=1; fi
    done
else echo "   note: real Vulkan device here: software-refusal of FUI_GPU=vk|gl not exercised"; fi
for t in open compile; do
    if grep -q "^path=cpu (FUI_GPU=1 asked" "$D/$t/out.txt" && grep -q "^DONE" "$D/$t/out.txt" && cmp -s "$D/cpu/canvas.rgba" "$D/$t/canvas.rgba"; then
        echo "   ok:   FUI_GPU_FAIL=$t -> CPU painter, canvas bit-equal to the CPU run"
    else echo "   FAIL: FUI_GPU_FAIL=$t fallback"; head -6 "$D/$t/out.txt"; fail=1; fi
done
# 3b. FUI_GPU_VALIDATE=1 (tools/fui/gpuvalidate.sh): the Khronos layer logs into each run's out.txt; any message fails the gate
if [ "${FUI_GPU_VALIDATE:-0}" = 1 ]; then
    if grep -l "Validation Error\|Validation Warning\|VUID-" "$D"/*/out.txt >"$D/vmsg.txt" 2>/dev/null && [ -s "$D/vmsg.txt" ]; then
        echo "   FAIL: validation messages in: $(sed 's|.*/\([^/]*\)/out.txt|\1|' "$D/vmsg.txt" | tr '\n' ' ')"
        cat "$D"/*/out.txt | grep -o 'VUID-[A-Za-z0-9_-]*\|Validation \(Error\|Warning\).\{0,300\}' | sort | uniq -c | sort -rn | head -8; fail=1
    else echo "   ok:   validation layer: no messages in any window run (every window run)"; fi
fi
# 4. repaint time (forced full repaints incl. fUi painting), min of 3 alternating rounds. lavapipe is a CPU: NOT a GPU timing.
if [ "$BENCH" = 1 ]; then
    for sz in "1280 800" "1920 1080"; do
        set -- $sz; bw=$1; bh=$2
        bc=999999999; bg=999999999
        for r in 1 2 3; do
            run bcpu 60 "$bw" "$bh" 0 2 "$NOGPU"; run bvk 60 "$bw" "$bh" 0 2 FUI_GPU=1 FUI_GPU_PRESENT=0
            x=$(sed -n 's/^bench .*us_per_frame=\([0-9]*\).*/\1/p' "$D/bcpu/out.txt"); [ -n "$x" ] && [ "$x" -lt "$bc" ] && bc=$x
            x=$(sed -n 's/^bench .*us_per_frame=\([0-9]*\).*/\1/p' "$D/bvk/out.txt"); [ -n "$x" ] && [ "$x" -lt "$bg" ] && bg=$x
        done
        echo "   metric gpuhost ${bw}x${bh} us/frame, min of 3 (Vulkan = lavapipe/software if the line above says so, NOT a real GPU): cpu=$bc vulkan=$bg"
    done
fi
[ "$fail" -eq 0 ] && echo "RESULT gpuhost: ok" || echo "RESULT gpuhost: FAIL"
exit "$fail"
