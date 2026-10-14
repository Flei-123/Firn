#!/usr/bin/env bash
# tools/gpu/vk/vkscene_run.sh -- Vulkan scene layer (lib/mc/vk_mc.fi: sky, tint, entities, HUD; roadmap r180) vs mc_raster + mc_sky + mc_hud
# (vkscene_dump.fi). lavapipe, NO real GPU. SKIP (exit 0) without libvulkan / ICD. Usage: bash tools/gpu/vk/vkscene_run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libvulkan.so.1 ] || [ -e /usr/lib64/libvulkan.so.1 ] || { echo "   SKIP: no libvulkan.so.1"; exit 0; }
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
"$FIRNC" -o "$D/dump" tools/gpu/vk/vkscene_dump.fi 2>"$D/err" || { echo "   FAIL: compile vkscene_dump.fi"; head -8 "$D/err"; exit 1; }
LP_NUM_THREADS=${LP_NUM_THREADS:-4} timeout 300 "$D/dump" >"$D/out.txt" 2>&1; rc=$?
sed 's/^/   /' "$D/out.txt"
if [ "$rc" -eq 4 ]; then echo "   SKIP: no Vulkan device"; exit 0; fi
if [ "$rc" -eq 0 ]; then echo "   ok:   Vulkan scene layer vs CPU path: $(grep -c '^scene .* ok$' "$D/out.txt") scenes within the thresholds"; exit 0; fi
echo "   FAIL: vkscene_dump exit $rc"; exit 1
