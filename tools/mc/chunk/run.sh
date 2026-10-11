#!/usr/bin/env bash
# tools/mc/chunk/run.sh -- offline self test of the M0 client (no network, no Java).
#
#   1. demos/mcclient/selftest.fi builds a map_chunk by hand in all three wire
#      shapes of a paletted container, decodes it and prints the dump.
#   2. tools/mc/chunk/expected.py computes the same dump independently.
#   3. The two must be byte-identical (counter-check: a damaged copy must differ).
#
# Usage: bash tools/mc/chunk/run.sh [firnc-binary]
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
    if ! $FIRNC $stage -o "$W/selftest" demos/mcclient/selftest.fi 2>"$W/err"; then
        echo "  FAIL  $name: selftest.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    "$W/selftest" > "$W/got.txt" 2>"$W/stderr"
    python3 -I tools/mc/chunk/expected.py > "$W/want.txt"
    if cmp -s "$W/got.txt" "$W/want.txt"; then
        echo "  ok    $name: chunk decoder + dump == independent python dump ($(wc -l < "$W/got.txt") lines)"
    else
        echo "  FAIL  $name: dump differs"; diff "$W/got.txt" "$W/want.txt" | head -10; ERR=1
    fi
done
# counter-check: a one-bit change in the expectation must be detected
sed 's/blocks=\([0-9a-f]\)/blocks=0\1/' "$W/want.txt" > "$W/bad.txt"
if cmp -s "$W/got.txt" "$W/bad.txt"; then
    echo "  FAIL  counter-check: a damaged dump compared equal"; ERR=1
else
    echo "  ok    counter-check: a damaged expectation is detected"
fi
[ $ERR -eq 0 ] && echo "RESULT mc-chunk: ok" || echo "RESULT mc-chunk: FAIL"
exit $ERR
