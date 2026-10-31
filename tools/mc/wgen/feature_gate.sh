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
    FEATURES=ore bash tools/mc/wgen/feature_oracle.sh "$W/ref_$dim.txt" "$dim" "$@" 2>&1 | grep -v WARNING
    : > "$W/got_$dim.txt"
    for t in "$@"; do
        IFS=: read -r sd cx cz <<< "$t"
        "$W/surface_probe" "$DATA" "$dim" "$sd" "$cx" "$cz" 0 1 1 >> "$W/got_$dim.txt" || { echo "  FAIL  run $dim $t"; ERR=1; }
    done
    if cmp -s "$W/ref_$dim.txt" "$W/got_$dim.txt"; then
        echo "  ok    features $dim: $(wc -l < "$W/ref_$dim.txt") columns identical (features)"
    else
        echo "  FAIL  features $dim: $(diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | grep -c '^<') columns differ"; ERR=1
        diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | head -4 | cut -c1-240
    fi
}

run_dim overworld ${FEAT_OVERWORLD:-5:0:0 5:-3:2 11:0:0 11:2:4 42:-2:-2 3:-2:2 6:85:44}
run_dim amplified ${FEAT_AMPLIFIED:-3:2:2}
run_dim large_biomes ${FEAT_LARGE:-3:2:2}
SPECIAL_DEFAULT="3:-72:-92 3:-60:-88 11:-44:-16 11:-24:0 3:-120:24 2:-100:-88 1:-4:-120 3:4:-104 3:-120:16 5:9:-117 7:75:6 3:-120:15 6:-78:-39"
if [ -z "${FEAT_SKIP_SPECIAL:-}" ]; then
    run_dim overworld ${FEAT_SPECIAL:-$SPECIAL_DEFAULT}
fi
run_dim nether ${FEAT_NETHER:-7:0:0 7:3:-2 12:-5:4}
[ $ERR = 0 ] && echo "mc/wgen features: all ok" || echo "mc/wgen features: FAILED"
exit $ERR
