#!/usr/bin/env bash
# tools/mc/sky/run.sh -- cross-check of lib/mc/mc_sky.fi against an independent python implementation
# (tools/mc/sky/ref.py): 12 scenes (sun / moon / sunrise / sunset glow / rain / looking down / tick wrap / light tint)
# must be BYTE identical (framebuffer + SHA-256 + state fields + f64 bits of the sun direction). Built in 3 stages
# (release / --no-opt / dev-fast), all three must agree. Counter-check: a damaged sun path must be detected.
#
# Usage: bash tools/mc/sky/run.sh [firnc-binary]
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
    if ! $FIRNC $stage -o "$W/dump.$name" tools/mc/sky/dump.fi 2>"$W/err"; then
        echo "  FAIL  $name: dump.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    if ! "$W/dump.$name" "$W/$name" 2>"$W/stderr"; then
        echo "  FAIL  $name: dump crashed"; head -5 "$W/stderr"; ERR=1; continue
    fi
    if [ "$name" = "release-fast" ]; then
        if python3 -I tools/mc/sky/ref.py "$W/$name" > "$W/out.txt" 2>&1; then
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
# counter-check: the sun path is damaged (0.9 -> 0.8 elevation scale) -> state + image must differ in python
mkdir -p "$W/badlib" "$W/bad" && cp -r lib/. "$W/badlib/"
sed -i 's/(\*s).suny = sn \* 0.9$/(*s).suny = sn * 0.8/' "$W/badlib/mc/mc_sky.fi"
if cmp -s lib/mc/mc_sky.fi "$W/badlib/mc/mc_sky.fi"; then
    echo "  FAIL  counter-check: the damage did not apply"; ERR=1
elif FIRNLIB="$W/badlib" $FIRNC -o "$W/dump.bad" tools/mc/sky/dump.fi 2>/dev/null && "$W/dump.bad" "$W/bad" 2>/dev/null; then
    if python3 -I tools/mc/sky/ref.py "$W/bad" > "$W/bad.txt" 2>&1; then
        echo "  FAIL  counter-check: a damaged sun path passed"; ERR=1
    else
        echo "  ok    counter-check: a damaged sun path is detected ($(grep -c DIFF "$W/bad.txt") differences)"
    fi
else
    echo "  FAIL  counter-check build"; ERR=1
fi
if $FIRNC -o "$W/bench" tools/mc/sky/bench.fi 2>"$W/err"; then
    "$W/bench" | sed 's/^/  /'
else
    echo "  FAIL  bench.fi does not compile"; head -5 "$W/err"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-sky: ok" || echo "RESULT mc-sky: FAIL"
exit $ERR
