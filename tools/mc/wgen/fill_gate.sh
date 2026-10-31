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
"$FIRNC" -o "$W/fill_probe" tools/mc/wgen/fill_probe.fi 2>"$W/err" || { echo "  FAIL  compile fill_probe"; head -8 "$W/err"; exit 1; }

run_dim() {
    local dim=$1; shift
    bash tools/mc/wgen/fill_oracle.sh "$W/ref_$dim.txt" "$dim" "$@" 2>&1 | grep -v WARNING
    : > "$W/got_$dim.txt"
    for t in "$@"; do
        IFS=: read -r sd cx cz <<< "$t"
        "$W/fill_probe" "$DATA" "$dim" "$sd" "$cx" "$cz" >> "$W/got_$dim.txt" || { echo "  FAIL  run $dim $t"; ERR=1; }
    done
    if cmp -s "$W/ref_$dim.txt" "$W/got_$dim.txt"; then
        echo "  ok    fill $dim: $(wc -l < "$W/ref_$dim.txt") rows identical"
    else
        echo "  FAIL  fill $dim: $(diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | grep -c '^<') rows differ"; ERR=1
    fi
}

run_dim nether 7:0:0 7:-3:5 99:12:-9
run_dim overworld 11:0:0 11:-3:5 42:20:-17 5:-40:31 1234:7:-60
run_dim amplified 3:2:2 77:-9:14
run_dim large_biomes 3:2:2
run_dim caves 21:1:1
run_dim floating_islands 21:1:1
run_dim end 5:0:0 5:30:-44 9:-20:12
[ $ERR = 0 ] && echo "mc/wgen fill: all ok" || echo "mc/wgen fill: FAILED"
exit $ERR
