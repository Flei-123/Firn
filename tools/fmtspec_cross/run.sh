#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/fmtspec_cross/run.sh -- f-string format specs against Python's format().
# Random specs (fill/align/sign/#/0/width/.precision/type) over ints, floats, str, bool;
# the Firn program prints one line per case, Python computes the same lines.
# Usage: bash tools/fmtspec_cross/run.sh [SEEDS=3] [CASES=800]     (FIRNC / FIRNLIB honoured)
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$PWD/lib}
SEEDS=${1:-3}
CASES=${2:-800}
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
bad=0
total=0
for seed in $(seq 1 "$SEEDS"); do
    python3 tools/fmtspec_cross/gen.py "$seed" "$CASES" "$D/c$seed.fi" "$D/c$seed.exp" || exit 1
    for lvl in release-fast no-opt dev-fast release-safe; do
        flag="--opt-level=$lvl"
        [ "$lvl" = no-opt ] && flag="--no-opt"
        if ! "$FIRNC" "$flag" -o "$D/c$seed.bin" "$D/c$seed.fi" > "$D/cc.log" 2>&1; then
            echo "COMPILE FAIL seed=$seed level=$lvl"; head -5 "$D/cc.log"; bad=$((bad + 1)); continue
        fi
        "$D/c$seed.bin" > "$D/c$seed.out" 2>&1
        total=$((total + CASES))
        nd=$(diff "$D/c$seed.out" "$D/c$seed.exp" | grep -c '^<')
        if [ "$nd" != 0 ]; then
            echo "DIFF seed=$seed level=$lvl: $nd lines"; diff "$D/c$seed.out" "$D/c$seed.exp" | head -6
            bad=$((bad + nd))
        fi
    done
done
echo "fmtspec cross: $total cases, $bad differences"
[ "$bad" = 0 ]
