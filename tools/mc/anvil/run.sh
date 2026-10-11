#!/usr/bin/env bash
# tools/mc/anvil/run.sh -- mc_anvil against an independent python reader / writer of the .mca format.
#   A  Firn writes a region (80 random writes / overwrites that grow and shrink / removals, three
#      compressions, chunk sizes around the sector boundaries, negative coordinates); python parses it
#      STRICTLY (no shared sectors, lengths inside their sectors, size a sector multiple) and the chunk
#      contents must equal python's simulation of the same scenario;
#   B  python writes the end state with its own allocator; Firn reads it: same lines;
#   C  fuzz: hundreds of damaged copies (header-biased flips, cut-off files): no crash, no hang;
#   counter-check: one flipped payload octet must be detected.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt"; do
    name=${stage:-release-fast}
    $FIRNC $stage -o "$W/prog" tools/mc/anvil/prog.fi 2>"$W/err" || { echo "  FAIL  $name: compile"; head -5 "$W/err"; ERR=1; continue; }
    seeds="1 2 3"
    [ -n "$stage" ] && seeds="1"   # the slow unoptimised stage runs one scenario
    for seed in $seeds; do
        python3 -I tools/mc/anvil/ref.py expect $seed > "$W/want.txt"
        # A: Firn writes, python parses
        "$W/prog" write "$W/a.mca" $seed || { echo "  FAIL  $name seed $seed: Firn write"; ERR=1; continue; }
        python3 -I tools/mc/anvil/ref.py read "$W/a.mca" > "$W/a.txt" 2>"$W/err" || { echo "  FAIL  $name seed $seed: python rejects the Firn file"; head -3 "$W/err"; ERR=1; continue; }
        if cmp -s "$W/a.txt" "$W/want.txt"; then
            echo "  ok    $name seed $seed A: Firn-written region parsed strictly by python: $(wc -l < "$W/want.txt") chunks identical to the simulation"
        else
            echo "  FAIL  $name seed $seed A: contents differ"; diff "$W/a.txt" "$W/want.txt" | head -4; ERR=1
        fi
        # Firn re-reads its own file
        "$W/prog" read "$W/a.mca" > "$W/a2.txt"
        cmp -s "$W/a2.txt" "$W/want.txt" || { echo "  FAIL  $name seed $seed: Firn re-read differs"; ERR=1; }
        # B: python writes, Firn reads
        python3 -I tools/mc/anvil/ref.py write "$W/b.mca" $seed
        "$W/prog" read "$W/b.mca" > "$W/b.txt" 2>&1
        if cmp -s "$W/b.txt" "$W/want.txt"; then
            echo "  ok    $name seed $seed B: python-written region read by Firn: identical"
        else
            echo "  FAIL  $name seed $seed B: differs"; diff "$W/b.txt" "$W/want.txt" | head -4; ERR=1
        fi
    done
    [ -z "$stage" ] && ls -l "$W/a.mca" | awk '{print "        region file of the last scenario: " $5 " octets"}'
    # C: fuzz (optimised stage only)
    [ -n "$stage" ] && continue
    if "$W/prog" fuzz "$W/a.mca" 300 99 > "$W/fuzz.txt" 2>&1; then
        echo "  ok    $name C: $(cat "$W/fuzz.txt")"
    else
        echo "  FAIL  $name C: fuzz run failed (rc $?)"; head -3 "$W/fuzz.txt"; ERR=1
    fi
done
# counter-check: damage the stored payload of the Firn-written file -> python lines must differ
python3 -I - "$W/a.mca" "$W/a_bad.mca" <<'PY'
import sys
raw = bytearray(open(sys.argv[1], "rb").read())
raw[-100] ^= 1
open(sys.argv[2], "wb").write(raw)
PY
python3 -I tools/mc/anvil/ref.py read "$W/a_bad.mca" > "$W/bad.txt" 2>/dev/null
if cmp -s "$W/bad.txt" "$W/want.txt"; then
    echo "  FAIL  counter-check: a damaged file compared equal"; ERR=1
else
    echo "  ok    counter-check: a damaged region file is detected"
fi
[ $ERR -eq 0 ] && echo "RESULT mc-anvil: ok" || echo "RESULT mc-anvil: FAIL"
exit $ERR
