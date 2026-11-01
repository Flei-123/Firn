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
RANDOM=${SCAN_SEED:-7}
"$FIRNC" ${WG_OPT:+--opt-level=$WG_OPT} -o "$W/surface_probe" tools/mc/wgen/surface_probe.fi 2>"$W/err" || { echo "  FAIL  compile surface_probe"; head -8 "$W/err"; exit 1; }

random_tasks() {
    local n=$1 span=$2 i
    for ((i = 0; i < n; i++)); do
        printf '%d:%d:%d ' $((RANDOM % 90000 + 1)) $((RANDOM % (2 * span + 1) - span)) $((RANDOM % (2 * span + 1) - span))
    done
}

scan_dim() {
    local dim=$1 n=$2 span=$3 tasks t sd cx cz
    tasks=$(random_tasks "$n" "$span")
    FEATURES=dir bash tools/mc/wgen/surface_oracle.sh "$W/ref_$dim.txt" "$dim" $tasks 2>&1 | grep -v WARNING
    : > "$W/got_$dim.txt"
    for t in $tasks; do
        IFS=: read -r sd cx cz <<< "$t"
        "$W/surface_probe" "$DATA" "$dim" "$sd" "$cx" "$cz" 0 1 1 >> "$W/got_$dim.txt" || { echo "  FAIL  run $dim $t"; ERR=1; }
    done
    if cmp -s "$W/ref_$dim.txt" "$W/got_$dim.txt"; then
        echo "  ok    scan $dim: $n random chunks, $(wc -l < "$W/ref_$dim.txt") columns identical"
    else
        echo "  FAIL  scan $dim: $(diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | grep -c '^<') columns differ"; ERR=1
        diff "$W/ref_$dim.txt" "$W/got_$dim.txt" | head -4 | cut -c1-240
    fi
}

scan_dim overworld ${SCAN_OVERWORLD:-30} ${SCAN_SPAN:-200}
scan_dim nether ${SCAN_NETHER:-10} ${SCAN_SPAN:-200}
[ $ERR = 0 ] && echo "mc/wgen scan: all ok" || echo "mc/wgen scan: FAILED"
exit $ERR
