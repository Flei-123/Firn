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
"$FIRNC" -o "$W/bio_probe" tools/mc/wgen/bio_probe.fi 2>"$W/err" || { echo "  FAIL  compile bio_probe"; head -8 "$W/err"; exit 1; }

run_dim() {
    local dim=$1; shift
    bash tools/mc/wgen/biome_oracle.sh "$W/ref_$dim.txt" "$dim" "$@" 2>&1 | grep -v WARNING
    : > "$W/got_$dim.txt"
    for t in "$@"; do
        IFS=: read -r sd cx cz <<< "$t"
        "$W/bio_probe" "$DATA" "$dim" "$sd" "$cx" "$cz" >> "$W/got_$dim.txt" || { echo "  FAIL  run $dim $t"; ERR=1; }
    done
    if cmp -s "$W/ref_$dim.txt" "$W/got_$dim.txt"; then
        echo "  ok    biome $dim: $(wc -l < "$W/ref_$dim.txt") rows identical, $(awk '{for(i=6;i<=NF;i++)print $i}' "$W/ref_$dim.txt" | sort -u | wc -l) distinct biomes"
    else
        echo "  FAIL  biome $dim: $(diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | grep -c '^<') rows differ"; ERR=1
    fi
}

run_dim overworld 11:0:0 11:-3:5 42:20:-17 5:-40:31 1234:7:-60 99:300:-250 99:-800:640 7:1500:90 -5:-2000:-1700
run_dim amplified 3:2:2 77:-9:14 77:600:-300
run_dim large_biomes 3:2:2 5:400:400
run_dim nether 11:0:0 42:-30:70 42:200:-90
[ $ERR = 0 ] && echo "mc/wgen biome: all ok" || echo "mc/wgen biome: FAILED"
exit $ERR
