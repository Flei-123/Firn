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
        "$W/surface_probe" "$DATA" "$dim" "$sd" "$cx" "$cz" 0 0 >> "$W/got_$dim.txt" || { echo "  FAIL  run $dim $t"; ERR=1; }
    done
    if cmp -s "$W/ref_$dim.txt" "$W/got_$dim.txt"; then
        echo "  ok    surface $dim: $(wc -l < "$W/ref_$dim.txt") columns identical"
    else
        echo "  FAIL  surface $dim: $(diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | grep -c '^<') columns differ"; ERR=1
        diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | head -4 | cut -c1-240
    fi
}

EXTRA_DEFAULT="11:-65:-135 7:-150:-110 3:-70:-95 12:50:70 22:-55:-45 2:-10:-100 4:-40:45 2:10:-105 4:-5:25 1:-120:-30 1:-5:-120 1:20:-85 2:-30:115 3:5:-60 3:-10:-80 2:5:110 3:-120:20 1:-60:120 3:-60:-80 6:85:44 6:85:45"
if [ -z "${SURFACE_ONLY_EXTRA:-}" ]; then
    run_dim overworld ${SURFACE_OVERWORLD:-11:0:0 11:-3:5 42:20:-17}
    run_dim amplified ${SURFACE_AMPLIFIED:-3:2:2}
    run_dim large_biomes ${SURFACE_LARGE:-3:2:2}
    run_dim nether ${SURFACE_NETHER:-7:0:0}
    run_dim end ${SURFACE_END:-7:0:0}
fi
if [ -z "${SURFACE_SKIP_EXTRA:-}" ]; then
    run_dim overworld ${SURFACE_EXTRA:-$EXTRA_DEFAULT}
fi
[ $ERR = 0 ] && echo "mc/wgen surface: all ok" || echo "mc/wgen surface: FAILED"
exit $ERR
