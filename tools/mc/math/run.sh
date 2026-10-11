#!/usr/bin/env bash
# tools/mc/math/run.sh -- cross-check of lib/mc/mc_math.fi against independent references.
#
#   1. tools/mc/math/dump.fi writes random cases + its own results (built in 3 stages;
#      the three stages must write byte-identical dumps).
#   2. tools/mc/math/check.py re-derives everything with numpy / brute force / sampling
#      (matrix product, transpose, inverse, projection + look_at properties, frustum planes
#      vs clip-space, boxes + spheres, voxel raycast vs slab-test-over-all-cells + sampling,
#      sweep vs brute force + no-overlap + maximality) and rejects damaged copies.
#   3. seeds "1 2 3" (MC_MATH_SEEDS overrides), 20 cases per kind (MC_MATH_CASES).
#
# Usage: bash tools/mc/math/run.sh [firnc-binary]
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
SEEDS=${MC_MATH_SEEDS:-"1 2 3"}
CASES=${MC_MATH_CASES:-20}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
first=1
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/dump" tools/mc/math/dump.fi 2>"$W/err"; then
        echo "  FAIL  $name: dump.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    for seed in $SEEDS; do
        if ! "$W/dump" "$seed" "$CASES" > "$W/dump.$name.$seed" 2>"$W/stderr"; then
            echo "  FAIL  $name seed $seed: dump crashed"; head -5 "$W/stderr"; ERR=1; continue
        fi
        if [ "$name" = "release-fast" ]; then
            python3 -I tools/mc/math/check.py "$W/dump.$name.$seed" > "$W/out.txt" 2>&1; rc=$?
            if [ $rc -ne 0 ]; then
                echo "  FAIL  seed $seed:"; head -30 "$W/out.txt"; ERR=1
            else
                echo "  ok    seed $seed: $(grep -c . "$W/dump.$name.$seed") lines agree with the references"
                [ "$seed" = "${SEEDS%% *}" ] && sed -n '/cases:/p;/counter-check/p' "$W/out.txt"
            fi
        else
            if cmp -s "$W/dump.$name.$seed" "$W/dump.release-fast.$seed"; then
                echo "  ok    $name seed $seed: dump identical to release-fast"
            else
                echo "  FAIL  $name seed $seed: dump differs from release-fast"; ERR=1
            fi
        fi
    done
done
[ $ERR -eq 0 ] && echo "RESULT mc-math: ok" || echo "RESULT mc-math: FAIL"
exit $ERR
