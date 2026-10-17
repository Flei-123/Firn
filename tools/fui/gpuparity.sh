#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/gpuparity.sh -- fUi ON VULKAN vs THE CPU PAINTER (W18-F2).
#
#     tools/fui/gpuparity.sh [--out DIR]       DIR keeps cpu-/gpu-/diff-<case>.png
#
#   1. gpuvk_main      clear + rect through lib/fui/gpu.fi on the Vulkan table (lib/fui/gpu_vk.fi)
#   2. gpuparity_main  the zoo page (every way fUi draws) on the CPU and on Vulkan, light + dark, with and without text:
#                      mean abs diff <= 1.0; without text max <= 12; with text <= 5 per mille of the pixels over 32
#   3. FUI_GPU_FAIL=compile and FUI_GPU_FAIL=open: the GPU does not open, the second CPU run equals the first bit for bit
# No Vulkan device on the machine (exit 4): prints SKIP and exits 0. Software Vulkan (lavapipe) is a CPU: the numbers
# prove the arithmetic, not the speed. Heavy: run via /root/jarvis/bin/heavy. Everything lands in a mktemp dir.
set -u
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
OUT=""
if [ "${1:-}" = "--out" ]; then OUT="${2:-}"; mkdir -p "$OUT"; fi
T="$(mktemp -d "${TMPDIR:-/tmp}/fui-gpuparity-run.XXXXXX")"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/xdg"; chmod 700 "$T/xdg"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$T/xdg}"
"$FIRNC" --opt-level=dev -o "$T/smoke" tools/fui/gpuvk_main.fi || exit 1
"$FIRNC" --opt-level=dev -o "$T/par" tools/fui/gpuparity_main.fi || exit 1
echo "== 1. smoke: clear + rect on the Vulkan table =="
"$T/smoke"; rc=$?
if [ "$rc" = 4 ]; then echo "SKIP: no Vulkan device"; exit 0; fi
[ "$rc" = 0 ] || { echo "gpuparity: smoke failed ($rc)"; exit 1; }
echo "== 2. CPU vs Vulkan =="
if [ -n "$OUT" ]; then "$T/par" "$OUT" || exit 1; else "$T/par" || exit 1; fi
echo "== 3. fallback: FUI_GPU_FAIL=compile =="
FUI_GPU_FAIL=compile "$T/par" || exit 1
echo "== 4. fallback: FUI_GPU_FAIL=open =="
FUI_GPU_FAIL=open "$T/par" || exit 1
echo "gpuparity: ALL OK"
