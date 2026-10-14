#!/usr/bin/env bash
# tools/mc/raster/run.sh -- cross-check of lib/mc/mc_raster.fi against an independent python rasterizer
# (tools/mc/raster/ref.py): 14 scenes (cube, stairs, greedy floor with repeating quads, fog, depth in both
# draw orders, near clipping, cut-out leaves, culling from inside a block, 3 random triangle soups) must be
# BYTE identical (framebuffer + SHA-256 + the PNG/PPM files). Built in 3 stages (release / --no-opt /
# dev-fast), all three must agree. Counter-check: a damaged top-left rule must be detected. Prints timings.
#
# Usage: bash tools/mc/raster/run.sh [firnc-binary]
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    mkdir -p "$W/$name"
    if ! $FIRNC $stage -o "$W/dump.$name" tools/mc/raster/dump.fi 2>"$W/err"; then
        echo "  FAIL  $name: dump.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    if ! "$W/dump.$name" "$W/$name" 2>"$W/stderr"; then
        echo "  FAIL  $name: dump crashed"; head -5 "$W/stderr"; ERR=1; continue
    fi
    if [ "$name" = "release-fast" ]; then
        if python3 -I tools/mc/raster/ref.py "$W/$name" > "$W/out.txt" 2>&1; then
            cat "$W/out.txt"
        else
            echo "  FAIL  python reference differs:"; grep -v "^  ok" "$W/out.txt" | head -20; ERR=1
        fi
    else
        cmp -s "$W/$name/scenes.txt" "$W/release-fast/scenes.txt" || { echo "  FAIL  $name: scenes.txt differs from release-fast"; ERR=1; }
        for f in "$W/release-fast"/*.rgba; do
            cmp -s "$f" "$W/$name/$(basename "$f")" || { echo "  FAIL  $name: $(basename "$f") differs from release-fast"; ERR=1; }
        done
    fi
done
# counter-check: the fill rule is damaged (edge expansion of the opaque pass removed, every edge counts as "top-left")
# -> the coverage of pixel centres next to / on shared edges changes -> the python renderer must see a difference
mkdir -p "$W/badlib" "$W/bad" && cp -r lib/. "$W/badlib/"
sed -i 's/if e1dy < 0 as i64 || (e1dy == 0 as i64 \&\& e1dx > 0 as i64) { bias1 = 1 }/bias1 = 1/' "$W/badlib/mc/mc_raster.fi"
sed -i 's/if (\*r)\.edge_exp \&\& !(\*r)\.trans {/if false {/' "$W/badlib/mc/mc_raster.fi"
sed -i 's/if e2dy < 0 as i64 || (e2dy == 0 as i64 \&\& e2dx > 0 as i64) { bias2 = 1 }/bias2 = 1/' "$W/badlib/mc/mc_raster.fi"
if cmp -s lib/mc/mc_raster.fi "$W/badlib/mc/mc_raster.fi"; then
    echo "  FAIL  counter-check: the damage did not apply"; ERR=1
elif FIRNLIB="$W/badlib" $FIRNC -o "$W/dump.bad" tools/mc/raster/dump.fi 2>/dev/null && "$W/dump.bad" "$W/bad" 2>/dev/null; then
    if python3 -I tools/mc/raster/ref.py "$W/bad" > "$W/bad.txt" 2>&1; then
        echo "  FAIL  counter-check: a damaged fill rule passed"; ERR=1
    else
        echo "  ok    counter-check: a damaged fill rule is detected ($(grep -c DIFF "$W/bad.txt") scenes differ)"
    fi
else
    echo "  FAIL  counter-check build"; ERR=1
fi
if $FIRNC -o "$W/bench" tools/mc/raster/bench.fi 2>"$W/err"; then
    "$W/bench" | sed 's/^/  /'
else
    echo "  FAIL  bench.fi does not compile"; head -5 "$W/err"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-raster: ok" || echo "RESULT mc-raster: FAIL"
exit $ERR
