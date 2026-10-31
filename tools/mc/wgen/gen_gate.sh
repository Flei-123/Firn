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
"$FIRNC" -o "$W/gen_probe" tools/mc/wgen/gen_probe.fi 2>"$W/err" || { echo "  FAIL  compile gen_probe"; head -8 "$W/err"; exit 1; }

run_dim() {
    local dim=$1; shift
    bash tools/mc/wgen/fill_oracle.sh "$W/ref_$dim.txt" "$dim" "$@" 2>&1 | grep -v WARNING
    : > "$W/got_$dim.txt"
    for t in "$@"; do
        IFS=: read -r sd cx cz <<< "$t"
        "$W/gen_probe" "$DATA" "$dim" "$sd" "$cx" "$cz" 0 >> "$W/got_$dim.txt" || { echo "  FAIL  run $dim $t"; ERR=1; }
    done
    if cmp -s "$W/ref_$dim.txt" "$W/got_$dim.txt"; then
        echo "  ok    gen $dim: $(wc -l < "$W/ref_$dim.txt") rows identical"
    else
        echo "  FAIL  gen $dim: $(diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | grep -c '^<') rows differ"; ERR=1
    fi
}

run_dim overworld 11:0:0 11:-3:5 42:20:-17 5:-40:31 1234:7:-60
run_dim amplified 3:2:2 77:-9:14
run_dim large_biomes 3:2:2
[ $ERR = 0 ] && echo "mc/wgen gen: all ok" || echo "mc/wgen gen: FAILED"
exit $ERR
