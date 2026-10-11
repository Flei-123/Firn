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
if [ $ERR -eq 0 ]; then echo "RESULT mc: ok"; else echo "RESULT mc: $ERR failing parts"; fi
exit $ERR
