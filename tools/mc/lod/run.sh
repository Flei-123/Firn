#!/usr/bin/env bash
# tools/mc/lod/run.sh -- cross-check of lib/mc/mc_lod.fi: tests/2774_mc_lod.fi in three build stages (layout, skirts, bin maximum, heights ==
# direct scan of the terrain, far fewer triangles than the full meshes, image of the LOD columns == image of the full meshes on a flat world).
# `tools/mc/lod/run.sh --bench` also measures triangles / bytes for view distance 8 / 16 / 32 with and without LOD (RESULT lines).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
if [ "${1:-}" = "--bench" ]; then
    W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
    $FIRNC -o "$W/bench" tools/mc/stream/bench.fi || exit 1
    "$W/bench" 5 8 lod
    exit 0
fi
if bash tools/mc/testmc.sh lod; then echo "RESULT mc-lod: ok"; else echo "RESULT mc-lod: FAIL"; exit 1; fi
