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
    CARVE=1 bash tools/mc/wgen/surface_oracle.sh "$W/ref_$dim.txt" "$dim" "$@" 2>&1 | grep -v WARNING
    : > "$W/got_$dim.txt"
    for t in "$@"; do
        IFS=: read -r sd cx cz <<< "$t"
        "$W/surface_probe" "$DATA" "$dim" "$sd" "$cx" "$cz" 0 1 >> "$W/got_$dim.txt" || { echo "  FAIL  run $dim $t"; ERR=1; }
    done
    if cmp -s "$W/ref_$dim.txt" "$W/got_$dim.txt"; then
        echo "  ok    carve $dim: $(wc -l < "$W/ref_$dim.txt") columns identical (carvers)"
    else
        echo "  FAIL  carve $dim: $(diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | grep -c '^<') columns differ"; ERR=1
        diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | head -4 | cut -c1-240
    fi
}

run_dim overworld ${CARVE_OVERWORLD:-11:0:0 11:2:4 11:-1:3 42:-2:-2 42:-3:1 3:-2:2 6:85:44 7:-20:15 5:40:-33}
run_dim amplified ${CARVE_AMPLIFIED:-3:2:2}
run_dim large_biomes ${CARVE_LARGE:-3:2:2}
EXTRA_DEFAULT="1:-100:-100 11:-65:-135 7:-150:-110 3:-70:-95 12:50:70 22:-55:-45 2:-10:-100 4:-40:45 2:10:-105 4:-5:25 1:-120:-30 1:-5:-120 1:20:-85 2:-30:115 3:5:-60 3:-10:-80 2:5:110 3:-120:20 1:-60:120 3:-60:-80 6:85:45"
if [ -z "${CARVE_SKIP_EXTRA:-}" ]; then
    run_dim overworld ${CARVE_EXTRA:-$EXTRA_DEFAULT}
fi
run_dim nether ${CARVE_NETHER:-7:0:0 7:3:-2 12:-5:4}
[ $ERR = 0 ] && echo "mc/wgen carve: all ok" || echo "mc/wgen carve: FAILED"
exit $ERR
