#!/usr/bin/env bash
# tools/gpu/vk/vkmodel_run.sh -- Vulkan model quads (MESH_MODEL / MESH_MODEL_T, vk_glsl/mc_model_vs/fs.glsl) vs the CPU rasterizer (vkmodel_dump.fi,
# 7 scenes, PSNR >= 34 dB). lavapipe, NO real GPU. SKIP (exit 0) without libvulkan / ICD. Usage: bash tools/gpu/vk/vkmodel_run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc   # worktrees have no built compiler (r194)
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libvulkan.so.1 ] || [ -e /usr/lib64/libvulkan.so.1 ] || { echo "   SKIP: no libvulkan.so.1"; exit 0; }
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
"$FIRNC" -o "$D/dump" tools/gpu/vk/vkmodel_dump.fi 2>"$D/err" || { echo "   FAIL: compile vkmodel_dump.fi"; head -8 "$D/err"; exit 1; }
LP_NUM_THREADS=${LP_NUM_THREADS:-4} timeout 300 "$D/dump" >"$D/out.txt" 2>&1; rc=$?
sed 's/^/   /' "$D/out.txt"
if [ "$rc" -eq 4 ]; then echo "   SKIP: no Vulkan device"; exit 0; fi
if [ "$rc" -eq 0 ]; then echo "   ok:   Vulkan model quads vs CPU: $(grep -c '^scene .* ok$' "$D/out.txt") scenes within the thresholds"; exit 0; fi
echo "   FAIL: vkmodel_dump exit $rc"; exit 1
