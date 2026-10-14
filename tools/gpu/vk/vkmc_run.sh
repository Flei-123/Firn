#!/usr/bin/env bash
# tools/gpu/vk/vkmc_run.sh -- Vulkan voxel back end (lib/mc/vk_mc.fi) vs the CPU rasterizer on the GL scenes (vkmc_dump.fi). lavapipe, NO real GPU.
# SKIP (exit 0) without libvulkan / ICD. Usage: bash tools/gpu/vk/vkmc_run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc   # worktrees have no built compiler (r194)
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libvulkan.so.1 ] || [ -e /usr/lib64/libvulkan.so.1 ] || { echo "   SKIP: no libvulkan.so.1"; exit 0; }
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
"$FIRNC" -o "$D/dump" tools/gpu/vk/vkmc_dump.fi 2>"$D/err" || { echo "   FAIL: compile vkmc_dump.fi"; head -5 "$D/err"; exit 1; }
timeout 300 "$D/dump" >"$D/out.txt" 2>&1; rc=$?
sed 's/^/   /' "$D/out.txt"
if [ "$rc" -eq 4 ]; then echo "   SKIP: no Vulkan device"; exit 0; fi
if [ "$rc" -eq 0 ]; then echo "   ok:   Vulkan back end vs mc_raster: $(grep -c '^scene .* ok$' "$D/out.txt") scenes within the thresholds"; exit 0; fi
echo "   FAIL: vkmc_dump exit $rc"; exit 1
