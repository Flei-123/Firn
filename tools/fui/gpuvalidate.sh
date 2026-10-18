#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/fui/gpuvalidate.sh -- the fUi Vulkan path under the Khronos VALIDATION LAYER (opt-in FUI_GPU_VALIDATE=1, W42-b).
#
#     bash tools/fui/gpuvalidate.sh
#
#   Runs gpuvk_main (stress), gpuparity_main (zoo page, CPU vs Vulkan) and gpuhost.sh (window: read-back, swapchain present, lost,
#   resize) with VK_LAYER_KHRONOS_validation on the instance; every "Validation Error" / "Validation Warning" line fails the run.
#   The layer comes from, in this order: $FUI_VK_LAYER_DIR (a dir with VkLayer_khronos_validation.json + the .so, or a dpkg -x root),
#   the system layer dirs, or `apt-get download vulkan-validationlayers` unpacked into a mktemp dir (NOTHING is installed system wide).
#   SKIP (exit 0) when there is no Vulkan device or no layer can be had. Software Vulkan (lavapipe) is enough: the layer checks API use,
#   not the driver. Heavy (compiles three programs): run via /root/jarvis/bin/heavy. Everything lands in a mktemp dir.
set -u
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
T="$(mktemp -d "${TMPDIR:-/tmp}/fui-gpuvalidate-run.XXXXXX")"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/xdg"; chmod 700 "$T/xdg"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$T/xdg}"
LJ=VkLayer_khronos_validation.json
lp=""; ld=""
if [ -n "${FUI_VK_LAYER_DIR:-}" ]; then
    for d in "$FUI_VK_LAYER_DIR" "$FUI_VK_LAYER_DIR/usr/share/vulkan/explicit_layer.d"; do [ -f "$d/$LJ" ] && lp="$d"; done
    for d in "$FUI_VK_LAYER_DIR" "$FUI_VK_LAYER_DIR/usr/lib/x86_64-linux-gnu"; do [ -f "$d/libVkLayer_khronos_validation.so" ] && ld="$d"; done
elif [ -f /usr/share/vulkan/explicit_layer.d/$LJ ] || [ -f /etc/vulkan/explicit_layer.d/$LJ ]; then
    lp="system"
else
    ( mkdir -p "$T/deb" && cd "$T/deb" && apt-get download vulkan-validationlayers >/dev/null 2>&1 && dpkg -x ./*.deb "$T/root" ) 2>/dev/null
    [ -f "$T/root/usr/share/vulkan/explicit_layer.d/$LJ" ] && { lp="$T/root/usr/share/vulkan/explicit_layer.d"; ld="$T/root/usr/lib/x86_64-linux-gnu"; }
fi
[ -n "$lp" ] || { echo "   SKIP: no Vulkan validation layer (set FUI_VK_LAYER_DIR or install vulkan-validationlayers)"; exit 0; }
if [ "$lp" != "system" ]; then
    export VK_LAYER_PATH="$lp"
    [ -n "$ld" ] && export LD_LIBRARY_PATH="$ld${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
export FUI_GPU_VALIDATE=1
# synchronisation validation too (hazards between the one-command-buffer-per-frame copies / blits / draws); FUI_VALIDATE_SYNC=0 turns it off
[ "${FUI_VALIDATE_SYNC:-1}" = 1 ] && export VK_LAYER_ENABLES=VK_VALIDATION_FEATURE_ENABLE_SYNCHRONIZATION_VALIDATION_EXT
fail=0
chk() { # <tag> <file>: any validation message in the log fails the run
    if grep -q "Validation Error\|Validation Warning\|VUID-" "$2"; then
        echo "   FAIL: $1: validation messages:"; grep -o 'VUID-[A-Za-z0-9_-]*\|Validation \(Error\|Warning\).\{0,300\}' "$2" | sort | uniq -c | sort -rn | head -8; fail=1
    else echo "   ok:   $1: no validation messages"; fi
}
"$FIRNC" --opt-level=dev -o "$T/smoke" tools/fui/gpuvk_main.fi 2>"$T/err" || { echo "   FAIL: compile gpuvk_main.fi"; head -5 "$T/err"; exit 1; }
"$FIRNC" --opt-level=dev -o "$T/par" tools/fui/gpuparity_main.fi 2>"$T/err" || { echo "   FAIL: compile gpuparity_main.fi"; head -5 "$T/err"; exit 1; }
"$T/smoke" >"$T/smoke.txt" 2>&1; rc=$?
if [ "$rc" = 4 ]; then echo "   SKIP: no Vulkan device"; exit 0; fi
[ "$rc" = 0 ] || { echo "   FAIL: gpuvk_main exit $rc"; head -5 "$T/smoke.txt"; fail=1; }
if grep -q "validation=true" "$T/smoke.txt"; then echo "   ok:   the validation layer is on the instance"
else echo "   SKIP: the layer did not load (validation=false); is VK_LAYER_PATH right?"; head -3 "$T/smoke.txt"; exit 0; fi
chk "gpuvk_main (clear, rect, 4096 draws, ring, staging)" "$T/smoke.txt"
"$T/par" >"$T/par.txt" 2>&1 || { echo "   FAIL: gpuparity_main"; tail -3 "$T/par.txt"; fail=1; }
chk "gpuparity_main (zoo page on Vulkan, light + dark, glass)" "$T/par.txt"
FUI_GPU_FAIL=lost "$T/par" >"$T/parl.txt" 2>&1
chk "gpuparity_main with FUI_GPU_FAIL=lost" "$T/parl.txt"
# window runs (Xvfb): gpuhost.sh checks its own out.txt files for validation messages when FUI_GPU_VALIDATE=1
bash tools/fui/gpuhost.sh >"$T/host.txt" 2>&1 || fail=1
grep "ok:\|FAIL\|SKIP\|note:\|RESULT\|validation" "$T/host.txt" | head -30
[ "$fail" -eq 0 ] && echo "RESULT gpuvalidate: ok" || echo "RESULT gpuvalidate: FAIL"
exit "$fail"
