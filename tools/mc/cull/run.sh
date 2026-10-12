#!/usr/bin/env bash
# tools/mc/cull/run.sh -- cross-check of lib/mc/mc_cull.fi: tests/2770_mc_cull.fi in three build stages. The test renders 36 camera poses
# of a terrain world with caves twice (all frustum sections vs only the sections cave culling keeps) and demands byte identical images
# (the "reference" is the unculled image of mc_raster); a graph with every section closed must differ (counter-check inside the test).
set -uo pipefail
cd "$(dirname "$0")/../../.."
if bash tools/mc/testmc.sh cull; then echo "RESULT mc-cull: ok"; else echo "RESULT mc-cull: FAIL"; exit 1; fi
