#!/usr/bin/env bash
# tools/mc/play/run_vkview.sh -- mcview with the Vulkan back end (MCVIEW_GFX=vk, r196) against the CPU raster on the frozen world dump
# tools/mc/view/fixture/world.mcw (offline replay, NO server, no X display). lavapipe = NO real GPU: proves wiring + picture parity, not speed.
# Variants (flags/env): all (sky+box+hud), sunset, night+rain, fog off, stream (MCVIEW_STREAM=1, workers 0). Each: CPU image vs VK image, PSNR >= MCVIEW_VK_PSNR_MIN (34),
# `gfx=vk` line present, exit 0 (mcview itself checks that the cached 2nd frame == the 1st), VK run twice -> same hash. SKIP (exit 0) without a Vulkan device.
# Env: FIRNC, MCVIEW_VK_PSNR_MIN. Start via heavy. Prints `RESULT mc-vkview: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libvulkan.so.1 ] || [ -e /usr/lib64/libvulkan.so.1 ] || { echo "  SKIP: no libvulkan.so.1"; echo "RESULT mc-vkview: skip"; exit 0; }
command -v python3 >/dev/null || { echo "  FAIL  missing python3"; echo "RESULT mc-vkview: FAIL"; exit 1; }
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
PMIN=${MCVIEW_VK_PSNR_MIN:-34}; ERR=0
$FIRNC --opt-level=dev-fast -o "$W/mcview" demos/mcview/main.fi 2>"$W/err" || { head -10 "$W/err"; echo "  FAIL  demos/mcview does not compile"; echo "RESULT mc-vkview: FAIL"; exit 1; }
WORLD=tools/mc/view/fixture/world.mcw
psnr() { python3 -I - "$1" "$2" <<'PY'
import sys,math
a=open(sys.argv[1],'rb').read();b=open(sys.argv[2],'rb').read()
if len(a)!=len(b) or not a: print("size-mismatch"); sys.exit(1)
se=0
for i in range(0,len(a),4):
    for c in range(3):
        d=a[i+c]-b[i+c]; se+=d*d
m=se/(len(a)//4*3)
print("inf" if m==0 else "%.2f"%(10*math.log10(255*255/m)))
PY
}
# variant <label> <env assignments> <flags>
variant() {
    local lab=$1 envs=$2 flags=$3 mode
    for mode in cpu vk vk2; do
        mkdir -p "$W/$lab.$mode"
        local g=$mode; [ "$mode" = vk2 ] && g=vk
        # shellcheck disable=SC2086
        env MCVIEW_FPS=60 MCVIEW_WORKERS=0 $envs MCVIEW_GFX=$g "$W/mcview" $flags --replay "$W/$lab.$mode" $WORLD > "$W/$lab.$mode.out" 2>&1 \
            || { echo "  FAIL  $lab ($mode): exit code"; head -5 "$W/$lab.$mode.out"; ERR=1; return; }
    done
    grep -q "gfx=vk" "$W/$lab.vk.out" || { echo "  SKIP  $lab: no Vulkan device (fell back to cpu)"; return; }
    local h1 h2 p
    h1=$(grep -o "metric hash=.*" "$W/$lab.vk.out"); h2=$(grep -o "metric hash=.*" "$W/$lab.vk2.out")
    [ "$h1" = "$h2" ] || { echo "  FAIL  $lab: two VK runs differ ($h1 vs $h2)"; ERR=1; }
    p=$(psnr "$W/$lab.cpu/view.rgba" "$W/$lab.vk/view.rgba") || { echo "  FAIL  $lab: $p"; ERR=1; return; }
    if [ "$p" = inf ] || python3 -I -c "import sys;sys.exit(0 if float('$p')>=float('$PMIN') else 1)"; then
        echo "  ok    $lab: PSNR VK vs CPU $p dB (>= $PMIN)"
    else
        echo "  FAIL  $lab: PSNR VK vs CPU $p dB < $PMIN"; ERR=1
    fi
    [ -s "$W/$lab.vk/view.png" ] || { echo "  FAIL  $lab: no PNG"; ERR=1; }
}
variant all   ""                                  "--all"
variant sunset "MCVIEW_TICKS=12500"               "--all"
variant night "MCVIEW_TICKS=15000 MCVIEW_RAIN=200" "--all"
variant nofog "MCVIEW_FOG=0"                      "--all"
variant plain ""                                  ""
variant stream "MCVIEW_STREAM=1"                  "--all"
if [ $ERR -eq 0 ]; then echo "RESULT mc-vkview: ok"; else echo "RESULT mc-vkview: FAIL"; exit 1; fi
