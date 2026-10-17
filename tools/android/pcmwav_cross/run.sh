#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/pcmwav_cross/run.sh -- gate for lib/plat/android/pcmwav.fi: cross-check against Python's wave
# module in all four build stages + a counter-check (a sabotaged reference MUST be reported).
#   bash tools/android/pcmwav_cross/run.sh        exit 0 = no mismatch
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-pcmwav.XXXXXX")
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast" "release-fast:--opt-level=release-fast" "release-safe:--opt-level=release-safe"; do
    name=${stage%%:*}; opt=${stage#*:}
    if ! $FIRNC $opt -o "$W/tool.$name" tools/android/pcmwav_cross/pcmwavtool.fi 2> "$W/err"; then
        echo "  FAIL  $name: does not compile"; head -5 "$W/err"; ERR=$((ERR+1)); continue
    fi
    out=$(python3 tools/android/pcmwav_cross/pcmwav_cross.py "$W/tool.$name" 400 1 2>&1) || { echo "  FAIL  $name: $out"; ERR=$((ERR+1)); continue; }
    echo "  [$name] $out" | tail -1
done
# counter-check: the harness must notice a wrong reference
sed 's/rate, ch, len(pcm))/rate + 1, ch, len(pcm))/' tools/android/pcmwav_cross/pcmwav_cross.py > "$W/sab.py"
if python3 "$W/sab.py" "$W/tool.no-opt" 20 1 > /dev/null 2>&1; then echo "  FAIL  counter-check: sabotage not noticed"; ERR=$((ERR+1)); else echo "  [counter] sabotaged reference reported"; fi
echo "PCMWAV CROSS: $ERR failures"
[ $ERR -eq 0 ]
