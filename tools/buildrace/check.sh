#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib" FIRN_USE_AS=1
FIRNC="$(pwd)/${FIRNC:-compiler/target/release/firnc}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
cp tools/db/bench.fi "$W/prog.fi"
bad=0
for round in 1 2 3 4; do
    (cd "$W" && "$FIRNC" --no-cache --ownership=on prog.fi -o shared_out > /dev/null 2> e1; echo $? > r1) &
    (cd "$W" && "$FIRNC" --no-cache --ownership=off prog.fi -o shared_out > /dev/null 2> e2; echo $? > r2) &
    wait
    if [ "$(cat "$W/r1")$(cat "$W/r2")" != "00" ]; then
        bad=$((bad + 1))
        head -3 "$W/e1" "$W/e2"
    fi
done
leftovers=$(ls -A "$W" | grep -c '^\.shared_out' || true)
if [ "$bad" -eq 0 ] && [ "$leftovers" -eq 0 ]; then
    echo "  ok  parallel compiles with one output path: no collisions, no scratch leftovers"
    exit 0
fi
echo "  FAIL bad=$bad leftovers=$leftovers"
exit 1
