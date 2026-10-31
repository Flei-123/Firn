#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
DATA=tools/mcgen/worldgen/1.20.1
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
"$FIRNC" -o "$W/surface_probe" tools/mc/wgen/surface_probe.fi 2>"$W/err" || { echo "  FAIL  compile surface_probe"; head -8 "$W/err"; exit 1; }

run_dim() {
    local dim=$1; shift
    bash tools/mc/wgen/surface_oracle.sh "$W/ref_$dim.txt" "$dim" "$@" 2>&1 | grep -v WARNING
    : > "$W/got_$dim.txt"
    for t in "$@"; do
        IFS=: read -r sd cx cz <<< "$t"
        "$W/surface_probe" "$DATA" "$dim" "$sd" "$cx" "$cz" 0 >> "$W/got_$dim.txt" || { echo "  FAIL  run $dim $t"; ERR=1; }
    done
    if cmp -s "$W/ref_$dim.txt" "$W/got_$dim.txt"; then
        echo "  ok    surface $dim: $(wc -l < "$W/ref_$dim.txt") columns identical"
    else
        echo "  FAIL  surface $dim: $(diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | grep -c '^<') columns differ"; ERR=1
        diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | head -4 | cut -c1-240
    fi
}

run_dim overworld ${SURFACE_OVERWORLD:-11:0:0 11:-3:5 42:20:-17}
run_dim amplified ${SURFACE_AMPLIFIED:-3:2:2}
run_dim large_biomes ${SURFACE_LARGE:-3:2:2}
run_dim nether ${SURFACE_NETHER:-7:0:0}
run_dim end ${SURFACE_END:-7:0:0}
[ -n "${SURFACE_EXTRA:-}" ] && run_dim overworld $SURFACE_EXTRA
[ $ERR = 0 ] && echo "mc/wgen surface: all ok" || echo "mc/wgen surface: FAILED"
exit $ERR
