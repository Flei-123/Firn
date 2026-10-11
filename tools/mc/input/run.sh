#!/usr/bin/env bash
# tools/mc/input/run.sh -- cross-check of lib/mc/mc_input.fi against an independent python implementation (ref.py):
# mouse look bit identical, 7 physics scenarios (tolerance 1e-9), 600 pick rays vs a brute force slab test, 4 selection box
# framebuffers pixel exact. 3 build stages must agree. Counter-checks: damaged pick normal / gravity / line stepper are detected.
#
# Usage: bash tools/mc/input/run.sh [firnc-binary]
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
    if ! $FIRNC $stage -o "$W/dump.$name" tools/mc/input/dump.fi 2>"$W/err"; then
        echo "  FAIL  $name: dump.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    if ! "$W/dump.$name" "$W/$name" 2>"$W/stderr"; then
        echo "  FAIL  $name: dump crashed"; head -5 "$W/stderr"; ERR=1; continue
    fi
    if [ "$name" = "release-fast" ]; then
        if python3 -I tools/mc/input/ref.py "$W/$name" > "$W/out.txt" 2>&1; then
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
# counter-checks: damaged normals of the picker, damaged gravity of the player, damaged line stepper -- python must see each
counter() { # name sed-expression
    local nm=$1 expr=$2
    rm -rf "$W/badlib" "$W/bad"; mkdir -p "$W/badlib" "$W/bad"; cp -r lib/. "$W/badlib/"
    sed -i "$expr" "$W/badlib/mc/mc_input.fi"
    if cmp -s lib/mc/mc_input.fi "$W/badlib/mc/mc_input.fi"; then
        echo "  FAIL  counter-check $nm: the damage did not apply"; ERR=1
    elif FIRNLIB="$W/badlib" $FIRNC -o "$W/dump.bad" tools/mc/input/dump.fi 2>/dev/null && "$W/dump.bad" "$W/bad" 2>/dev/null; then
        if python3 -I tools/mc/input/ref.py "$W/bad" > "$W/bad.txt" 2>&1; then
            echo "  FAIL  counter-check $nm: damage not detected"; ERR=1
        else
            echo "  ok    counter-check $nm: detected ($(grep -c DIFF "$W/bad.txt") differences)"
        fi
    else
        echo "  FAIL  counter-check $nm: build"; ERR=1
    fi
}
counter "pick normal" 's/nx = 0 - sx/nx = sx/'
counter "gravity" 's/(\*p).vy - 0.08/(*p).vy - 0.07/g'
counter "line rounding" 's/floor_div64(2 as i64 \* ddx \* i + n, 2 as i64 \* n)/floor_div64(2 as i64 * ddx * i, 2 as i64 * n)/'
if $FIRNC -o "$W/bench" tools/mc/input/bench.fi 2>"$W/err"; then
    "$W/bench" | sed 's/^/  /'
else
    echo "  FAIL  bench.fi does not compile"; head -5 "$W/err"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-input: ok" || echo "RESULT mc-input: FAIL"
exit $ERR
