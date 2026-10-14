#!/usr/bin/env bash
# tools/mc/run.sh -- every Minecraft library (lib/mc/mc_*.fi) with its tests.
#
#  1. tests/*_mc_*.fi in three build stages (tools/mc/testmc.sh)
#  2. tools/mc/<lib>/run.sh: the cross-check of each library against an
#     independent reference (python / node / java tools, never Mojang code).
#     Each prints a line `RESULT <name>: ok` and exits 0 on success.
#
# Usage: bash tools/mc/run.sh [lib]      (lib = a directory name under tools/mc/)
set -uo pipefail
cd "$(dirname "$0")/../.."
ERR=0
ONLY=${1:-}
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
# every .fi file of the tree must be in the canonical shape of firnfmt (test.sh section 23
# fails otherwise): format with `build/firnfmt -w <file>`; this builds it into a temp dir.
if [ -z "$ONLY" ]; then
    FT=$(mktemp -d)
    trap 'rm -rf "$FT"' EXIT
    if FIRNLIB="$(pwd)/lib" "$FIRNC" -o "$FT/firnfmt" tools/fmt/firnfmt.fi 2>"$FT/err"; then
        files=$(ls lib/mc/*.fi lib/mc/v*/*.fi tests/*_mc_*.fi demos/mcclient/*.fi demos/mcview/*.fi demos/mcplay/*.fi 2>/dev/null; find tools/mc -name '*.fi')
        if bad=$("$FT/firnfmt" -c $files 2>&1) ; then echo "  ok    all mc sources are in canonical shape (firnfmt -c)"
        else echo "  FAIL  not formatted (run firnfmt -w):"; echo "$bad" | head -10 | sed 's/^/        /'; ERR=$((ERR+1)); fi
    else echo "  FAIL  cannot build tools/fmt/firnfmt.fi"; head -5 "$FT/err"; ERR=$((ERR+1)); fi
fi
if [ -z "$ONLY" ]; then
    bash tools/mc/testmc.sh || ERR=$((ERR+1))
fi
for s in tools/mc/*/run.sh; do
    [ -f "$s" ] || continue
    lib=$(basename "$(dirname "$s")")
    [ -n "$ONLY" ] && [ "$ONLY" != "$lib" ] && continue
    echo "== mc/$lib =="
    bash "$s" || ERR=$((ERR+1))
done
# Windows build under Wine (cross-compile, login, window, raw mouse, F11, GUI exe + bank + log, recv-timeout seam): ~4 min, needs wine + Xvfb + java.
if [ -z "$ONLY" ] || [ "$ONLY" = wine ]; then
    echo "== mc/wine =="
    bash tools/mc/play/run_wine.sh || ERR=$((ERR+1))
fi
# mcplay with LOD rings (MCPLAY_LOD, mc_stream) against the real server: ~2 min, needs Xvfb + xdotool + java.
if [ -z "$ONLY" ] || [ "$ONLY" = lodplay ]; then
    echo "== mc/lodplay =="
    bash tools/mc/play/run_lod.sh || ERR=$((ERR+1))
fi
# entities in the mcplay play path against the real server (live packets + CPU/GL pictures with / without boxes): ~4 min, needs Xvfb + java.
if [ -z "$ONLY" ] || [ "$ONLY" = ents ]; then
    echo "== mc/ents =="
    bash tools/mc/play/run_ents.sh || ERR=$((ERR+1))
fi
if [ $ERR -eq 0 ]; then echo "RESULT mc: ok"; else echo "RESULT mc: $ERR failing parts"; fi
exit $ERR
