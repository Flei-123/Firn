#!/usr/bin/env bash
# tools/mc/flat/run.sh -- cross-check of the flattening table (id << 4 | meta -> 1.13+ state id).
#
#   1. gen_flat.py regenerates lib/mc/v340/mc_flat_v340.fi; it must equal the committed file.
#   2. tools/mc/flat/dump.fi prints the table as the Firn reader sees it (three build stages).
#   3. ref_flat.cjs (Node, minecraft-data npm API, written independently of the generator) computes
#      the same 4096 numbers from the same rules; the two must be identical.
#   4. sanity: a handful of hand-known states, plus a damaged expectation must be detected.
#
# Usage: bash tools/mc/flat/run.sh [firnc-binary]
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
export NODE_PATH=${NODE_PATH:-/root/firn/tools/mcserver/node_modules}
DATA=${MCDATA:-/root/firn/tools/mcserver/node_modules/minecraft-data/minecraft-data/data}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0

if python3 -I tools/mcgen/gen_flat.py "$DATA" "$W/gen.fi" 2>"$W/gen.err"; then
    if cmp -s "$W/gen.fi" lib/mc/v340/mc_flat_v340.fi; then
        echo "  ok    gen_flat.py output == committed lib/mc/v340/mc_flat_v340.fi"
    else
        echo "  FAIL  committed mc_flat_v340.fi is stale (run tools/mcgen/gen_flat.py)"; ERR=1
    fi
else
    echo "  FAIL  gen_flat.py"; head -5 "$W/gen.err"; ERR=1
fi

node tools/mc/flat/ref_flat.cjs tools/mcgen/legacy_flat.json > "$W/want.txt" 2>"$W/node.err" || { echo "  FAIL  ref_flat.cjs"; head -5 "$W/node.err"; ERR=1; }
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/dump" tools/mc/flat/dump.fi 2>"$W/err"; then
        echo "  FAIL  $name: dump.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    "$W/dump" > "$W/got.txt" 2>/dev/null
    if cmp -s "$W/got.txt" "$W/want.txt"; then
        echo "  ok    $name: Firn table == independent JS table ($(wc -l < "$W/got.txt") states)"
    else
        echo "  FAIL  $name: table differs"; diff "$W/got.txt" "$W/want.txt" | head -6; ERR=1
    fi
done

# hand-known states of the 1.20.1 numbering (stone 1, grass_block snowy=false 9, air 0, oak_planks 15)
chk() { # legacy value, expected
    got=$(sed -n "$(($1 + 1))p" "$W/got.txt")
    if [ "$got" = "$2" ]; then :; else echo "  FAIL  legacy $1 -> $got, wanted $2"; ERR=1; fi
}
chk 0 0; chk $((1 << 4)) 1; chk $((2 << 4)) 9; chk $((5 << 4)) 15
[ $ERR -eq 0 ] && echo "  ok    spot checks (air, stone, grass_block, oak_planks)"
# counter-check: a damaged expectation is detected
sed '100s/.*/99999/' "$W/want.txt" > "$W/bad.txt"
if cmp -s "$W/got.txt" "$W/bad.txt"; then echo "  FAIL  counter-check: damaged table compared equal"; ERR=1
else echo "  ok    counter-check: a damaged expectation is detected"; fi
[ $ERR -eq 0 ] && echo "RESULT mc-flat: ok" || echo "RESULT mc-flat: FAIL"
exit $ERR
