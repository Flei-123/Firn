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
"$FIRNC" ${WG_OPT:+--opt-level=$WG_OPT} -o "$W/surface_probe" tools/mc/wgen/surface_probe.fi 2>"$W/err" || { echo "  FAIL  compile surface_probe"; head -8 "$W/err"; exit 1; }

run_dim() {
    local dim=$1; shift
    FEATURES=dir bash tools/mc/wgen/feature_oracle.sh "$W/ref_$dim.txt" "$dim" "$@" 2>&1 | grep -v WARNING
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
SPECIAL_DEFAULT="1:-100:-100 3:-4:4 3:-2:0 3:-72:-92 3:-60:-88 11:-44:-16 11:-24:0 3:-120:24 2:-100:-88 1:-4:-120 3:4:-104 3:-120:16 5:9:-117 7:75:6 3:-120:15 6:-78:-39"
if [ -z "${FEAT_SKIP_SPECIAL:-}" ]; then
    run_dim overworld ${FEAT_SPECIAL:-$SPECIAL_DEFAULT}
fi
TREE_DEFAULT="8:-48:33 8:-57:-15 8:-42:-60 42:48:-60 6:-36:30 8:-55:30 11:-50:20 5:-45:-45 42:-35:50 11:-50:-25 8:-20:-4 5:24:-32 5:20:32 42:-32:-40 11:-50:-50 42:-40:-40 11:-35:50 24:-52:-36 4:-92:-52 3:84:20 10:-44:-100 14:-20:-60 16:-58:-68 3:-80:20 6:12:-8 8:36:68 1:9:117 11:-72:6 42:57:111 11:87:90 3:-57:3 5:-18:-45 1:114:-6 3:-33:42 1:66:93 1:63:96 3:-36:39 3:3:6 1:-182:-32 1:-164:-32 6:-110:-38 6:-111:-38 6:172:-98 6:46:-140 8:112:70 8:118:46"
FLORA_DEFAULT="6:112:172 5:-75:-72 5:-69:6 5:-69:-45 5:-69:-72 5:-75:-63 5:-75:-24 5:-72:-75 5:-75:-27 5:-75:-9 5:-75:42 5:-75:-66 5:-69:75 5:-75:-54 5:-69:-75 5:-63:-24 5:-39:-33 5:-24:27 5:-63:-12 7:-63:18 7:-75:3 7:-75:9 7:-72:24 1:-188:22 1:-182:28 5:-66:-63 7:-51:15 7:-36:45"
if [ -z "${FEAT_SKIP_FLORA:-}" ]; then
    run_dim overworld ${FEAT_FLORA:-$FLORA_DEFAULT}
fi
if [ -z "${FEAT_SKIP_TREES:-}" ]; then
    run_dim overworld ${FEAT_TREES:-$TREE_DEFAULT}
fi
LUSH_DEFAULT="3:-48:24 3:-28:46 3:-22:42 3:-10:-16"
if [ -z "${FEAT_SKIP_LUSH:-}" ]; then
    run_dim overworld ${FEAT_LUSH:-$LUSH_DEFAULT}
fi
run_dim nether ${FEAT_NETHER:-7:0:0 7:3:-2 12:-5:4}
[ $ERR = 0 ] && echo "mc/wgen features: all ok" || echo "mc/wgen features: FAILED"
exit $ERR
